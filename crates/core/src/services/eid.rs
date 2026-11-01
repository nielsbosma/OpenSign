//! Identifying a party with an eID through their signing link. The browser only ever learns the
//! eID session's id; provider secrets stay encrypted on the server, and the signer session is
//! handed out once, to a caller that also holds the signing link.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Utc};
use opensign_crypto::{KeyPurpose, constant_time_eq, random_bytes, random_token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::settings;
use super::signing::{Resolved, SESSION_MINUTES, issue_session, resolve, satisfied};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::{AuthMethod, DOCUMENT_COLUMNS, Delivery, DocumentRow, PARTY_COLUMNS, PartyRow, required_personal_number, status};
use crate::eid::EidText;
use crate::eid::bankid::{self, BankId, Collected};
use crate::eid::freja::{self, Freja, Outcome, User};
use crate::eid::{Identity, bankid_via_tic, oidc, same_person, se_personal_number, tic};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, not_found, too_many};
use crate::service::{Access, Meta, Service};
use crate::settings::{EidSettings, OrgSettings, SecuritySettings};

const STARTS_PER_HOUR: i64 = 10;
const CONTINUE_TICKET: &str = "eid-continue";

fn secrets_context(id: Uuid) -> String {
    format!("eid:{id}:secrets")
}

fn evidence_context(id: Uuid) -> String {
    format!("eid:{id}:evidence")
}

pub fn state_mac(ctx: &Ctx, state: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("eid-state:{state}").as_bytes()))
}

fn browser_mac(ctx: &Ctx, binding: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("eid-browser:{binding}").as_bytes()))
}

fn release_mac(ctx: &Ctx, id: Uuid, code: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("eid-release:{id}:{code}").as_bytes()))
}

fn number_context(id: Uuid) -> String {
    format!("eid:{id}:personal_number")
}

/// The proven identity as stored on the eID session, with the personal number encrypted.
fn seal_identity(ctx: &Ctx, id: Uuid, identity: &Identity) -> Value {
    json!({
        "name": identity.name,
        "personalNumber": identity.personal_number.as_deref().map(|n| ctx.keys().encrypt_str(KeyPurpose::Settings, &number_context(id), n)),
    })
}

fn open_identity(ctx: &Ctx, id: Uuid, stored: &Value) -> Option<Identity> {
    let personal_number = match stored["personalNumber"].as_str() {
        Some(sealed) => Some(ctx.keys().decrypt_str(KeyPurpose::Settings, &number_context(id), sealed).ok()?.to_string()),
        None => None,
    };
    Some(Identity { name: stored["name"].as_str()?.to_string(), personal_number })
}

fn redirect_uri(ctx: &Ctx) -> String {
    format!("{}/api/eid/callback", ctx.app.public_url())
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    /// Before seeing the document.
    View,
    /// Before signing it.
    Sign,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Purpose::View => "view",
            Purpose::Sign => "sign",
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartInput {
    pub token: String,
    pub session: Option<String>,
    pub purpose: Purpose,
    /// The signing page runs in a frame on another site, where this browser's cookies for OpenSign
    /// do not stick. `redirectUrl` then leads to a page on OpenSign that takes over in its own window.
    #[serde(default)]
    pub framed: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    pub id: Uuid,
    pub method: AuthMethod,
    /// Opens the BankID app on this device: `bankid:///?autostarttoken=...&redirect=null`.
    pub auto_start_token: Option<String>,
    /// Content for a QR code to scan with a phone. For BankID it changes every second; poll for it.
    pub qr: Option<String>,
    /// Send the browser here to identify at the eID broker.
    pub redirect_url: Option<String>,
    pub expires_at: DateTime<Utc>,
    /// Becomes a cookie in the RPC layer and is removed from the answer: the eID broker's answer
    /// counts only in the browser that holds it.
    pub browser_binding: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Secrets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    qr_start_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    qr_start_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorize_url: Option<String>,
}

fn method_for(party: &PartyRow, purpose: Purpose) -> AuthMethod {
    match purpose {
        Purpose::View => party.auth_to_view(),
        Purpose::Sign => party.auth_to_sign(),
    }
}

fn provider(method: AuthMethod) -> Result<&'static str> {
    match method {
        AuthMethod::SeBankid => Ok("se_bankid"),
        AuthMethod::Freja => Ok("freja"),
        AuthMethod::Broker => Ok("broker"),
        _ => Err(bad_request("This party does not identify with an eID")),
    }
}

