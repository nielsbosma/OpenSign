//! Identifying a party with an eID through their signing link. The browser only ever learns the
//! eID session's id; provider secrets stay encrypted on the server, and the signer session is
//! handed out once, to a caller that also holds the signing link.

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Utc};
use opensign_crypto::{KeyPurpose, random_token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::settings;
use super::signing::{Resolved, SESSION_MINUTES, issue_session, resolve, satisfied};
use crate::audit::{Entry, audit};
use crate::context::Ctx;
use crate::documents::{AuthMethod, PartyRow, required_personal_number, status};
use crate::eid::bankid::{self, BankId, Collected};
use crate::eid::freja::{self, Freja, Outcome, User};
use crate::eid::{Identity, oidc, same_person, se_personal_number};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, not_found, too_many};
use crate::i18n::Lang;
use crate::service::{Access, Meta, Service};
use crate::settings::{EidSettings, OrgSettings};

const STARTS_PER_HOUR: i64 = 10;

fn secrets_context(id: Uuid) -> String {
    format!("eid:{id}:secrets")
}

fn evidence_context(id: Uuid) -> String {
    format!("eid:{id}:evidence")
}

pub fn state_mac(ctx: &Ctx, state: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("eid-state:{state}").as_bytes()))
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
        "se_bankid" => AuthMethod::SeBankid,
        "freja" => AuthMethod::Freja,
        _ => AuthMethod::Broker,
    }
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
        let expected = required_personal_number(ctx.keys(), &party)?;
        let id = Uuid::now_v7();
        let mut secrets = Secrets { qr_start_token: None, qr_start_secret: None, nonce: None, verifier: None };
        let mut state_mac_value = None;
        let (reference, started) = match method {
            AuthMethod::SeBankid => {
                let number = match expected.as_deref() {
                    Some(n) => Some(se_personal_number(n, ctx.now.year()).ok_or_else(|| bad_request("The personal number on this party is not a Swedish one, so BankID cannot be used. Ask the sender to correct it."))?),
                    None => None,
                };
                let org: OrgSettings = settings(ctx, conn).await?;
                let organisation = if org.name.is_empty() { "OpenSign".to_string() } else { org.name };
                let text =
                    (input.purpose == Purpose::Sign).then(|| format!("I sign the document \"{}\" sent by {organisation}.", document.title));
                let ip = ctx.ip.clone().unwrap_or_else(|| "127.0.0.1".into());
                let order = BankId::from_settings(&eid)?.start(&ip, number.as_deref(), text.as_deref()).await?;
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
                };
                (Some(order.order_ref), started)
            }
            AuthMethod::Freja => {
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
                };
                (Some(auth_ref), started)
            }
            _ => {
                if !eid.broker_enabled {
                    return Err(bad_request("eID is not set up. An admin enables it in Admin settings."));
                }
                let discovery = oidc::discover(&ctx.app.http.public, &eid.broker_issuer).await?;
                let state = random_token();
                let nonce = random_token();
                let verifier = random_token();
                let redirect = redirect_uri(ctx);
                let url = oidc::authorize_url(
                    &discovery,
                    &oidc::AuthorizeRequest {
                        client_id: &eid.broker_client_id,
                        redirect_uri: &redirect,
                        state: &state,
                        nonce: &nonce,
                        verifier: &verifier,
                        acr_values: &eid.broker_acr_values,
                        login_hint: None,
                    },
                )?;
                state_mac_value = Some(state_mac(ctx, &state));
                secrets.nonce = Some(nonce);
                secrets.verifier = Some(verifier);
                let started = Started {
                    id,
                    method,
                    auto_start_token: None,
                    qr: None,
                    redirect_url: Some(url),
                    expires_at: ctx.now + Duration::minutes(10),
                };
                (None, started)
            }
        };
        let sealed = ctx.keys().encrypt_str(
            KeyPurpose::Settings,
            &secrets_context(id),
            &serde_json::to_string(&secrets).map_err(AppError::internal)?,
        );
        sqlx::query(
            "insert into eid_sessions (id, party_id, provider, purpose, reference, state_mac, secrets, expires_at, created_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EidStatus {
    /// `pending`, `complete` or `failed`.
    pub status: String,
    /// What to tell the person.
    pub message: Option<String>,
    /// The QR code to show now, while pending.
    pub qr: Option<String>,
    /// Set once, when complete: pass it as `session` with the next calls.
    pub session: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
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
}

