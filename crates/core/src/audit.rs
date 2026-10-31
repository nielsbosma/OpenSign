use std::collections::BTreeSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, Keys, constant_time_eq};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{Connection, PgConnection};

use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result};
use crate::storage::Storage;

const SECRET_KEYS: &[&str] = &[
    "password",
    "password_hash",
    "current_password",
    "new_password",
    "token",
    "token_hash",
    "totp_secret",
    "totp_pending_secret",
    "api_key",
    "secret",
    "code",
    "code_hash",
    "key",
    "personal_number",
];

/// Arbitrary key for the transaction-scoped lock that serializes appends to the chain.
const CHAIN_LOCK: i64 = 0x6f70_656e_7369_676e;

pub const GENESIS: [u8; 32] = [0u8; 32];

const WITNESS_HEAD: &str = "audit/anchor-head.json";
/// Entries this long past the last anchor mean anchoring has stopped; the job runs far more often.
const ANCHOR_STALE_HOURS: i64 = 3;
/// A token's time and the row's time come from different clocks, but not a day apart.
const ANCHOR_CLOCK_TOLERANCE_HOURS: i64 = 24;

/// Masks secrets by key name. A set secret shows as "••••", so the log still says that it changed.
pub fn redact(value: &Value, extra: &BTreeSet<&str>) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let secret = SECRET_KEYS.contains(&k.as_str()) || extra.contains(k.as_str());
                    let v = if secret {
                        if v.is_null() || v == &json!("") { Value::Null } else { json!("••••") }
                    } else {
                        redact(v, extra)
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| redact(v, extra)).collect()),
        other => other.clone(),
    }
}

#[derive(Default)]
pub struct Entry<'a> {
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub metadata: Option<Value>,
    pub secret_keys: &'a [&'a str],
}

impl<'a> Entry<'a> {
    pub fn new(action: &'a str, entity_type: &'a str, entity_id: impl ToString) -> Self {
        Entry { action, entity_type, entity_id: Some(entity_id.to_string()), ..Default::default() }
    }

    pub fn before(mut self, value: impl Serialize) -> Self {
        self.before = serde_json::to_value(value).ok();
        self
    }

    pub fn after(mut self, value: impl Serialize) -> Self {
        self.after = serde_json::to_value(value).ok();
        self
    }

    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = Some(value);
        self
    }
}

/// The fields a chain hash covers, in a fixed order. serde_json sorts object keys, so the same
/// row always serializes to the same bytes.
#[derive(Serialize)]
pub struct ChainedFields<'a> {
    pub occurred_at: String,
    pub actor_id: Option<String>,
    pub actor_label: &'a str,
    pub surface: &'a str,
    pub action: &'a str,
    pub entity_type: &'a str,
    pub entity_id: Option<&'a str>,
    pub before: &'a Value,
    pub after: &'a Value,
    pub metadata: &'a Value,
}

fn canonical(prev: &[u8], fields: &ChainedFields<'_>) -> Vec<u8> {
    let mut data = prev.to_vec();
    data.extend(serde_json::to_vec(&serde_json::to_value(fields).unwrap_or(Value::Null)).unwrap_or_default());
    data
}

/// HMAC-SHA256 keyed from `APP_SECRET`: without the key, a changed entry cannot be re-chained.
pub fn chain_hash(keys: &Keys, prev: &[u8], fields: &ChainedFields<'_>) -> [u8; 32] {
    keys.mac(KeyPurpose::AuditChain, &canonical(prev, fields))
}

