//! Signing in to the app through one OpenID Connect provider. The account is the token's issuer and
//! subject. An email only finds an account the first time, and never an admin's: otherwise whoever
//! controls the provider could take over an account that holds the keys to the install.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, constant_time_eq, random_token};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Acquire, PgConnection};
use uuid::Uuid;

use super::auth::notify_keys_revoked;
use super::{Empty, settings};
use crate::audit::{Entry, audit};
use crate::auth::{
    UserRow, actor_for, create_session, expire_account_links, normalize_email, revoke_api_keys, revoke_sessions, user_by_email,
};
use crate::context::{Credential, Ctx};
use crate::eid::oidc;
use crate::error::{AppError, Result, bad_request, too_many};
use crate::permissions::Role;
use crate::service::{Access, Meta, Service};
use crate::settings::{SecuritySettings, SsoSettings};

const SCOPE: &str = "openid email profile";
const LOGIN_MINUTES: i64 = 10;
/// Starting a sign-in needs no account, so what it can leave behind is bounded.
const MAX_PENDING: i64 = 10_000;
/// RFC 8176 values that say a factor beyond the password was used.
const MFA_METHODS: &[&str] = &["mfa", "otp", "hwk"];

fn secrets_context(id: Uuid) -> String {
    format!("sso:{id}:secrets")
}

fn state_mac(ctx: &Ctx, state: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("sso-state:{state}").as_bytes()))
}

fn browser_mac(ctx: &Ctx, binding: &str) -> String {
    hex::encode(ctx.keys().mac(KeyPurpose::TokenMac, format!("sso-browser:{binding}").as_bytes()))
}

fn redirect_uri(ctx: &Ctx) -> String {
    format!("{}/api/auth/sso/callback", ctx.app.public_url())
}

/// A real install needs a public https provider. Only `APP_ENV=development` may use a local one.
pub(crate) fn check_issuer(issuer: &str, development: bool) -> std::result::Result<(), String> {
    if development {
        let parsed = reqwest::Url::parse(issuer).map_err(|_| "Enter the issuer's address".to_string())?;
        return if matches!(parsed.scheme(), "http" | "https") { Ok(()) } else { Err("Enter the issuer's http or https address".into()) };
    }
    crate::net::check_public_url(issuer).map(|_| ())
}

fn provider(ctx: &Ctx, sso: &SsoSettings) -> Result<oidc::Provider> {
    let development = ctx.app.env.development;
    let issuer = sso.issuer.trim().trim_end_matches('/').to_string();
    check_issuer(&issuer, development).map_err(bad_request)?;
    Ok(oidc::Provider {
        http: if development { ctx.app.http.internal.clone() } else { ctx.app.http.public.clone() },
        issuer,
        client_id: sso.client_id.trim().to_string(),
        client_secret: sso.client_secret.clone().unwrap_or_default(),
        scope: SCOPE.into(),
        acr_values: String::new(),
        require_public: !development,
    })
}