const ROW_COLUMNS: &str = "id, provider, purpose, reference, secrets, status, message, expires_at, created_at, consumed_at";

fn secrets(ctx: &Ctx, row: &Row) -> Result<Secrets> {
    let sealed = row.secrets.as_deref().ok_or_else(|| AppError::internal(anyhow::anyhow!("eID session without secrets")))?;
    let plain = ctx.keys().decrypt_str(KeyPurpose::Settings, &secrets_context(row.id), sealed).map_err(AppError::internal)?;
    serde_json::from_str(&plain).map_err(AppError::internal)
}

fn failed(message: impl Into<String>) -> EidStatus {
    EidStatus { status: "failed".into(), message: Some(message.into()), qr: None, session: None, expires_at: None }
}

async fn fail(conn: &mut PgConnection, id: Uuid, message: &str) -> Result<EidStatus> {
    sqlx::query("update eid_sessions set status = 'failed', message = $2, secrets = null where id = $1 and status = 'pending'")
        .bind(id)
        .bind(message)
        .execute(&mut *conn)
        .await?;
    Ok(failed(message))
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
) -> Result<std::result::Result<(), String>> {
    if let Some(expected) = required_personal_number(ctx.keys(), party)? {
        let matches = identity.personal_number.as_deref().is_some_and(|proven| same_person(&expected, proven, ctx.now.year()));
        if !matches {
            let message = "This eID belongs to someone other than the person the document was sent to.";
            fail(conn, row_id, message).await?;
            audit(
                ctx,
                conn,
                Entry::new("party.eid_mismatch", "document", party.document_id)
                    .metadata(json!({ "party": party.id, "name": party.name, "method": provider, "eidName": identity.name })),
            )
            .await?;
            return Ok(Err(message.into()));
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
            return Ok(failed("This identification was already used. Start again."));
        }
        let method = method_of(&row.provider);
        let purpose = if row.purpose == "sign" { Purpose::Sign } else { Purpose::View };
        if method_for(&party, purpose) != method {
            return fail(conn, row.id, "The sender changed how you identify. Start again.").await;
        }
        if row.status == "failed" {
            return Ok(failed(row.message.unwrap_or_else(|| "The identification did not go through. Try again.".into())));
        }
        if row.status == "pending" {
            if row.expires_at < ctx.now {
                return fail(conn, row.id, "The identification took too long. Try again.").await;
            }
            let pending = |message: &str, qr: Option<String>| EidStatus {
                status: "pending".into(),
                message: Some(message.to_string()),
                qr,
                session: None,
                expires_at: Some(row.expires_at),
            };
            let lang = Lang::of(&document.language);
            let eid: EidSettings = settings(ctx, conn).await?;
            let reference = row.reference.clone().unwrap_or_default();
            let (identity, evidence) = match method {
                AuthMethod::SeBankid => match BankId::from_settings(&eid)?.collect(&reference).await? {
                    Collected::Pending { hint } => {
                        let s = secrets(ctx, &row)?;
                        let qr = match (s.qr_start_token, s.qr_start_secret) {
                            (Some(t), Some(k)) => Some(bankid::qr_code(&t, &k, (ctx.now - row.created_at).num_seconds().max(0))),
                            _ => None,
                        };
                        return Ok(pending(bankid::message(hint.as_deref(), false, lang), qr));
                    }
                    Collected::Failed { hint } => return fail(conn, row.id, bankid::message(hint.as_deref(), true, lang)).await,
                    Collected::Complete { identity, evidence } => (identity, evidence),
                },
                AuthMethod::Freja => match Freja::from_settings(&eid)?.result(&reference).await? {
                    Outcome::Pending => {
                        return Ok(pending(
                            if lang == Lang::Sv { "Bekräfta i Freja eID-appen." } else { "Confirm in the Freja eID app." },
                            None,
                        ));
                    }
                    Outcome::Failed(message) => return fail(conn, row.id, message).await,
                    Outcome::Complete { identity, evidence } => (identity, evidence),
                },
                _ => return Ok(pending("Waiting for the eID provider.", None)),
            };
            if let Err(message) = complete(ctx, conn, &party, row.id, &row.provider, &identity, &evidence).await? {
                return Ok(failed(message));
            }
        }
        let consumed: Option<Value> = sqlx::query_scalar(
            "update eid_sessions set consumed_at = $2 where id = $1 and status = 'complete' and consumed_at is null returning identity",
        )
        .bind(row.id)
        .bind(ctx.now)
        .fetch_optional(&mut *conn)
        .await?;
        let Some(identity) = consumed.and_then(|v| open_identity(ctx, row.id, &v)) else {
            return Ok(failed("This identification was already used. Start again."));
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
        Ok(EidStatus {
            status: "complete".into(),
            message: None,
            qr: None,
            session: Some(issue_session(ctx, &party, method, Some(proof))),
            expires_at: Some(ctx.now + Duration::minutes(SESSION_MINUTES)),
        })
    }
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
            "update eid_sessions set status = 'failed', message = 'Canceled', secrets = null where id = $1 and party_id = $2 and status = 'pending'
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

/// The broker sends the browser back here. Returns the eID session to resume on the signing page.
pub async fn broker_callback(ctx: &Ctx, conn: &mut PgConnection, state: &str, code: Option<&str>, error: Option<&str>) -> Result<Uuid> {
    let row: Option<(Uuid, Uuid, Option<String>, DateTime<Utc>)> = sqlx::query_as(
        "update eid_sessions set state_mac = null where state_mac = $1 and provider = 'broker' and status = 'pending'
         returning id, party_id, secrets, expires_at",
    )
    .bind(state_mac(ctx, state))
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id, party_id, sealed, expires_at)) = row else {
        return Err(bad_request("This identification is not valid or was already used. Go back to the signing link and start again."));
    };
    let finish = async {
        if expires_at < ctx.now {
            return Err(bad_request("The identification took too long. Try again."));
        }
        if error.is_some() {
            return Err(bad_request("The identification was canceled or did not go through."));
        }
        let code = code.ok_or_else(|| bad_request("The eID broker sent no result"))?;
        let party: PartyRow = sqlx::query_as(&format!("select {} from parties where id = $1", crate::documents::PARTY_COLUMNS))
            .bind(party_id)
            .fetch_one(&mut *conn)
            .await?;
        let plain = ctx
            .keys()
            .decrypt_str(KeyPurpose::Settings, &secrets_context(id), sealed.as_deref().unwrap_or_default())
            .map_err(AppError::internal)?;
        let secrets: Secrets = serde_json::from_str(&plain).map_err(AppError::internal)?;
        let (Some(nonce), Some(verifier)) = (secrets.nonce, secrets.verifier) else {
            return Err(AppError::internal(anyhow::anyhow!("broker session without nonce")));
        };
        let eid: EidSettings = settings(ctx, conn).await?;
        let http = &ctx.app.http.public;
        let discovery = oidc::discover(http, &eid.broker_issuer).await?;
        let id_token = oidc::exchange(
            http,
            &discovery,
            &oidc::Exchange {
                client_id: &eid.broker_client_id,
                client_secret: eid.broker_client_secret.as_deref().unwrap_or_default(),
                code,
                verifier: &verifier,
                redirect_uri: &redirect_uri(ctx),
            },
        )
        .await?;
        let keys = oidc::keys(http, &discovery).await?;
        let claims = oidc::verify_id_token(&id_token, &keys, &discovery.issuer, &eid.broker_client_id, &nonce, ctx.now.timestamp())?;
        let identity = oidc::identity(&claims).ok_or_else(|| bad_request("The eID broker did not say who you are"))?;
        let evidence = json!({ "provider": "broker", "issuer": discovery.issuer, "idToken": id_token });
        if let Err(message) = complete(ctx, conn, &party, id, "broker", &identity, &evidence).await? {
            return Err(bad_request(message));
        }
        Ok(())
    };
    if let Err(e) = finish.await {
        let message =
            if e.kind == ErrorKind::BadRequest { e.message.clone() } else { "The identification did not go through. Try again.".into() };
        if e.kind != ErrorKind::BadRequest {
            tracing::warn!(error = %e, "eID broker callback failed");
        }
        fail(conn, id, &message).await?;
    }
    Ok(id)
}