fn timestamp(at: &DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

/// Records who did what, when, from which surface, with before and after values, chained to the
/// previous entry. Call it inside the mutating service, on the service's connection, so the entry
/// commits or rolls back with the change it describes.
pub async fn audit(ctx: &Ctx, conn: &mut PgConnection, entry: Entry<'_>) -> Result<i64> {
    let extra: BTreeSet<&str> = entry.secret_keys.iter().copied().collect();
    let before = entry.before.as_ref().map(|v| redact(v, &extra)).unwrap_or(Value::Null);
    let after = entry.after.as_ref().map(|v| redact(v, &extra)).unwrap_or(Value::Null);
    let mut metadata = match entry.metadata.as_ref().map(|v| redact(v, &extra)) {
        Some(Value::Object(map)) => map,
        Some(other) => Map::from_iter([("value".to_string(), other)]),
        None => Map::new(),
    };
    if let Some(ip) = &ctx.ip {
        metadata.insert("ip".into(), json!(ip));
    }
    if let Some(ua) = &ctx.user_agent {
        metadata.insert("user_agent".into(), json!(ua));
    }
    if let Some(rid) = &ctx.request_id {
        metadata.insert("request_id".into(), json!(rid));
    }
    let metadata = Value::Object(metadata);
    let actor_id = ctx.actor.as_ref().map(|a| a.user_id);
    let party = (ctx.surface == crate::context::Surface::Signer).then(|| metadata.get("name").and_then(Value::as_str)).flatten();
    let actor_label = match (&ctx.actor, party) {
        (Some(actor), _) => actor.email.clone(),
        (None, Some(name)) => format!("{name} (signer)"),
        (None, None) => ctx.surface.as_str().to_string(),
    };
    let occurred_at = crate::context::now();

    // A savepoint inside the caller's transaction, or a transaction of its own on a bare connection,
    // so the lock is held until the entry is written.
    let mut tx = conn.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(CHAIN_LOCK).execute(&mut *tx).await?;
    let prev: Option<Vec<u8>> = sqlx::query_scalar("select hash from audit_log order by id desc limit 1").fetch_optional(&mut *tx).await?;
    let prev = prev.unwrap_or_else(|| GENESIS.to_vec());
    let hash = chain_hash(
        ctx.keys(),
        &prev,
        &ChainedFields {
            occurred_at: timestamp(&occurred_at),
            actor_id: actor_id.map(|id| id.to_string()),
            actor_label: &actor_label,
            surface: ctx.surface.as_str(),
            action: entry.action,
            entity_type: entry.entity_type,
            entity_id: entry.entity_id.as_deref(),
            before: &before,
            after: &after,
            metadata: &metadata,
        },
    );
    let id: i64 = sqlx::query_scalar(
        "insert into audit_log (occurred_at, actor_id, actor_label, surface, action, entity_type, entity_id, before, after, metadata, prev_hash, hash, hash_alg)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'hmac-sha256') returning id",
    )
    .bind(occurred_at)
    .bind(actor_id)
    .bind(&actor_label)
    .bind(ctx.surface.as_str())
    .bind(entry.action)
    .bind(entry.entity_type)
    .bind(entry.entity_id.as_deref())
    .bind(&before)
    .bind(&after)
    .bind(&metadata)
    .bind(&prev)
    .bind(hash.as_slice())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

/// What the copy of the latest anchor kept in the bucket says about the chain in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WitnessStatus {
    /// Not looked at, because the chain itself is already broken.
    NotChecked,
    /// No anchors and no copy yet.
    None,
    /// The copy matches the latest anchor.
    Ok,
    /// The copy matches an anchor, but newer anchors have no copy.
    Behind,
    /// Anchors exist but the bucket has no copy.
    Missing,
    /// The bucket did not answer.
    Unreadable,
    /// The copy is not genuine, or the anchor or entry it names is gone or different.
    Mismatch,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct ChainReport {
    pub entries: i64,
    /// False when an entry, an anchor or the bucket copy shows the trail was changed.
    pub valid: bool,
    /// The first entry whose stored hash does not match its contents or its predecessor.
    pub broken_at: Option<i64>,
    pub head: Option<String>,
    /// Hourly time-stamped heads that no longer match the entry they were taken from.
    pub broken_anchors: Vec<i64>,
    /// Anchors with no time stamp token. OpenSign never writes these.
    pub unsigned_anchors: Vec<i64>,
    /// Anchors whose time stamp token has a bad signature, covers something else or names another time.
    pub invalid_anchors: Vec<i64>,
    pub last_anchor_at: Option<DateTime<Utc>>,
    /// A time stamp authority is set, yet entries older than a few hours have no anchor. A warning:
    /// an authority that is down causes this too.
    pub anchor_stale: bool,
    pub witness: WitnessStatus,
}

impl ChainReport {
    fn broken(entries: i64, id: i64) -> Self {
        ChainReport {
            entries,
            valid: false,
            broken_at: Some(id),
            head: None,
            broken_anchors: Vec::new(),
            unsigned_anchors: Vec::new(),
            invalid_anchors: Vec::new(),
            last_anchor_at: None,
            anchor_stale: false,
            witness: WitnessStatus::NotChecked,
        }
    }
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    occurred_at: DateTime<Utc>,
    actor_id: Option<uuid::Uuid>,
    actor_label: String,
    surface: String,
    action: String,
    entity_type: String,
    entity_id: Option<String>,
    before: Option<Value>,
    after: Option<Value>,
    metadata: Option<Value>,
    prev_hash: Vec<u8>,
    hash: Vec<u8>,
    hash_alg: String,
}

#[derive(sqlx::FromRow)]
struct AnchorRow {
    id: i64,
    head_hash: Vec<u8>,
    tsa_token: Option<Vec<u8>>,
    created_at: DateTime<Utc>,
}

/// Whether a stored token is a genuine time stamp of this head, made around when the row says.
fn anchor_token_ok(anchor: &AnchorRow, token: &[u8]) -> bool {
    opensign_pdf::verify_timestamp_token(token, &anchor.head_hash)
        .is_ok_and(|time| (time - anchor.created_at).abs() <= Duration::hours(ANCHOR_CLOCK_TOLERANCE_HOURS))
}

/// Recomputes the whole chain with the audit key, and checks the time-stamped anchors against it.
/// Only keyed entries count: an unkeyed one could have been written by anyone.
pub async fn verify_chain(conn: &mut PgConnection, keys: &Keys) -> Result<ChainReport> {
    let mut expected_prev = GENESIS.to_vec();
    let mut entries = 0;
    let mut last_id = 0i64;
    loop {
        let rows: Vec<Row> =
            sqlx::query_as("select * from audit_log where id > $1 order by id limit 1000").bind(last_id).fetch_all(&mut *conn).await?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            entries += 1;
            last_id = row.id;
            let null = Value::Null;
            let fields = ChainedFields {
                occurred_at: timestamp(&row.occurred_at),
                actor_id: row.actor_id.map(|id| id.to_string()),
                actor_label: &row.actor_label,
                surface: &row.surface,
                action: &row.action,
                entity_type: &row.entity_type,
                entity_id: row.entity_id.as_deref(),
                before: row.before.as_ref().unwrap_or(&null),
                after: row.after.as_ref().unwrap_or(&null),
                metadata: row.metadata.as_ref().unwrap_or(&null),
            };
            let keyed = row.hash_alg == "hmac-sha256";
            if !keyed || row.prev_hash != expected_prev || row.hash != chain_hash(keys, &expected_prev, &fields) {
                return Ok(ChainReport::broken(entries, row.id));
            }
            expected_prev = row.hash;
        }
    }
    let broken_anchors: Vec<i64> = sqlx::query_scalar(
        "select a.id from audit_anchors a left join audit_log l on l.id = a.audit_id where l.hash is distinct from a.head_hash order by a.id",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut unsigned_anchors = Vec::new();
    let mut invalid_anchors = Vec::new();
    let mut last_anchor_at = None;
    let mut last_anchor = 0i64;
    loop {
        let anchors: Vec<AnchorRow> =
            sqlx::query_as("select id, head_hash, tsa_token, created_at from audit_anchors where id > $1 order by id limit 200")
                .bind(last_anchor)
                .fetch_all(&mut *conn)
                .await?;
        let Some(last) = anchors.last() else { break };
        last_anchor = last.id;
        last_anchor_at = Some(last.created_at);
        let (unsigned, invalid) = tokio::task::spawn_blocking(move || {
            let unsigned: Vec<i64> = anchors.iter().filter(|a| a.tsa_token.is_none()).map(|a| a.id).collect();
            let invalid: Vec<i64> =
                anchors.iter().filter(|a| a.tsa_token.as_deref().is_some_and(|t| !anchor_token_ok(a, t))).map(|a| a.id).collect();
            (unsigned, invalid)
        })
        .await
        .map_err(AppError::internal)?;
        unsigned_anchors.extend(unsigned);
        invalid_anchors.extend(invalid);
    }
    Ok(ChainReport {
        entries,
        valid: broken_anchors.is_empty() && unsigned_anchors.is_empty() && invalid_anchors.is_empty(),
        broken_at: None,
        head: (entries > 0).then(|| hex::encode(&expected_prev)),
        broken_anchors,
        unsigned_anchors,
        invalid_anchors,
        last_anchor_at,
        anchor_stale: false,
        witness: WitnessStatus::NotChecked,
    })
}

/// A copy of an anchor outside the database. Someone who holds only the database cannot move it,
/// so removing the end of the trail, or the anchors themselves, shows.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Witness {
    anchor_id: i64,
    audit_id: i64,
    head_hash: String,
    token: String,
    mac: String,
}

fn witness_mac(keys: &Keys, anchor_id: i64, audit_id: i64, head_hash: &str, token: &str) -> String {
    let message = format!("audit-anchor-witness\0{anchor_id}\0{audit_id}\0{head_hash}\0{token}");
    hex::encode(keys.mac(KeyPurpose::AuditChain, message.as_bytes()))
}

/// Writes the copy of an anchor, under its own name and as the latest one.
pub async fn write_witness(storage: &Storage, keys: &Keys, anchor_id: i64, audit_id: i64, head: &[u8], token: &[u8]) -> Result<()> {
    let head_hash = hex::encode(head);
    let token = B64.encode(token);
    let mac = witness_mac(keys, anchor_id, audit_id, &head_hash, &token);
    let body = serde_json::to_vec(&Witness { anchor_id, audit_id, head_hash, token, mac }).map_err(AppError::internal)?;
    storage.put_raw(&format!("audit/anchors/{anchor_id}.json"), body.clone()).await?;
    storage.put_raw(WITNESS_HEAD, body).await
}

async fn check_witness(conn: &mut PgConnection, storage: &Storage, keys: &Keys) -> Result<WitnessStatus> {
    let latest: Option<i64> = sqlx::query_scalar("select max(id) from audit_anchors").fetch_one(&mut *conn).await?;
    let body = match storage.get_raw(WITNESS_HEAD).await {
        Ok(body) => body,
        Err(e) if e.kind == ErrorKind::NotFound => {
            return Ok(if latest.is_some() { WitnessStatus::Missing } else { WitnessStatus::None });
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not read the audit anchor copy from the bucket");
            return Ok(WitnessStatus::Unreadable);
        }
    };
    let Ok(witness) = serde_json::from_slice::<Witness>(&body) else { return Ok(WitnessStatus::Mismatch) };
    let expected = witness_mac(keys, witness.anchor_id, witness.audit_id, &witness.head_hash, &witness.token);
    if !constant_time_eq(expected.as_bytes(), witness.mac.as_bytes()) {
        return Ok(WitnessStatus::Mismatch);
    }
    let Ok(head) = hex::decode(&witness.head_hash) else { return Ok(WitnessStatus::Mismatch) };
    let matches: bool = sqlx::query_scalar(
        "select exists (select 1 from audit_anchors a join audit_log l on l.id = a.audit_id
         where a.id = $1 and a.audit_id = $2 and a.head_hash = $3 and l.hash = $3)",
    )
    .bind(witness.anchor_id)
    .bind(witness.audit_id)
    .bind(&head)
    .fetch_one(&mut *conn)
    .await?;
    if !matches {
        return Ok(WitnessStatus::Mismatch);
    }
    Ok(if latest == Some(witness.anchor_id) { WitnessStatus::Ok } else { WitnessStatus::Behind })
}