/// Before the first admin exists, an account would also close the setup page for good.
async fn set_up(conn: &mut PgConnection) -> Result<bool> {
    Ok(sqlx::query_scalar("select exists (select 1 from users)").fetch_one(&mut *conn).await?)
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SsoStatus {
    pub enabled: bool,
    /// The button's text. Empty means the app's general one.
    pub button_label: String,
}

pub struct Status;

#[async_trait]
impl Service for Status {
    type Input = Empty;
    type Output = SsoStatus;
    const META: Meta =
        Meta::query("auth.sso_status", "Whether single sign-on is on, and what its button on the sign-in page says.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<SsoStatus> {
        let sso: SsoSettings = settings(ctx, conn).await?;
        let enabled = sso.ready() && set_up(conn).await?;
        Ok(SsoStatus { enabled, button_label: if enabled { sso.button_label } else { String::new() } })
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SsoStarted {
    /// Send the browser here to sign in at the provider.
    pub redirect_url: String,
    /// Becomes a cookie in the RPC layer and is removed from the answer: the provider's answer counts
    /// only in the browser that holds it.
    pub browser_binding: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Secrets {
    nonce: String,
    verifier: String,
}

pub struct Start;

#[async_trait]
impl Service for Start {
    type Input = Empty;
    type Output = SsoStarted;
    const META: Meta = Meta::mutation(
        "auth.sso_start",
        "Start signing in through the identity provider. Returns where to send the browser.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<SsoStarted> {
        let sso: SsoSettings = settings(ctx, conn).await?;
        if !sso.ready() || !set_up(conn).await? {
            return Err(bad_request("Single sign-on is not set up."));
        }
        sqlx::query("delete from sso_logins where expires_at <= $1").bind(ctx.now).execute(&mut *conn).await?;
        let pending: i64 = sqlx::query_scalar("select count(*) from sso_logins").fetch_one(&mut *conn).await?;
        if pending >= MAX_PENDING {
            return Err(too_many("Too many sign-ins right now. Try again in a few minutes."));
        }
        let oidc = provider(ctx, &sso)?;
        let discovery = oidc.discover_cached().await?;
        let (state, binding, id) = (random_token(), random_token(), Uuid::now_v7());
        let secrets = Secrets { nonce: random_token(), verifier: random_token() };
        let redirect = redirect_uri(ctx);
        // No forced prompt: someone already signed in at the provider should not have to do it again.
        let redirect_url = oidc::authorize_url(
            &discovery,
            &oidc::AuthorizeRequest {
                client_id: &oidc.client_id,
                redirect_uri: &redirect,
                state: &state,
                nonce: &secrets.nonce,
                verifier: &secrets.verifier,
                scope: &oidc.scope,
                acr_values: "",
                login_hint: None,
                prompt: None,
            },
        )?;
        let sealed = ctx.keys().encrypt_str(
            KeyPurpose::Settings,
            &secrets_context(id),
            &serde_json::to_string(&secrets).map_err(AppError::internal)?,
        );
        sqlx::query("insert into sso_logins (id, state_mac, browser_mac, secrets, expires_at, created_at) values ($1, $2, $3, $4, $5, $6)")
            .bind(id)
            .bind(state_mac(ctx, &state))
            .bind(browser_mac(ctx, &binding))
            .bind(sealed)
            .bind(ctx.now + Duration::minutes(LOGIN_MINUTES))
            .bind(ctx.now)
            .execute(&mut *conn)
            .await?;
        Ok(SsoStarted { redirect_url, browser_binding: Some(binding) })
    }
}

/// A new session, for the HTTP layer to put in a cookie.
#[derive(Debug)]
pub struct SsoSession {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Why a sign-in was refused. The code is what the sign-in page shows a text for.
struct Refusal {
    code: &'static str,
    user: Option<Uuid>,
}

fn refused(code: &'static str) -> Refusal {
    Refusal { code, user: None }
}

fn refused_for(code: &'static str, user: &UserRow) -> Refusal {
    Refusal { code, user: Some(user.id) }
}

/// What failed stays in the log; the person is told to try again.
impl From<AppError> for Refusal {
    fn from(error: AppError) -> Self {
        tracing::warn!(error = %error, cause = ?error.cause, "single sign-on failed");
        refused("failed")
    }
}

impl From<sqlx::Error> for Refusal {
    fn from(error: sqlx::Error) -> Self {
        AppError::from(error).into()
    }
}

/// Finishes a sign-in when the provider sends the browser back. Runs without a surrounding
/// transaction: the state must stay used and a refusal audited whatever happens after. On a refusal
/// the error is a fixed code for the sign-in page.
pub async fn callback(
    ctx: &Ctx,
    state: &str,
    browser: Option<&str>,
    code: Option<&str>,
    error: Option<&str>,
) -> std::result::Result<SsoSession, &'static str> {
    // Whoever else is signed in in this browser did not do this.
    let anonymous = Ctx { actor: None, session_id: None, ..ctx.clone() };
    let ctx = &anonymous;
    let mut conn = ctx.db().acquire().await.map_err(|_| "failed")?;
    let sso: SsoSettings = settings(ctx, &mut conn).await.map_err(|_| "failed")?;
    if !sso.ready() {
        return Err("disabled");
    }
    // Whoever can reach this address can call it, so what does not match a sign-in we started
    // leaves nothing in the append-only audit log.
    let taken: Option<(Uuid, String, String, DateTime<Utc>)> =
        sqlx::query_as("delete from sso_logins where state_mac = $1 returning id, browser_mac, secrets, expires_at")
            .bind(state_mac(ctx, state))
            .fetch_optional(&mut *conn)
            .await
            .map_err(|_| "failed")?;
    let Some((id, expected_browser, sealed, expires_at)) = taken else { return Err("expired") };
    if expires_at <= ctx.now {
        return Err("expired");
    }
    let outcome = finish(ctx, &mut conn, &sso, id, &expected_browser, &sealed, browser, code, error).await;
    match outcome {
        Ok(session) => Ok(session),
        Err(refusal) => {
            let entry = match refusal.user {
                Some(user) => Entry::new("auth.sso_refused", "user", user),
                None => Entry::new("auth.sso_refused", "settings", "sso"),
            };
            if let Err(e) = audit(ctx, &mut conn, entry.metadata(json!({ "reason": refusal.code }))).await {
                tracing::error!(error = %e, "could not audit a refused single sign-on");
            }
            Err(refusal.code)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn finish(
    ctx: &Ctx,
    conn: &mut PgConnection,
    sso: &SsoSettings,
    id: Uuid,
    expected_browser: &str,
    sealed: &str,
    browser: Option<&str>,
    code: Option<&str>,
    error: Option<&str>,
) -> std::result::Result<SsoSession, Refusal> {
    let same_browser = browser.is_some_and(|b| constant_time_eq(browser_mac(ctx, b).as_bytes(), expected_browser.as_bytes()));
    if !same_browser {
        return Err(refused("other_browser"));
    }
    if let Some(error) = error {
        return Err(refused(if error == "access_denied" { "denied" } else { "failed" }));
    }
    let code = code.filter(|c| !c.is_empty() && c.len() <= 4096).ok_or_else(|| refused("failed"))?;
    let plain = ctx.keys().decrypt_str(KeyPurpose::Settings, &secrets_context(id), sealed).map_err(AppError::internal)?;
    let secrets: Secrets = serde_json::from_str(&plain).map_err(AppError::internal)?;

    let oidc = provider(ctx, sso)?;
    let discovery = oidc.discover_cached().await?;
    let redirect = redirect_uri(ctx);
    let id_token = oidc::exchange(
        &oidc.http,
        &discovery,
        &oidc::Exchange {
            client_id: &oidc.client_id,
            client_secret: &oidc.client_secret,
            code,
            verifier: &secrets.verifier,
            redirect_uri: &redirect,
        },
    )
    .await?;
    let claims = oidc.verify_cached(&discovery, &id_token, &secrets.nonce, ctx.now.timestamp()).await?;
    let subject = claims["sub"].as_str().filter(|s| !s.is_empty() && s.len() <= 255).ok_or_else(|| refused("failed"))?;

    if !email_verified(&claims) {
        return Err(refused("email_unverified"));
    }
    let email = claims["email"].as_str().and_then(|e| normalize_email(e).ok()).ok_or_else(|| refused("email_unverified"))?;
    let security: SecuritySettings = settings(ctx, conn).await?;
    let mfa = proves_mfa(&claims);
    // A required second factor is not weaker for coming through here.
    if (sso.require_provider_mfa || security.enforce_two_factor) && !mfa {
        return Err(refused("mfa_required"));
    }

    let mut tx = conn.begin().await?;
    let (user, keys_revoked) = account(ctx, &mut tx, sso, &oidc.issuer, subject, &email, &claims).await?;
    let session = create_session(&mut tx, user.id, &security, ctx.ip.as_deref(), ctx.user_agent.as_deref(), ctx.now).await?;
    // No confirmation counts as made: nothing that asks for one is open to this account anyway.
    sqlx::query("update sessions set sso_mfa = $2, elevated_until = null where id = $1")
        .bind(&session.id)
        .bind(mfa)
        .execute(&mut *tx)
        .await?;
    sqlx::query("update users set last_login_at = $2 where id = $1").bind(user.id).bind(ctx.now).execute(&mut *tx).await?;
    sqlx::query("update user_identities set last_login_at = $2 where user_id = $1").bind(user.id).bind(ctx.now).execute(&mut *tx).await?;
    let actor = actor_for(&user, &security, Credential::Session).ok_or_else(|| refused_for("failed", &user))?;
    let signed_in = ctx.clone().with_actor(actor);
    audit(&signed_in, &mut tx, Entry::new("auth.sign_in", "user", user.id).metadata(json!({ "method": "sso", "mfa": mfa }))).await?;
    tx.commit().await?;
    if keys_revoked > 0 {
        notify_keys_revoked(ctx, conn, &user, keys_revoked, "Your account now signs in through single sign-on.").await?;
    }
    Ok(SsoSession { token: session.token, expires_at: session.expires_at })
}

/// `xms_edov` is how Microsoft Entra ID says the same thing; it has no `email_verified`.
fn email_verified(claims: &Value) -> bool {
    let yes = |v: &Value| *v == Value::Bool(true) || v.as_str().is_some_and(|s| s == "true" || s == "1");
    yes(&claims["email_verified"]) || yes(&claims["xms_edov"])
}

fn proves_mfa(claims: &Value) -> bool {
    claims["amr"].as_array().is_some_and(|methods| methods.iter().filter_map(Value::as_str).any(|m| MFA_METHODS.contains(&m)))
}

fn display_name(claims: &Value, email: &str) -> String {
    let name = oidc::name(claims).unwrap_or_else(|| email.split('@').next().unwrap_or(email).to_string());
    name.chars().filter(|c| !c.is_control()).take(200).collect()
}

async fn by_identity(conn: &mut PgConnection, issuer: &str, subject: &str) -> Result<Option<UserRow>> {
    Ok(sqlx::query_as("select u.* from users u join user_identities i on i.user_id = u.id where i.issuer = $1 and i.subject = $2")
        .bind(issuer)
        .bind(subject)
        .fetch_optional(&mut *conn)
        .await?)
}

async fn add_identity(conn: &mut PgConnection, user: Uuid, issuer: &str, subject: &str) -> Result<()> {
    sqlx::query("insert into user_identities (id, user_id, issuer, subject) values ($1, $2, $3, $4)")
        .bind(Uuid::now_v7())
        .bind(user)
        .bind(issuer)
        .bind(subject)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The account for this identity: known already, an existing one with this email, or a new one.
/// Also returns how many API keys making an existing account a single sign-on one revoked.
async fn account(
    ctx: &Ctx,
    conn: &mut PgConnection,
    sso: &SsoSettings,
    issuer: &str,
    subject: &str,
    email: &str,
    claims: &Value,
) -> std::result::Result<(UserRow, u64), Refusal> {
    let mut known = by_identity(conn, issuer, subject).await?;
    if known.is_none() {
        // Shared with setup, so that the first admin and a first sign-in cannot pass each other, and
        // with other first sign-ins of the same person.
        sqlx::query("select pg_advisory_xact_lock($1)").bind(super::setup::SETUP_LOCK).execute(&mut *conn).await?;
        known = by_identity(conn, issuer, subject).await?;
    }
    if let Some(user) = known {
        return Ok((returning(ctx, conn, user, email).await?, 0));
    }
    if !set_up(conn).await? {
        return Err(refused("disabled"));
    }
    match user_by_email(conn, email).await? {
        Some(user) if !user.active => Err(refused_for("inactive", &user)),
        Some(user) if user.role == Role::Admin.as_str() => Err(refused_for("admin_account", &user)),
        // It belongs to another identity. The address may have moved on to someone new.
        Some(user) if user.sso => Err(refused_for("email_conflict", &user)),
        Some(user) => link(ctx, conn, user, issuer, subject).await,
        None => Ok((create(ctx, conn, sso, issuer, subject, email, claims).await?, 0)),
    }
}

async fn returning(ctx: &Ctx, conn: &mut PgConnection, user: UserRow, email: &str) -> std::result::Result<UserRow, Refusal> {
    if !user.active {
        return Err(refused_for("inactive", &user));
    }
    if user.email.eq_ignore_ascii_case(email) {
        return Ok(user);
    }
    if user_by_email(conn, email).await?.is_some() {
        return Err(refused_for("email_conflict", &user));
    }
    sqlx::query("update users set email = $2, updated_at = now() where id = $1").bind(user.id).bind(email).execute(&mut *conn).await?;
    audit(
        ctx,
        conn,
        Entry::new("users.email_changed", "user", user.id)
            .before(json!({ "email": user.email }))
            .after(json!({ "email": email }))
            .metadata(json!({ "source": "sso" })),
    )
    .await?;
    Ok(UserRow { email: email.to_string(), ..user })
}

/// Everything that signed in on its own goes: it would outlive the person being blocked at the provider.
async fn link(
    ctx: &Ctx,
    conn: &mut PgConnection,
    user: UserRow,
    issuer: &str,
    subject: &str,
) -> std::result::Result<(UserRow, u64), Refusal> {
    sqlx::query(
        "update users set sso = true, password_hash = null, totp_secret = null, totp_pending_secret = null, totp_enabled_at = null,
           totp_last_step = null, has_passkey = false, updated_at = now() where id = $1",
    )
    .bind(user.id)
    .execute(&mut *conn)
    .await?;
    let passkeys = sqlx::query("delete from passkeys where user_id = $1").bind(user.id).execute(&mut *conn).await?.rows_affected();
    let codes = sqlx::query("delete from recovery_codes where user_id = $1").bind(user.id).execute(&mut *conn).await?.rows_affected();
    expire_account_links(conn, user.id).await?;
    revoke_sessions(conn, user.id, None).await?;
    let keys = revoke_api_keys(conn, user.id).await?;
    add_identity(conn, user.id, issuer, subject).await?;
    audit(
        ctx,
        conn,
        Entry::new("auth.sso_linked", "user", user.id).metadata(json!({
            "issuer": issuer,
            "hadPassword": user.password_hash.is_some(),
            "passkeysRemoved": passkeys,
            "recoveryCodesRemoved": codes,
            "apiKeysRevoked": keys,
        })),
    )
    .await?;
    let linked = UserRow {
        sso: true,
        password_hash: None,
        totp_secret: None,
        totp_pending_secret: None,
        totp_enabled_at: None,
        totp_last_step: None,
        has_passkey: false,
        ..user
    };
    Ok((linked, keys))
}

async fn create(
    ctx: &Ctx,
    conn: &mut PgConnection,
    sso: &SsoSettings,
    issuer: &str,
    subject: &str,
    email: &str,
    claims: &Value,
) -> std::result::Result<UserRow, Refusal> {
    // Checked when saved too, but a pinned setting skips that. The first admin only comes from setup.
    let role = Role::parse(&sso.default_role).filter(|r| *r != Role::Admin).ok_or_else(|| refused("disabled"))?;
    let id = Uuid::now_v7();
    let user: UserRow = sqlx::query_as("insert into users (id, email, name, role, sso) values ($1, $2, $3, $4, true) returning *")
        .bind(id)
        .bind(email)
        .bind(display_name(claims, email))
        .bind(role.as_str())
        .fetch_one(&mut *conn)
        .await?;
    add_identity(conn, id, issuer, subject).await?;
    // An invitation to this address could carry another role; the account exists now.
    sqlx::query("update invites set revoked_at = now() where lower(email) = $1 and accepted_at is null and revoked_at is null")
        .bind(email)
        .execute(&mut *conn)
        .await?;
    audit(ctx, conn, Entry::new("auth.sso_user_created", "user", id).after(json!({ "email": email, "role": role, "issuer": issuer })))
        .await?;
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_the_token_proves() {
        assert!(email_verified(&json!({ "email_verified": true })));
        assert!(email_verified(&json!({ "email_verified": "true" })));
        assert!(email_verified(&json!({ "xms_edov": "1" })));
        assert!(!email_verified(&json!({ "email_verified": false })));
        assert!(!email_verified(&json!({ "email": "a@example.com" })));
        assert!(proves_mfa(&json!({ "amr": ["pwd", "otp"] })));
        assert!(proves_mfa(&json!({ "amr": ["mfa"] })));
        assert!(!proves_mfa(&json!({ "amr": ["pwd"] })));
        assert!(!proves_mfa(&json!({ "amr": "mfa" })));
        assert!(!proves_mfa(&json!({})));
    }

    #[test]
    fn production_needs_a_public_https_issuer() {
        assert!(check_issuer("https://login.example.com", false).is_ok());
        for bad in ["http://login.example.com", "https://127.0.0.1", "https://10.0.0.1", "ftp://x.example.com"] {
            assert!(check_issuer(bad, false).is_err(), "{bad}");
        }
        assert!(check_issuer("http://localhost:9000", true).is_ok());
        assert!(check_issuer("ftp://localhost", true).is_err());
    }
}
