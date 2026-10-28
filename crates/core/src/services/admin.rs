use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{Empty, Ok_, settings};
use crate::audit::{ChainReport, Entry, audit, verify_chain};
use crate::context::Ctx;
use crate::email::{self, Message};
use crate::error::{AppError, Result, bad_request};
use crate::permissions::Permission;
use crate::service::{Access, Meta, Service};
use crate::settings::{
    AiSettings, EidSettings, EmailSettings, OrgSettings, RetentionSettings, SecuritySettings, SettingsGroup, SigningSettings, SmsSettings,
    public_view,
};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Org,
    Email,
    Security,
    Retention,
    Signing,
    Ai,
    Sms,
    Eid,
}

#[derive(Deserialize, JsonSchema)]
pub struct GroupInput {
    pub group: Group,
}

#[derive(Serialize, JsonSchema)]
pub struct GroupView {
    /// Secret fields show `true` when set; their values are never returned.
    pub values: Value,
    /// Fields fixed by OPENSIGN_* environment variables.
    pub pinned: Vec<String>,
}

async fn view<G: SettingsGroup>(ctx: &Ctx, conn: &mut PgConnection) -> Result<GroupView> {
    let group: G = settings(ctx, conn).await?;
    Ok(GroupView { values: public_view(&group), pinned: ctx.app.settings.pinned::<G>() })
}

pub struct GetSettings;