/// The full check behind `audit.verify`: the chain and anchors, whether anchoring has stopped, and
/// the copy of the latest anchor in the bucket.
pub async fn verify_trail(ctx: &Ctx, conn: &mut PgConnection) -> Result<ChainReport> {
    let mut report = verify_chain(conn, ctx.keys()).await?;
    if report.broken_at.is_some() {
        return Ok(report);
    }
    let signing: crate::settings::SigningSettings = crate::services::settings(ctx, conn).await?;
    if !signing.tsa_url.trim().is_empty() {
        report.anchor_stale = sqlx::query_scalar(
            "select exists (select 1 from audit_log where occurred_at < $1
             and id > coalesce((select max(audit_id) from audit_anchors), 0))",
        )
        .bind(ctx.now - Duration::hours(ANCHOR_STALE_HOURS))
        .fetch_one(&mut *conn)
        .await?;
    }
    report.witness = check_witness(conn, &ctx.app.storage, ctx.keys()).await?;
    report.valid = report.valid && report.witness != WitnessStatus::Mismatch;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secrets_at_any_depth() {
        let value =
            json!({ "name": "A", "password": "x", "nested": { "api_key": "k", "empty": null, "token": "" }, "list": [{ "secret": "s" }] });
        let extra = BTreeSet::from(["empty"]);
        assert_eq!(
            redact(&value, &extra),
            json!({ "name": "A", "password": "••••", "nested": { "api_key": "••••", "empty": null, "token": null }, "list": [{ "secret": "••••" }] })
        );
    }

    #[test]
    fn hash_depends_on_every_field_and_the_predecessor() {
        let null = Value::Null;
        let fields = |action| ChainedFields {
            occurred_at: "2026-10-28T10:00:00.000000Z".into(),
            actor_id: None,
            actor_label: "system",
            surface: "system",
            action,
            entity_type: "x",
            entity_id: None,
            before: &null,
            after: &null,
            metadata: &null,
        };
        let keys = Keys::derive("test-secret-test-secret-test-secret-0123").unwrap_or_else(|_| unreachable!());
        let other = Keys::derive("another-secret-another-secret-another-0").unwrap_or_else(|_| unreachable!());
        let a = chain_hash(&keys, &GENESIS, &fields("a.b"));
        assert_eq!(a, chain_hash(&keys, &GENESIS, &fields("a.b")));
        assert_ne!(a, chain_hash(&keys, &GENESIS, &fields("a.c")));
        assert_ne!(a, chain_hash(&keys, &[1u8; 32], &fields("a.b")));
        assert_ne!(a, chain_hash(&other, &GENESIS, &fields("a.b")));
    }
}