fn method_of(provider: &str) -> AuthMethod {
    match provider {
        "se_bankid" | "tic" => AuthMethod::SeBankid,
        "freja" => AuthMethod::Freja,
        _ => AuthMethod::Broker,
    }
}

fn oidc_provider(ctx: &Ctx, eid: &EidSettings, provider: &str) -> Result<oidc::Provider> {
    if provider == "tic" {
        return tic::provider(eid, &ctx.app.http.public);
    }
    if !eid.broker_enabled {
        return Err(bad_request("eID is not set up. An admin enables it in Admin settings."));
    }
    Ok(oidc::Provider {
        http: ctx.app.http.public.clone(),
        issuer: eid.broker_issuer.clone(),
        client_id: eid.broker_client_id.clone(),
        client_secret: eid.broker_client_secret.clone().unwrap_or_default(),
        scope: "openid".into(),
        acr_values: eid.broker_acr_values.clone(),
        require_public: true,
    })
}

fn swedish(expected: Option<&str>, this_year: i32) -> Result<Option<String>> {
    expected
        .map(|n| {
            se_personal_number(n, this_year).ok_or_else(|| {
                bad_request(
                    "The personal number on this party is not a Swedish one, so BankID cannot be used. Ask the sender to correct it.",
                )
            })
        })
        .transpose()
}

pub struct Start;