#[async_trait]
impl Service for GetSettings {
    type Input = GroupInput;
    type Output = GroupView;
    const META: Meta = Meta::query(
        "settings.get",
        "Read one group of Admin settings. Secrets show only whether they are set.",
        Access::Permission(Permission::SettingsManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: GroupInput) -> Result<GroupView> {
        match input.group {
            Group::Org => view::<OrgSettings>(ctx, conn).await,
            Group::Email => view::<EmailSettings>(ctx, conn).await,
            Group::Security => view::<SecuritySettings>(ctx, conn).await,
            Group::Retention => view::<RetentionSettings>(ctx, conn).await,
            Group::Signing => view::<SigningSettings>(ctx, conn).await,
            Group::Ai => view::<AiSettings>(ctx, conn).await,
            Group::Sms => view::<SmsSettings>(ctx, conn).await,
            Group::Eid => view::<EidSettings>(ctx, conn).await,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateInput {
    pub group: Group,
    /// The fields to change. Leave a secret out (or send null) to keep it; send "" to clear it.
    pub values: Value,
}

fn merge(current: Value, patch: &Value, secrets: &[&str]) -> Value {
    let mut merged = current;
    if let (Value::Object(target), Value::Object(changes)) = (&mut merged, patch) {
        for (k, v) in changes {
            if secrets.contains(&k.as_str()) && (v.is_null() || v.is_boolean()) {
                continue;
            }
            target.insert(k.clone(), v.clone());
        }
    }
    merged
}

async fn update<G: SettingsGroup>(ctx: &Ctx, conn: &mut PgConnection, patch: &Value) -> Result<GroupView> {
    let pinned = ctx.app.settings.pinned::<G>();
    if let Value::Object(changes) = patch
        && let Some(field) = changes.keys().find(|k| pinned.contains(k))
    {
        return Err(bad_request(format!("{field} is fixed by an environment variable")));
    }
    let current: G = settings(ctx, conn).await?;
    let before = public_view(&current);
    let mut base = serde_json::to_value(&current).map_err(AppError::internal)?;
    if let Value::Object(map) = &mut base {
        for secret in G::SECRETS {
            map.insert(secret.to_string(), Value::Null);
        }
    }
    let mut patch = patch.clone();
    if G::NAME == "email"
        && let Value::Object(changes) = &mut patch
    {
        // Only a successful test email marks email as working; a new key or sender needs a new test.
        changes.remove("verifiedAt");
        let changed = |field: &str| changes.get(field).is_some_and(|v| !v.is_null() && !v.is_boolean() && Some(v) != base.get(field));
        if changed("apiKey") || changed("fromAddress") {
            changes.insert("verifiedAt".into(), Value::Null);
        }
    }
    if G::NAME == "eid"
        && let Value::Object(changes) = &mut patch
    {
        // A .p12/.pfx arrives as base64 with its password; only the PEM it unlocks is stored.
        for (field, password_field) in
            [("bankidCertificate", "bankidCertificatePassword"), ("frejaCertificate", "frejaCertificatePassword")]
        {
            let password = changes.remove(password_field);
            let Some(Value::String(upload)) = changes.get(field) else { continue };
            if upload.is_empty() || upload.trim_start().starts_with("-----BEGIN") {
                continue;
            }
            let der = B64.decode(upload.trim()).map_err(|_| bad_request("The certificate file is neither PEM nor PKCS#12"))?;
            let password = password.as_ref().and_then(Value::as_str).unwrap_or_default();
            let pem = crate::eid::pkcs12_to_pem(&der, password).map_err(bad_request)?;
            changes.insert(field.into(), Value::String(pem));
        }
    }
    let next: G = serde_json::from_value(merge(base, &patch, G::SECRETS)).map_err(|e| bad_request(format!("Invalid settings: {e}")))?;
    validate::<G>(&next)?;
    ctx.app.settings.save(conn, ctx.keys(), &next, Some(ctx.actor()?.user_id)).await?;
    let saved: G = settings(ctx, conn).await?;
    let after = public_view(&saved);
    audit(
        ctx,
        conn,
        Entry {
            action: "settings.update",
            entity_type: "settings",
            entity_id: Some(G::NAME.into()),
            before: Some(before),
            after: Some(after.clone()),
            metadata: None,
            secret_keys: G::SECRETS,
        },
    )
    .await?;
    Ok(GroupView { values: after, pinned })
}

fn validate<G: SettingsGroup>(group: &G) -> Result<()> {
    let value = serde_json::to_value(group).map_err(AppError::internal)?;
    let url = |field: &str| -> Result<()> {
        if let Some(u) = value.get(field).and_then(Value::as_str).filter(|u| !u.is_empty()) {
            crate::net::check_outbound_url(u, G::NAME == "signing").map_err(bad_request)?;
        }
        Ok(())
    };
    match G::NAME {
        "signing" => url("tsaUrl"),
        // A LiteLLM or other self-hosted proxy usually runs next to OpenSign on the private network.
        "ai" if matches!(value["provider"].as_str(), Some("litellm" | "openai-compatible")) => {
            match value["baseUrl"].as_str().filter(|u| !u.is_empty()) {
                Some(u) => match reqwest::Url::parse(u) {
                    Ok(parsed) if matches!(parsed.scheme(), "http" | "https") && parsed.username().is_empty() => Ok(()),
                    _ => Err(bad_request("Enter the proxy's http or https address")),
                },
                None => Err(bad_request("Enter the address of the OpenAI-compatible endpoint")),
            }
        }
        "ai" => url("baseUrl"),
        "eid" => {
            url("brokerIssuer")?;
            let eid: EidSettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            let present = |s: &str| !s.trim().is_empty();
            for (name, ca) in [("BankID server CA", &eid.bankid_ca_certificate), ("Freja server CA", &eid.freja_ca_certificate)] {
                if present(ca) && reqwest::Certificate::from_pem_bundle(ca.as_bytes()).map_or(true, |c| c.is_empty()) {
                    return Err(bad_request(format!("The {name} must be a PEM certificate")));
                }
            }
            for (name, identity) in [("BankID", &eid.bankid_certificate), ("Freja eID", &eid.freja_certificate)] {
                if let Some(pem) = identity.as_deref().filter(|p| present(p)) {
                    crate::net::client_with_certificate(pem, None).map_err(|e| bad_request(format!("{name}: {e}")))?;
                }
            }
            if present(&eid.freja_signing_certificate) {
                crate::eid::jws::Key::from_certificate_pem(&eid.freja_signing_certificate)
                    .map_err(|e| bad_request(format!("Freja's signing certificate is not usable: {e}")))?;
            }
            Ok(())
        }
        "retention" => {
            let rules: RetentionSettings = serde_json::from_value(value.clone()).map_err(AppError::internal)?;
            if [rules.completed_days, rules.ended_days, rules.draft_days].iter().any(|d| *d > 36_500) {
                return Err(bad_request("Keep retention periods under 100 years"));
            }
            Ok(())
        }
        "email" => {
            if let Some(from) = value.get("fromAddress").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                crate::auth::normalize_email(from)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub struct UpdateSettings;

#[async_trait]
impl Service for UpdateSettings {
    type Input = UpdateInput;
    type Output = GroupView;
    const META: Meta = Meta::mutation(
        "settings.update",
        "Change Admin settings in one group. Every change is audited with secrets masked.",
        Access::Permission(Permission::SettingsManage),
    )
    .not_a_tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<GroupView> {
        if !input.values.is_object() {
            return Err(bad_request("values must be an object"));
        }
        match input.group {
            Group::Org => update::<OrgSettings>(ctx, conn, &input.values).await,
            Group::Email => {
                let mut patch = input.values.clone();
                if let Value::Object(map) = &mut patch {
                    map.remove("verifiedAt");
                    map.insert("verifiedAt".into(), Value::Null);
                }
                update::<EmailSettings>(ctx, conn, &patch).await
            }
            Group::Security => update::<SecuritySettings>(ctx, conn, &input.values).await,
            Group::Retention => update::<RetentionSettings>(ctx, conn, &input.values).await,
            Group::Signing => update::<SigningSettings>(ctx, conn, &input.values).await,
            Group::Ai => update::<AiSettings>(ctx, conn, &input.values).await,
            Group::Sms => update::<SmsSettings>(ctx, conn, &input.values).await,
            Group::Eid => update::<EidSettings>(ctx, conn, &input.values).await,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct TestEmailInput {
    /// Defaults to your own address.
    pub to: Option<String>,
}

pub struct SendTestEmail;

#[async_trait]
impl Service for SendTestEmail {
    type Input = TestEmailInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "settings.send_test_email",
        "Send a test email through Resend. A successful test marks email as set up.",
        Access::Permission(Permission::SettingsManage),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TestEmailInput) -> Result<Ok_> {
        let to = match input.to {
            Some(to) => crate::auth::normalize_email(&to)?,
            None => ctx.actor()?.email.clone(),
        };
        let mut email_settings: EmailSettings = settings(ctx, conn).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        email::send(
            &ctx.app,
            &email_settings,
            &org,
            &Message {
                to: to.clone(),
                to_name: None,
                subject: "OpenSign test email".into(),
                paragraphs: vec!["Email from OpenSign works. Signing invitations and notifications will be sent like this.".into()],
                button: None,
                footer: None,
                attachments: vec![],
            },
        )
        .await?;
        email_settings.verified_at = Some(ctx.now);
        email_settings.api_key = None;
        ctx.app.settings.save(conn, ctx.keys(), &email_settings, Some(ctx.actor()?.user_id)).await?;
        audit(ctx, conn, Entry::new("settings.email_verified", "settings", "email").metadata(serde_json::json!({ "to": to }))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListAuditInput {
    /// Return entries older than this id (for paging).
    pub before_id: Option<i64>,
    pub limit: Option<i64>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub action: Option<String>,
    pub actor_id: Option<Uuid>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub actor_id: Option<Uuid>,
    pub actor_label: String,
    pub surface: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub metadata: Option<Value>,
    #[schemars(with = "String")]
    #[serde(serialize_with = "hex_bytes")]
    pub hash: Vec<u8>,
}

fn hex_bytes<S: serde::Serializer>(bytes: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
    s.serialize_str(&hex::encode(bytes))
}

pub struct ListAudit;

#[async_trait]
impl Service for ListAudit {
    type Input = ListAuditInput;
    type Output = Vec<AuditView>;
    const META: Meta = Meta::query(
        "audit.list",
        "Browse the audit log, newest first. Filter by entity, action or actor.",
        Access::Permission(Permission::AuditRead),
    );

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, input: ListAuditInput) -> Result<Vec<AuditView>> {
        Ok(sqlx::query_as(
            "select id, occurred_at, actor_id, actor_label, surface, action, entity_type, entity_id, before, after, metadata, hash from audit_log
             where ($1::bigint is null or id < $1) and ($2::text is null or entity_type = $2) and ($3::text is null or entity_id = $3)
               and ($4::text is null or action = $4) and ($5::uuid is null or actor_id = $5)
             order by id desc limit $6",
        )
        .bind(input.before_id)
        .bind(input.entity_type)
        .bind(input.entity_id)
        .bind(input.action)
        .bind(input.actor_id)
        .bind(input.limit.unwrap_or(50).clamp(1, 500))
        .fetch_all(&mut *conn)
        .await?)
    }
}

pub struct VerifyAudit;

#[async_trait]
impl Service for VerifyAudit {
    type Input = Empty;
    type Output = ChainReport;
    const META: Meta = Meta::query(
        "audit.verify",
        "Recompute the audit log's hash chain and report the first entry that was changed or removed, if any.",
        Access::Permission(Permission::AuditRead),
    );

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<ChainReport> {
        verify_chain(conn, ctx.keys()).await
    }
}