#[async_trait]
impl Service for Start {
    type Input = StartInput;
    type Output = Started;
    const META: Meta =
        Meta::mutation("signing.eid_start", "Start identifying the party with the eID their document asks for.", Access::Public)
            .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: StartInput) -> Result<Started> {
        let Resolved { party, document, session } = resolve(ctx, conn, &input.token, input.session.as_deref()).await?;
        let method = method_for(&party, input.purpose);
        let provider = provider(method)?;
        if input.purpose == Purpose::Sign {
            if document.status != status::PENDING {
                return Err(conflict("This document is no longer waiting for signatures."));
            }
            if !satisfied(party.auth_to_view(), session.as_ref()) {
                return Err(AppError::new(
                    ErrorKind::Unauthorized,
                    format!("Identify yourself with {} first", party.auth_to_view().label()),
                ));
            }
        }
        let recent: i64 = sqlx::query_scalar("select count(*) from eid_sessions where party_id = $1 and created_at > $2")
            .bind(party.id)
            .bind(ctx.now - Duration::hours(1))
            .fetch_one(&mut *conn)
            .await?;
        if recent >= STARTS_PER_HOUR {
            return Err(too_many("Too many identification attempts. Try again in an hour."));
        }
        let eid: EidSettings = settings(ctx, conn).await?;
        let provider = if provider == "se_bankid" && bankid_via_tic(&eid) { "tic" } else { provider };
        let expected = required_personal_number(ctx.keys(), &party)?;
        let id = Uuid::now_v7();
        let mut secrets = Secrets { qr_start_token: None, qr_start_secret: None, nonce: None, verifier: None, authorize_url: None };
        let mut state_mac_value = None;
        let mut browser_mac_value = None;
        let mut framed = false;
        let (reference, started) = match provider {
            "se_bankid" => {
                let number = swedish(expected.as_deref(), ctx.now.year())?;
                let org: OrgSettings = settings(ctx, conn).await?;
                let organisation = if org.name.is_empty() { "OpenSign".to_string() } else { org.name };
                let sign = input.purpose == Purpose::Sign;
                let text = if sign {
                    format!("I sign the document \"{}\" sent by {organisation}.", document.title)
                } else {
                    format!("I identify myself to read the document \"{}\" sent by {organisation}.", document.title)
                };
                let ip = ctx.ip.clone().unwrap_or_else(|| "127.0.0.1".into());
                let order = BankId::from_settings(&eid)?.start(&ip, number.as_deref(), &text, sign).await?;
                let qr = bankid::qr_code(&order.qr_start_token, &order.qr_start_secret, 0);
                secrets.qr_start_token = Some(order.qr_start_token);
                secrets.qr_start_secret = Some(order.qr_start_secret);
                let started = Started {
                    id,
                    method,
                    auto_start_token: Some(order.auto_start_token),
                    qr: Some(qr),
                    redirect_url: None,
                    expires_at: ctx.now + Duration::minutes(3),
                    browser_binding: None,
                };
                (Some(order.order_ref), started)
            }
            "freja" => {
                let ssn = expected.as_deref().and_then(|n| se_personal_number(n, ctx.now.year()));
                let user = match (&ssn, &party.email) {
                    (Some(ssn), _) => User::Ssn(ssn),
                    (None, Some(email)) => User::Email(email),
                    (None, None) => User::Inferred,
                };
                let inferred = matches!(user, User::Inferred);
                let auth_ref = Freja::from_settings(&eid)?.start(user).await?;
                let started = Started {
                    id,
                    method,
                    auto_start_token: None,
                    qr: inferred.then(|| freja::qr_code(&auth_ref)),
                    redirect_url: None,
                    expires_at: ctx.now + Duration::minutes(3),
                    browser_binding: None,
                };
                (Some(auth_ref), started)
            }
            _ => {
                if provider == "tic" {
                    swedish(expected.as_deref(), ctx.now.year())?;
                }
                let oidc = oidc_provider(ctx, &eid, provider)?;
                let discovery = oidc.discover().await?;
                let state = random_token();
                let nonce = random_token();
                let verifier = random_token();
                let redirect = redirect_uri(ctx);
                let url = oidc::authorize_url(
                    &discovery,
                    &oidc::AuthorizeRequest {
                        client_id: &oidc.client_id,
                        redirect_uri: &redirect,
                        state: &state,
                        nonce: &nonce,
                        verifier: &verifier,
                        scope: &oidc.scope,
                        acr_values: &oidc.acr_values,
                        login_hint: None,
                        prompt: Some("login"),
                    },
                )?;
                state_mac_value = Some(state_mac(ctx, &state));
                secrets.nonce = Some(nonce);
                secrets.verifier = Some(verifier);
                let expires_at = ctx.now + Duration::minutes(10);
                let (redirect_url, browser_binding) = if input.framed {
                    let security: SecuritySettings = settings(ctx, conn).await?;
                    if party.delivery != Delivery::Link.as_str() || security.embed_origins().is_empty() {
                        return Err(bad_request("This signing link cannot identify from inside a frame. Open it on its own."));
                    }
                    secrets.authorize_url = Some(url);
                    framed = true;
                    let ticket = ContinueTicket { s: id, p: party.id, v: party.link_version };
                    let ticket = ctx.keys().sign_token(CONTINUE_TICKET, &ticket, (expires_at - ctx.now).num_seconds(), ctx.now.timestamp());
                    (format!("{}/sign/eid#{ticket}", ctx.app.public_url()), None)
                } else {
                    let binding = random_token();
                    browser_mac_value = Some(browser_mac(ctx, &binding));
                    (url, Some(binding))
                };
                let started =
                    Started { id, method, auto_start_token: None, qr: None, redirect_url: Some(redirect_url), expires_at, browser_binding };
                (None, started)
            }
        };
        let sealed = ctx.keys().encrypt_str(
            KeyPurpose::Settings,
            &secrets_context(id),
            &serde_json::to_string(&secrets).map_err(AppError::internal)?,
        );
        sqlx::query(
            "insert into eid_sessions (id, party_id, provider, purpose, reference, state_mac, secrets, expires_at, created_at, browser_mac, framed)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(id)
        .bind(party.id)
        .bind(provider)
        .bind(input.purpose.as_str())
        .bind(reference)
        .bind(state_mac_value)
        .bind(sealed)
        .bind(started.expires_at)
        .bind(ctx.now)
        .bind(browser_mac_value)
        .bind(framed)
        .execute(&mut *conn)
        .await?;
        audit(
            ctx,
            conn,
            Entry::new("party.eid_started", "document", document.id)
                .metadata(json!({ "party": party.id, "name": party.name, "method": provider, "purpose": input.purpose.as_str() })),
        )
        .await?;
        Ok(started)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusInput {
    pub token: String,
    pub id: Uuid,
    /// The code the window showed, when the status asks for one with `needsCode`.
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EidStatus {
    /// `pending`, `complete` or `failed`.
    pub status: String,
    /// What to tell the person.
    pub message: Option<String>,
    /// Which text `message` is, for showing it in the person's language.
    pub code: Option<String>,
    /// The QR code to show now, while pending.
    pub qr: Option<String>,
    /// Set once, when complete: pass it as `session` with the next calls.
    pub session: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    /// The person finished in another window, and the session comes only with the code shown there.
    #[serde(default)]
    pub needs_code: bool,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    provider: String,
    purpose: String,
    reference: Option<String>,
    secrets: Option<String>,
    status: String,
    message: Option<String>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    framed: bool,
    release_mac: Option<String>,
}

const ROW_COLUMNS: &str =
    "id, provider, purpose, reference, secrets, status, message, expires_at, created_at, consumed_at, framed, release_mac";

fn secrets(ctx: &Ctx, row: &Row) -> Result<Secrets> {
    let sealed = row.secrets.as_deref().ok_or_else(|| AppError::internal(anyhow::anyhow!("eID session without secrets")))?;
    let plain = ctx.keys().decrypt_str(KeyPurpose::Settings, &secrets_context(row.id), sealed).map_err(AppError::internal)?;
    serde_json::from_str(&plain).map_err(AppError::internal)
}

fn status(status: &str, text: EidText) -> EidStatus {
    EidStatus {
        status: status.into(),
        message: Some(text.message.into()),
        code: Some(text.code.into()),
        qr: None,
        session: None,
        expires_at: None,
        needs_code: false,
    }
}

fn failed(text: EidText) -> EidStatus {
    status("failed", text)
}

/// The stored reason holds a text's code, or the words of an error from the provider.
fn failed_with(stored: Option<String>) -> EidStatus {
    let Some(stored) = stored else { return failed(EidText::FAILED) };
    match EidText::by_code(&stored) {
        Some(text) => failed(text),
        None => EidStatus { message: Some(stored), code: None, ..failed(EidText::FAILED) },
    }
}

async fn store_failure(conn: &mut PgConnection, id: Uuid, stored: &str) -> Result<()> {
    sqlx::query("update eid_sessions set status = 'failed', message = $2, secrets = null where id = $1 and status = 'pending'")
        .bind(id)
        .bind(stored)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn fail(conn: &mut PgConnection, id: Uuid, text: EidText) -> Result<EidStatus> {
    store_failure(conn, id, text.code).await?;
    Ok(failed(text))
}

/// Records what the provider proved, unless it is someone other than the person the document was
/// sent to.
async fn complete(
    ctx: &Ctx,
    conn: &mut PgConnection,
    party: &PartyRow,
    row_id: Uuid,
    provider: &str,
    identity: &Identity,
    evidence: &Value,
) -> Result<std::result::Result<(), EidText>> {
    if let Some(expected) = required_personal_number(ctx.keys(), party)? {
        let matches = identity.personal_number.as_deref().is_some_and(|proven| same_person(&expected, proven, ctx.now.year()));
        if !matches {
            fail(conn, row_id, EidText::OTHER_PERSON).await?;
            audit(
                ctx,
                conn,
                Entry::new("party.eid_mismatch", "document", party.document_id)
                    .metadata(json!({ "party": party.id, "name": party.name, "method": provider, "eidName": identity.name })),
            )
            .await?;
            return Ok(Err(EidText::OTHER_PERSON));
        }
    }
    let evidence = ctx.keys().encrypt_str(KeyPurpose::Settings, &evidence_context(row_id), &evidence.to_string());
    sqlx::query(
        "update eid_sessions set status = 'complete', identity = $2, evidence = $3, completed_at = $4, secrets = null
         where id = $1 and status = 'pending'",
    )
    .bind(row_id)
    .bind(seal_identity(ctx, row_id, identity))
    .bind(evidence)
    .bind(ctx.now)
    .execute(&mut *conn)
    .await?;
    Ok(Ok(()))
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = StatusInput;
    type Output = EidStatus;
    const META: Meta =
        Meta::query("signing.eid_status", "Check an eID identification; once complete, it returns the signer session.", Access::Public)
            .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: StatusInput) -> Result<EidStatus> {
        let Resolved { party, document, .. } = resolve(ctx, conn, &input.token, None).await?;
        let row: Row = sqlx::query_as(&format!("select {ROW_COLUMNS} from eid_sessions where id = $1 and party_id = $2"))
            .bind(input.id)
            .bind(party.id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| not_found("Identification"))?;
        if row.consumed_at.is_some() {
            return Ok(failed(EidText::ALREADY_USED));
        }
        let method = method_of(&row.provider);
        let purpose = if row.purpose == "sign" { Purpose::Sign } else { Purpose::View };
        if method_for(&party, purpose) != method {
            return fail(conn, row.id, EidText::METHOD_CHANGED).await;
        }
        if row.status == "failed" {
            return Ok(failed_with(row.message));
        }
        if row.status == "pending" {
            if row.expires_at < ctx.now {
                return fail(conn, row.id, EidText::TOO_LONG).await;
            }
            let pending = |text: EidText, qr: Option<String>| EidStatus { qr, expires_at: Some(row.expires_at), ..status("pending", text) };
            let lang = party.lang(&document);
            let eid: EidSettings = settings(ctx, conn).await?;
            let reference = row.reference.clone().unwrap_or_default();
            let (identity, evidence) = match row.provider.as_str() {
                "se_bankid" => match BankId::from_settings(&eid)?.collect(&reference).await? {
                    Collected::Pending { hint } => {
                        let s = secrets(ctx, &row)?;
                        let qr = match (s.qr_start_token, s.qr_start_secret) {
                            (Some(t), Some(k)) => Some(bankid::qr_code(&t, &k, (ctx.now - row.created_at).num_seconds().max(0))),
                            _ => None,
                        };
                        return Ok(pending(bankid::message(hint.as_deref(), false), qr));
                    }
                    Collected::Failed { hint } => return fail(conn, row.id, bankid::message(hint.as_deref(), true)).await,
                    Collected::Complete { identity, evidence } => (identity, evidence),
                },
                "freja" => match Freja::from_settings(&eid)?.result(&reference).await? {
                    Outcome::Pending => {
                        return Ok(pending(EidText::new("freja_confirm", lang.freja_confirm()), None));
                    }
                    Outcome::Failed(text) => return fail(conn, row.id, text).await,
                    Outcome::Complete { identity, evidence } => (identity, evidence),
                },
                _ => return Ok(pending(EidText::new("waiting_provider", "Waiting for the eID provider."), None)),
            };
            if let Err(text) = complete(ctx, conn, &party, row.id, &row.provider, &identity, &evidence).await? {
                return Ok(failed(text));
            }
        }
        // The window may have reached someone else than whoever holds this frame, by a forwarded link.
        if row.framed && !release(ctx, conn, &row, &party, input.code.as_deref()).await? {
            if row.release_mac.is_none() || input.code.is_none() {
                return Ok(EidStatus {
                    status: "pending".into(),
                    message: None,
                    code: None,
                    qr: None,
                    session: None,
                    expires_at: Some(row.expires_at),
                    needs_code: true,
                });
            }
            let left: Option<i32> = sqlx::query_scalar("select release_attempts from eid_sessions where id = $1 and status = 'complete'")
                .bind(row.id)
                .fetch_optional(&mut *conn)
                .await?;
            return Ok(match left {
                Some(_) => EidStatus {
                    status: "pending".into(),
                    message: None,
                    code: None,
                    qr: None,
                    session: None,
                    expires_at: Some(row.expires_at),
                    needs_code: true,
                },
                None => failed(EidText::TOO_MANY_CODES),
            });
        }
        let consumed: Option<Value> = sqlx::query_scalar(
            "update eid_sessions set consumed_at = $2 where id = $1 and status = 'complete' and consumed_at is null returning identity",
        )
        .bind(row.id)
        .bind(ctx.now)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(identity) = consumed.and_then(|v| open_identity(ctx, row.id, &v)) else {
            return Ok(failed(EidText::ALREADY_USED));
        };
        audit(
            ctx,
            conn,
            Entry::new("party.authenticated", "document", document.id).metadata(json!({
                "party": party.id, "name": party.name, "method": row.provider, "eidName": identity.name, "eidSession": row.id,
            })),
        )
        .await?;
        let mut proof = format!("{}, {}", identity.name, method.label());
        if let Some(number) = &identity.personal_number {
            proof.push_str(&format!(", personal number {number}"));
        }
        let eid: EidSettings = settings(ctx, conn).await?;
        if (row.provider == "se_bankid" && eid.bankid_test) || (method == AuthMethod::Freja && eid.freja_test) {
            proof.push_str(" (TEST environment, not a real identification)");
        }
        Ok(EidStatus {
            status: "complete".into(),
            message: None,
            code: None,
            qr: None,
            session: Some(issue_session(ctx, &party, method, Some(proof), purpose == Purpose::Sign)),
            expires_at: Some(ctx.now + Duration::minutes(SESSION_MINUTES)),
            needs_code: false,
        })
    }
}

const MAX_RELEASE_ATTEMPTS: i32 = 5;

/// Whether `code` is the one the window showed. Wrong guesses are counted, and too many throw the
/// identification away.
async fn release(ctx: &Ctx, conn: &mut PgConnection, row: &Row, party: &PartyRow, code: Option<&str>) -> Result<bool> {
    let (Some(mac), Some(code)) = (&row.release_mac, code) else { return Ok(false) };
    if constant_time_eq(release_mac(ctx, row.id, code.trim()).as_bytes(), mac.as_bytes()) {
        return Ok(true);
    }
    let attempts: Option<i32> = sqlx::query_scalar(
        "update eid_sessions set release_attempts = release_attempts + 1 where id = $1 and status = 'complete' returning release_attempts",
    )
    .bind(row.id)
    .fetch_optional(&mut *conn)
    .await?;
    audit(
        ctx,
        conn,
        Entry::new("party.eid_code_refused", "document", party.document_id)
            .metadata(json!({ "party": party.id, "name": party.name, "method": row.provider })),
    )
    .await?;
    if attempts.is_some_and(|n| n >= MAX_RELEASE_ATTEMPTS) {
        sqlx::query("update eid_sessions set status = 'failed', message = $2, identity = null, evidence = null, release_mac = null, secrets = null where id = $1")
            .bind(row.id)
            .bind(EidText::TOO_MANY_CODES.code)
            .execute(&mut *conn)
            .await?;
    }
    Ok(false)
}

pub struct Cancel;

#[async_trait]
impl Service for Cancel {
    type Input = StatusInput;
    type Output = super::Ok_;
    const META: Meta =
        Meta::mutation("signing.eid_cancel", "Cancel an eID identification that is still waiting.", Access::Public).without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: StatusInput) -> Result<super::Ok_> {
        let Resolved { party, .. } = resolve(ctx, conn, &input.token, None).await?;
        let row: Option<(String, Option<String>)> = sqlx::query_as(
            "update eid_sessions set status = 'failed', message = 'canceled', secrets = null where id = $1 and party_id = $2 and status = 'pending'
             returning provider, reference",
        )
        .bind(input.id)
        .bind(party.id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((provider, Some(reference))) = row {
            let eid: EidSettings = settings(ctx, conn).await?;
            let result = match provider.as_str() {
                "se_bankid" => BankId::from_settings(&eid)?.cancel(&reference).await,
                "freja" => Freja::from_settings(&eid)?.cancel(&reference).await,
                _ => Ok(()),
            };
            if let Err(e) = result {
                tracing::info!(error = %e, "eID cancel was not confirmed by the provider");
            }
        }
        Ok(super::Ok_::default())
    }
}

#[derive(Serialize, Deserialize)]
struct ContinueTicket {
    s: Uuid,
    p: Uuid,
    v: i32,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContinueInput {
    /// From the address of the page opened by a framed signing page.
    pub ticket: String,
    /// The person pressed Continue. Without it, this only says what they are about to identify for.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContinueOutput {
    pub title: String,
    pub organisation: String,
    pub method: AuthMethod,
    pub language: String,
    /// Set once confirmed: where to send the browser.
    pub redirect_url: Option<String>,
    /// See `Started::browser_binding`.
    pub browser_binding: Option<String>,
}

pub struct Continue;

#[async_trait]
impl Service for Continue {
    type Input = ContinueInput;
    type Output = ContinueOutput;
    const META: Meta = Meta::mutation(
        "signing.eid_continue",
        "Show what a framed signing page's eID identification is for and, once confirmed, tie it to this browser and go on to the eID provider.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ContinueInput) -> Result<ContinueOutput> {
        let invalid = || bad_request("This identification is not valid or was already used. Go back to the signing link and start again.");
        let ticket: ContinueTicket =
            ctx.keys().verify_token(CONTINUE_TICKET, input.ticket.trim(), ctx.now.timestamp()).map_err(|_| invalid())?;
        let party: PartyRow = sqlx::query_as(&format!("select {PARTY_COLUMNS} from parties where id = $1 and link_version = $2"))
            .bind(ticket.p)
            .bind(ticket.v)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(invalid)?;
        let document: DocumentRow = sqlx::query_as(&format!("select {DOCUMENT_COLUMNS} from documents d where id = $1"))
            .bind(party.document_id)
            .fetch_one(&mut *conn)
            .await?;
        if document.status != status::PENDING {
            return Err(invalid());
        }
        let row: Row = sqlx::query_as(&format!(
            "select {ROW_COLUMNS} from eid_sessions where id = $1 and party_id = $2 and status = 'pending' and browser_mac is null and expires_at > $3"
        ))
        .bind(ticket.s)
        .bind(party.id)
        .bind(ctx.now)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(invalid)?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let mut output = ContinueOutput {
            title: document.title.clone(),
            organisation: if org.name.is_empty() { "OpenSign".to_string() } else { org.name },
            method: method_of(&row.provider),
            language: party.lang(&document).code().to_string(),
            redirect_url: None,
            browser_binding: None,
        };
        if input.confirm {
            let binding = random_token();
            let bound =
                sqlx::query("update eid_sessions set browser_mac = $2 where id = $1 and status = 'pending' and browser_mac is null")
                    .bind(row.id)
                    .bind(browser_mac(ctx, &binding))
                    .execute(&mut *conn)
                    .await?;
            if bound.rows_affected() == 0 {
                return Err(invalid());
            }
            audit(
                ctx,
                conn,
                Entry::new("party.eid_continued", "document", document.id)
                    .metadata(json!({ "party": party.id, "name": party.name, "method": row.provider })),
            )
            .await?;
            output.redirect_url = secrets(ctx, &row)?.authorize_url;
            output.browser_binding = Some(binding);
        }
        Ok(output)
    }
}

type CallbackRow = (Uuid, Uuid, String, Option<String>, DateTime<Utc>, Option<String>, bool);

/// Where the browser goes after the callback.
pub struct Returned {
    /// The eID session to resume on the signing page.
    pub id: Uuid,
    /// For a framed signing page: what to show the person, to enter in the frame.
    pub code: Option<String>,
}

/// The broker or TIC Identity sends the browser back here.
pub async fn broker_callback(
    ctx: &Ctx,
    conn: &mut PgConnection,
    state: &str,
    browser: Option<&str>,
    code: Option<&str>,
    error: Option<&str>,
) -> Result<Returned> {
    let row: Option<CallbackRow> = sqlx::query_as(
        "update eid_sessions set state_mac = null where state_mac = $1 and provider in ('broker', 'tic') and status = 'pending'
         returning id, party_id, provider, secrets, expires_at, browser_mac, framed",
    )
    .bind(state_mac(ctx, state))
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id, party_id, provider, sealed, expires_at, bound_to, framed)) = row else {
        return Err(bad_request("This identification is not valid or was already used. Go back to the signing link and start again."));
    };
    let party: PartyRow =
        sqlx::query_as(&format!("select {PARTY_COLUMNS} from parties where id = $1")).bind(party_id).fetch_one(&mut *conn).await?;
    let same_browser = match (&bound_to, browser) {
        (Some(mac), Some(binding)) => constant_time_eq(browser_mac(ctx, binding).as_bytes(), mac.as_bytes()),
        _ => false,
    };
    if !same_browser {
        fail(conn, id, EidText::OTHER_BROWSER).await?;
        audit(
            ctx,
            conn,
            Entry::new("party.eid_other_browser", "document", party.document_id)
                .metadata(json!({ "party": party.id, "name": party.name, "method": provider })),
        )
        .await?;
        return Err(bad_request(EidText::OTHER_BROWSER.message));
    }
    let finish = async {
        if expires_at < ctx.now {
            return Err(bad_request(EidText::TOO_LONG.message));
        }
        if error.is_some() {
            return Err(bad_request(EidText::BROKER_CANCELED.message));
        }
        let code = code.ok_or_else(|| bad_request(EidText::BROKER_NO_RESULT.message))?;
        let plain = ctx
            .keys()
            .decrypt_str(KeyPurpose::Settings, &secrets_context(id), sealed.as_deref().unwrap_or_default())
            .map_err(AppError::internal)?;
        let secrets: Secrets = serde_json::from_str(&plain).map_err(AppError::internal)?;
        let (Some(nonce), Some(verifier)) = (secrets.nonce, secrets.verifier) else {
            return Err(AppError::internal(anyhow::anyhow!("broker session without nonce")));
        };
        let eid: EidSettings = settings(ctx, conn).await?;
        let oidc = oidc_provider(ctx, &eid, &provider)?;
        let discovery = oidc.discover().await?;
        let id_token = oidc::exchange(
            &oidc.http,
            &discovery,
            &oidc::Exchange {
                client_id: &oidc.client_id,
                client_secret: &oidc.client_secret,
                code,
                verifier: &verifier,
                redirect_uri: &redirect_uri(ctx),
            },
        )
        .await?;
        let keys = oidc::keys(&oidc.http, &discovery).await?;
        let claims =
            oidc::verify_id_token(&id_token, &keys, &discovery.issuer, &oidc.client_id, &nonce, &oidc.acr_values, ctx.now.timestamp())?;
        let identity = if provider == "tic" {
            tic::identity(&claims, ctx.now.year()).ok_or_else(|| bad_request(EidText::NO_PERSONAL_NUMBER.message))?
        } else {
            oidc::identity(&claims).ok_or_else(|| bad_request(EidText::BROKER_NO_PERSON.message))?
        };
        let evidence = json!({ "provider": provider, "issuer": discovery.issuer, "idToken": id_token });
        if let Err(text) = complete(ctx, conn, &party, id, &provider, &identity, &evidence).await? {
            return Err(bad_request(text.message));
        }
        Ok(())
    };
    if let Err(e) = finish.await {
        let stored = match EidText::by_message(&e.message) {
            Some(text) => text.code,
            None if e.kind == ErrorKind::BadRequest => e.message.as_str(),
            None => EidText::FAILED.code,
        };
        if e.kind != ErrorKind::BadRequest {
            tracing::warn!(error = %e, "eID broker callback failed");
        }
        store_failure(conn, id, stored).await?;
        return Ok(Returned { id, code: None });
    }
    if !framed {
        return Ok(Returned { id, code: None });
    }
    let code = format!("{:06}", u32::from_be_bytes(random_bytes::<4>()) % 1_000_000);
    sqlx::query("update eid_sessions set release_mac = $2 where id = $1")
        .bind(id)
        .bind(release_mac(ctx, id, &code))
        .execute(&mut *conn)
        .await?;
    Ok(Returned { id, code: Some(code) })
}
