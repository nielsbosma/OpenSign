use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{KeyPurpose, Totp, password_needs_rehash, random_token, recovery_codes, sha256_hex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use super::{Empty, Ok_, settings};
use crate::audit::{Entry, audit};
use crate::auth::{
    UserRow, burn_password, check_password, create_session, expire_account_links, new_password_hash, normalize_email, revoke_api_keys,
    revoke_sessions, user_by_email, user_by_id,
};
use crate::context::Ctx;
use crate::email::{self, Message};
use crate::error::{AppError, ErrorKind, Result, bad_request, unauthorized};
use crate::service::{Access, Meta, Service};
use crate::settings::{EmailSettings, OrgSettings, SecuritySettings};

const MAX_FAILURES: i32 = 5;
const CHALLENGE_MINUTES: i64 = 5;
const RESET_MINUTES: i64 = 60;
/// A quiet day forgets earlier failures. Waiting that long buys fewer guesses than the hourly
/// guess the longest lock allows anyway.
const FORGET_HOURS: i64 = 24;
const ALL_ADDRESSES: &str = "*";
/// Only this many failures from at least `ACCOUNT_LOCK_ADDRESSES` addresses lock everyone out, so
/// one address cannot do it to the owner alone.
const ACCOUNT_LOCK_FAILURES: i32 = 20;
const ACCOUNT_LOCK_ADDRESSES: i64 = 3;

fn invalid_login() -> AppError {
    AppError::new(ErrorKind::Unauthorized, "Wrong email or password")
}

fn locked() -> AppError {
    AppError::new(ErrorKind::TooManyRequests, "Too many attempts. Try again in a few minutes, or reset your password.")
}

fn lock_minutes(failures: i32) -> i64 {
    if failures < MAX_FAILURES { 0 } else { (1i64 << (failures - MAX_FAILURES).min(6)).min(60) }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginResult {
    /// The HTTP layer moves the token into a cookie and never returns it in a body.
    SignedIn {
        token: String,
        expires_at: DateTime<Utc>,
    },
    TwoFactorRequired {
        challenge: String,
    },
    /// The password was right; finish with the account's passkey through `auth.passkey_sign_in`.
    PasskeyRequired {
        ceremony: super::passkeys::Ceremony,
    },
}

pub(crate) async fn start_session(ctx: &Ctx, conn: &mut PgConnection, user: &UserRow) -> Result<LoginResult> {
    let security: SecuritySettings = settings(ctx, conn).await?;
    let session = create_session(conn, user.id, &security, ctx.ip.as_deref(), ctx.user_agent.as_deref(), ctx.now).await?;
    sqlx::query("update users set last_login_at = $2 where id = $1").bind(user.id).bind(ctx.now).execute(&mut *conn).await?;
    forget_address(ctx, conn, &login_subject(&user.email)).await?;
    let ctx =
        ctx.clone().with_actor(crate::auth::actor_for(user, &security, crate::context::Credential::Session).ok_or_else(invalid_login)?);
    audit(&ctx, conn, Entry::new("auth.sign_in", "user", user.id)).await?;
    Ok(LoginResult::SignedIn { token: session.token, expires_at: session.expires_at })
}

/// Failures are kept per typed email rather than per user, so an address nobody signs in with
/// throttles exactly like a real one.
pub(crate) fn login_subject(email: &str) -> String {
    sha256_hex(format!("login:{}", email.trim().to_lowercase()))
}

/// One bucket per IPv4 address or IPv6 /64, the block one customer usually gets.
fn login_address(ctx: &Ctx) -> String {
    let ip = ctx.ip.as_deref().and_then(|ip| ip.parse::<std::net::IpAddr>().ok()).map(|ip| ip.to_canonical());
    match ip {
        Some(std::net::IpAddr::V6(v6)) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Some(ip) => ip.to_string(),
        None => "unknown".into(),
    }
}

pub(crate) async fn login_locked(ctx: &Ctx, conn: &mut PgConnection, subject: &str) -> Result<bool> {
    Ok(sqlx::query_scalar("select exists (select 1 from login_failures where subject = $1 and address = any($2) and locked_until > $3)")
        .bind(subject)
        .bind(vec![login_address(ctx), ALL_ADDRESSES.to_string()])
        .bind(ctx.now)
        .fetch_one(&mut *conn)
        .await?)
}

async fn count_failure(ctx: &Ctx, conn: &mut PgConnection, subject: &str, address: &str) -> Result<i32> {
    Ok(sqlx::query_scalar(
        "insert into login_failures (subject, address, failures, last_failed_at) values ($1, $2, 1, $3)
         on conflict (subject, address) do update set
           failures = case when login_failures.last_failed_at < $3 - make_interval(hours => $4) then 1 else login_failures.failures + 1 end,
           last_failed_at = $3
         returning failures",
    )
    .bind(subject)
    .bind(address)
    .bind(ctx.now)
    .bind(FORGET_HOURS as i32)
    .fetch_one(&mut *conn)
    .await?)
}

async fn lock_login(ctx: &Ctx, conn: &mut PgConnection, subject: &str, address: &str, minutes: i64) -> Result<()> {
    sqlx::query("update login_failures set locked_until = greatest(locked_until, $3) where subject = $1 and address = $2")
        .bind(subject)
        .bind(address)
        .bind(ctx.now + Duration::minutes(minutes))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Counts in the database, so parallel guesses cannot all read the same count and skip the lockout.
pub(crate) async fn record_failure(ctx: &Ctx, conn: &mut PgConnection, subject: &str, user: Option<&UserRow>) -> Result<()> {
    let address = login_address(ctx);
    let here = count_failure(ctx, conn, subject, &address).await?;
    if lock_minutes(here) > 0 {
        lock_login(ctx, conn, subject, &address, lock_minutes(here)).await?;
    }
    let total = count_failure(ctx, conn, subject, ALL_ADDRESSES).await?;
    if total >= ACCOUNT_LOCK_FAILURES {
        let addresses: i64 = sqlx::query_scalar(
            "select count(*) from login_failures where subject = $1 and address <> $2 and last_failed_at >= $3 - make_interval(hours => $4)",
        )
        .bind(subject)
        .bind(ALL_ADDRESSES)
        .bind(ctx.now)
        .bind(FORGET_HOURS as i32)
        .fetch_one(&mut *conn)
        .await?;
        if addresses >= ACCOUNT_LOCK_ADDRESSES {
            let minutes = lock_minutes(total - ACCOUNT_LOCK_FAILURES + MAX_FAILURES);
            lock_login(ctx, conn, subject, ALL_ADDRESSES, minutes).await?;
        }
    }
    if let Some(user) = user {
        audit(
            ctx,
            conn,
            Entry::new("auth.sign_in_failed", "user", user.id).metadata(serde_json::json!({ "failures": here, "total": total })),
        )
        .await?;
    }
    Ok(())
}

/// A sign-in clears only what its own address did: the owner signing in must not hand a guesser
/// elsewhere a fresh set of tries.
async fn forget_address(ctx: &Ctx, conn: &mut PgConnection, subject: &str) -> Result<()> {
    let forgotten: Option<i32> = sqlx::query_scalar("delete from login_failures where subject = $1 and address = $2 returning failures")
        .bind(subject)
        .bind(login_address(ctx))
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(failures) = forgotten {
        sqlx::query(
            "update login_failures set failures = greatest(failures - $3, 0),
               locked_until = case when failures - $3 < $4 then null else locked_until end
             where subject = $1 and address = $2",
        )
        .bind(subject)
        .bind(ALL_ADDRESSES)
        .bind(failures)
        .bind(ACCOUNT_LOCK_FAILURES)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Sends after the transaction, in the background: nothing waits for the mail server, and the
/// response time does not tell who has an account.
pub(crate) async fn send_in_background(ctx: &Ctx, conn: &mut PgConnection, message: Message) -> Result<()> {
    let email_settings: EmailSettings = settings(ctx, conn).await?;
    let org: OrgSettings = settings(ctx, conn).await?;
    let app = ctx.app.clone();
    tokio::spawn(async move {
        if let Err(error) = email::send(&app, &email_settings, &org, &message).await {
            tracing::warn!(error = %error, subject = %message.subject, "account email failed");
        }
    });
    Ok(())
}

/// Tells the owner that lasting access ended, so a key an integration relied on does not just stop.
pub(crate) async fn notify_keys_revoked(ctx: &Ctx, conn: &mut PgConnection, user: &UserRow, keys: u64, reason: &str) -> Result<()> {
    if keys == 0 {
        return Ok(());
    }
    send_in_background(
        ctx,
        conn,
        Message {
            to: user.email.clone(),
            to_name: Some(user.name.clone()),
            subject: "Your OpenSign API keys were revoked".into(),
            paragraphs: vec![
                format!("Hi {},", user.name),
                format!("{reason} To keep your account safe, your API keys ({keys}) stopped working. Create new ones under Settings."),
            ],
            button: None,
            footer: Some("If you did not expect this, contact your OpenSign admin right away.".into()),
            attachments: vec![],
            lang: crate::i18n::Lang::En,
        },
    )
    .await
}

pub struct Login;

#[async_trait]
impl Service for Login {
    type Input = LoginInput;
    type Output = LoginResult;
    const META: Meta = Meta::mutation("auth.login", "Sign in with email and password.", Access::Public).without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: LoginInput) -> Result<LoginResult> {
        let subject = login_subject(&input.email);
        if login_locked(ctx, conn, &subject).await? {
            burn_password(&input.password).await?;
            return Err(locked());
        }
        let Some(user) = user_by_email(conn, &input.email).await? else {
            burn_password(&input.password).await?;
            record_failure(ctx, conn, &subject, None).await?;
            return Err(invalid_login());
        };
        let password_ok = match &user.password_hash {
            Some(hash) => check_password(hash, &input.password).await?,
            None => {
                burn_password(&input.password).await?;
                false
            }
        };
        if !password_ok || !user.active {
            record_failure(ctx, conn, &subject, Some(&user)).await?;
            return Err(invalid_login());
        }
        if let Some(hash) = &user.password_hash
            && password_needs_rehash(hash)
            && let Ok(new_hash) = new_password_hash(&input.password).await
        {
            sqlx::query("update users set password_hash = $2 where id = $1").bind(user.id).bind(new_hash).execute(&mut *conn).await?;
        }
        if user.totp_enabled_at.is_some() {
            let challenge = random_token();
            sqlx::query("insert into login_challenges (id, user_id, expires_at) values ($1, $2, $3)")
                .bind(sha256_hex(&challenge))
                .bind(user.id)
                .bind(ctx.now + Duration::minutes(CHALLENGE_MINUTES))
                .execute(&mut *conn)
                .await?;
            return Ok(LoginResult::TwoFactorRequired { challenge });
        }
        if user.has_passkey {
            return Ok(LoginResult::PasskeyRequired { ceremony: super::passkeys::begin_second_factor(ctx, conn, user.id).await? });
        }
        start_session(ctx, conn, &user).await
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TwoFactorInput {
    pub challenge: String,
    /// A 6-digit authenticator code or a recovery code.
    pub code: String,
}

pub struct VerifyTwoFactor;

#[async_trait]
impl Service for VerifyTwoFactor {
    type Input = TwoFactorInput;
    type Output = LoginResult;
    const META: Meta = Meta::mutation("auth.verify_two_factor", "Finish signing in with a two-factor or recovery code.", Access::Public)
        .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TwoFactorInput) -> Result<LoginResult> {
        let expired = || AppError::new(ErrorKind::Unauthorized, "This sign-in has expired. Start again.");
        let row: Option<(Uuid, i32, DateTime<Utc>)> =
            sqlx::query_as("update login_challenges set attempts = attempts + 1 where id = $1 returning user_id, attempts, expires_at")
                .bind(sha256_hex(&input.challenge))
                .fetch_optional(&mut *conn)
                .await?;
        let Some((user_id, attempts, expires_at)) = row else { return Err(expired()) };
        if expires_at <= ctx.now || attempts > MAX_FAILURES {
            sqlx::query("delete from login_challenges where id = $1").bind(sha256_hex(&input.challenge)).execute(&mut *conn).await?;
            return Err(expired());
        }
        let user = user_by_id(conn, user_id).await?.filter(|u| u.active).ok_or_else(expired)?;
        let subject = login_subject(&user.email);
        if login_locked(ctx, conn, &subject).await? {
            return Err(locked());
        }
        if !check_second_factor(ctx, conn, &user, &input.code).await? {
            record_failure(ctx, conn, &subject, Some(&user)).await?;
            return Err(AppError::new(ErrorKind::Unauthorized, "That code is not right"));
        }
        sqlx::query("delete from login_challenges where id = $1").bind(sha256_hex(&input.challenge)).execute(&mut *conn).await?;
        start_session(ctx, conn, &user).await
    }
}

/// A TOTP code (each one accepted once) or an unused recovery code (consumed).
async fn check_second_factor(ctx: &Ctx, conn: &mut PgConnection, user: &UserRow, code: &str) -> Result<bool> {
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() == 6 && compact.chars().all(|c| c.is_ascii_digit()) {
        let Some(sealed) = &user.totp_secret else { return Ok(false) };
        let secret = ctx.keys().decrypt_str(KeyPurpose::Settings, &format!("user:{}:totp", user.id), sealed)?;
        let totp = Totp::from_base32(&secret).map_err(AppError::internal)?;
        let Some(step) = totp.verify(&compact, ctx.now.timestamp(), user.totp_last_step) else { return Ok(false) };
        let updated = sqlx::query("update users set totp_last_step = $2 where id = $1 and (totp_last_step is null or totp_last_step < $2)")
            .bind(user.id)
            .bind(step)
            .execute(&mut *conn)
            .await?;
        return Ok(updated.rows_affected() == 1);
    }
    let normalized = compact.to_lowercase().replace('-', "");
    let used = sqlx::query("update recovery_codes set used_at = now() where user_id = $1 and code_hash = $2 and used_at is null")
        .bind(user.id)
        .bind(sha256_hex(&normalized))
        .execute(&mut *conn)
        .await?;
    if used.rows_affected() == 1 {
        audit(ctx, conn, Entry::new("auth.recovery_code_used", "user", user.id)).await?;
        return Ok(true);
    }
    Ok(false)
}

pub struct Logout;

#[async_trait]
impl Service for Logout {
    type Input = Empty;
    type Output = Ok_;
    const META: Meta = Meta::mutation("auth.logout", "Sign out of this browser.", Access::Authenticated)
        .not_a_tool("Ends the caller's own browser session.")
        .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Ok_> {
        if let Some(id) = &ctx.session_id {
            sqlx::query("delete from sessions where id = $1").bind(id).execute(&mut *conn).await?;
        }
        audit(ctx, conn, Entry::new("auth.sign_out", "user", ctx.actor()?.user_id)).await?;
        Ok(Ok_::default())
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: crate::permissions::Role,
    pub permissions: Vec<crate::permissions::Permission>,
    pub two_factor_enabled: bool,
    pub must_enable_two_factor: bool,
    /// The running OpenSign version.
    pub version: String,
    /// The language the app is shown in, if the user chose one.
    pub language: Option<String>,
}

pub struct WhoAmI;

#[async_trait]
impl Service for WhoAmI {
    type Input = Empty;
    type Output = Me;
    const META: Meta = Meta::query("account.me", "Who the signed-in user is, their role and permissions.", Access::Authenticated)
        .before_two_factor()
        .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Me> {
        let actor = ctx.actor()?;
        let user = user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?;
        Ok(Me {
            id: user.id,
            email: user.email,
            name: user.name,
            role: actor.role,
            permissions: actor.permissions.iter().copied().collect(),
            two_factor_enabled: user.totp_enabled_at.is_some(),
            must_enable_two_factor: actor.must_enable_two_factor,
            version: crate::env::app_version().to_string(),
            language: user.language,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetLanguageInput {
    /// en, zh, hi, es, fr, ar, bn, pt, ru, ur or sv. Leave out to follow the browser.
    pub language: Option<String>,
}

pub struct SetLanguage;

#[async_trait]
impl Service for SetLanguage {
    type Input = SetLanguageInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation("account.set_language", "Choose the language the app is shown in for you.", Access::Authenticated)
        .before_two_factor()
        .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SetLanguageInput) -> Result<Ok_> {
        let user_id = ctx.actor()?.user_id;
        let language = input.language.as_deref().map(|l| crate::services::documents::language(Some(l))).transpose()?;
        sqlx::query("update users set language = $2 where id = $1").bind(user_id).bind(&language).execute(&mut *conn).await?;
        audit(ctx, conn, Entry::new("account.language_set", "user", user_id).metadata(serde_json::json!({ "language": language }))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TwoFactorSetup {
    pub secret: String,
    pub otpauth_url: String,
}

pub struct BeginTwoFactor;

#[async_trait]
impl Service for BeginTwoFactor {
    type Input = Empty;
    type Output = TwoFactorSetup;
    const META: Meta = Meta::mutation("account.two_factor_begin", "Start setting up an authenticator app.", Access::Authenticated)
        .not_a_tool("Two-factor setup is a credential, so it stays in the app.")
        .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<TwoFactorSetup> {
        let actor = ctx.actor()?;
        let enabled: bool =
            sqlx::query_scalar("select totp_secret is not null from users where id = $1").bind(actor.user_id).fetch_one(&mut *conn).await?;
        if enabled {
            // Replacing the authenticator would let a stolen session lock the owner out; an Admin resets it instead.
            return Err(crate::error::conflict("Two-factor authentication is already on"));
        }
        let totp = Totp::generate();
        let secret = totp.to_base32();
        let sealed = ctx.keys().encrypt_str(KeyPurpose::Settings, &format!("user:{}:totp", actor.user_id), &secret);
        sqlx::query("update users set totp_pending_secret = $2 where id = $1").bind(actor.user_id).bind(sealed).execute(&mut *conn).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let issuer = if org.name.is_empty() { "OpenSign".to_string() } else { format!("OpenSign {}", org.name) };
        Ok(TwoFactorSetup { secret: secret.to_string(), otpauth_url: totp.otpauth_url(&issuer, &actor.email) })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct CodeInput {
    pub code: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCodes {
    pub recovery_codes: Vec<String>,
}

pub struct ConfirmTwoFactor;

#[async_trait]
impl Service for ConfirmTwoFactor {
    type Input = CodeInput;
    type Output = RecoveryCodes;
    const META: Meta =
        Meta::mutation("account.two_factor_confirm", "Turn on two-factor with a code from the authenticator app.", Access::Authenticated)
            .not_a_tool("Two-factor setup is a credential, so it stays in the app.")
            .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CodeInput) -> Result<RecoveryCodes> {
        let actor = ctx.actor()?;
        let user = user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?;
        let sealed = user.totp_pending_secret.ok_or_else(|| bad_request("Start the two-factor setup first"))?;
        let context = format!("user:{}:totp", user.id);
        let secret = ctx.keys().decrypt_str(KeyPurpose::Settings, &context, &sealed)?;
        let totp = Totp::from_base32(&secret).map_err(AppError::internal)?;
        let step = totp
            .verify(&input.code, ctx.now.timestamp(), None)
            .ok_or_else(|| bad_request("That code is not right. Check the time on your phone."))?;
        sqlx::query(
            "update users set totp_secret = $2, totp_pending_secret = null, totp_enabled_at = $3, totp_last_step = $4 where id = $1",
        )
        .bind(user.id)
        .bind(&sealed)
        .bind(ctx.now)
        .bind(step)
        .execute(&mut *conn)
        .await?;
        let codes = replace_recovery_codes(conn, user.id).await?;
        revoke_sessions(conn, user.id, ctx.session_id.as_deref()).await?;
        audit(ctx, conn, Entry::new("account.two_factor_enabled", "user", user.id)).await?;
        Ok(RecoveryCodes { recovery_codes: codes })
    }
}

pub async fn replace_recovery_codes(conn: &mut PgConnection, user_id: Uuid) -> Result<Vec<String>> {
    sqlx::query("delete from recovery_codes where user_id = $1").bind(user_id).execute(&mut *conn).await?;
    let codes = recovery_codes();
    for code in &codes {
        sqlx::query("insert into recovery_codes (id, user_id, code_hash) values ($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(user_id)
            .bind(sha256_hex(code.replace('-', "")))
            .execute(&mut *conn)
            .await?;
    }
    Ok(codes)
}

/// A signed-in check of the password. Wrong guesses count like those at the sign-in page, so a
/// stolen session is no quicker way to learn the password. Call it outside a transaction: the
/// count has to survive the refusal.
pub(crate) async fn confirm_password(ctx: &Ctx, conn: &mut PgConnection, user: &UserRow, password: Option<&str>) -> Result<bool> {
    let subject = login_subject(&user.email);
    if login_locked(ctx, conn, &subject).await? {
        return Err(locked());
    }
    let right = match (&user.password_hash, password) {
        (Some(hash), Some(password)) => check_password(hash, password).await?,
        _ => false,
    };
    if !right {
        record_failure(ctx, conn, &subject, Some(user)).await?;
    }
    Ok(right)
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}

pub struct ChangePassword;

#[async_trait]
impl Service for ChangePassword {
    type Input = ChangePasswordInput;
    type Output = Ok_;
    const META: Meta =
        Meta::mutation("account.change_password", "Change your password. Signs out your other sessions.", Access::Authenticated)
            .not_a_tool("Passwords never pass through an agent.")
            .before_two_factor()
            .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ChangePasswordInput) -> Result<Ok_> {
        let actor = ctx.actor()?;
        let user = user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?;
        if !confirm_password(ctx, conn, &user, Some(&input.current_password)).await? {
            return Err(bad_request("Your current password is not right"));
        }
        let hash = new_password_hash(&input.new_password).await?;
        let mut tx = conn.begin().await?;
        sqlx::query("update users set password_hash = $2, password_changed_at = now() where id = $1")
            .bind(user.id)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
        expire_account_links(&mut tx, user.id).await?;
        revoke_sessions(&mut tx, user.id, ctx.session_id.as_deref()).await?;
        audit(ctx, &mut tx, Entry::new("account.password_changed", "user", user.id)).await?;
        tx.commit().await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct EmailInput {
    pub email: String,
}

pub struct RequestPasswordReset;

#[async_trait]
impl Service for RequestPasswordReset {
    type Input = EmailInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation("auth.request_password_reset", "Email a password reset link.", Access::Public).without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: EmailInput) -> Result<Ok_> {
        let Ok(email) = normalize_email(&input.email) else { return Ok(Ok_::default()) };
        let Some(user) = user_by_email(conn, &email).await?.filter(|u| u.active) else { return Ok(Ok_::default()) };
        let recent: i64 =
            sqlx::query_scalar("select count(*) from password_reset_tokens where user_id = $1 and created_at > now() - interval '1 hour'")
                .bind(user.id)
                .fetch_one(&mut *conn)
                .await?;
        if recent >= 3 {
            return Ok(Ok_::default());
        }
        let token = random_token();
        let mut tx = conn.begin().await?;
        sqlx::query("insert into password_reset_tokens (id, user_id, expires_at) values ($1, $2, $3)")
            .bind(sha256_hex(&token))
            .bind(user.id)
            .bind(ctx.now + Duration::minutes(RESET_MINUTES))
            .execute(&mut *tx)
            .await?;
        audit(ctx, &mut tx, Entry::new("auth.password_reset_requested", "user", user.id)).await?;
        tx.commit().await?;
        let link = format!("{}/reset-password#{token}", ctx.app.public_url());
        send_in_background(
            ctx,
            conn,
            Message {
                to: user.email.clone(),
                to_name: Some(user.name.clone()),
                subject: "Reset your OpenSign password".into(),
                paragraphs: vec![
                    format!("Hi {},", user.name),
                    "Someone asked to reset the password of your account. If it was you, choose a new one within an hour.".into(),
                ],
                button: Some(("Choose a New Password".into(), link)),
                footer: Some("If you did not ask for this, ignore this email; your password stays the same.".into()),
                attachments: vec![],
                lang: crate::i18n::Lang::En,
            },
        )
        .await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ResetPasswordInput {
    pub token: String,
    pub password: String,
}

pub struct ResetPassword;

#[async_trait]
impl Service for ResetPassword {
    type Input = ResetPasswordInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation("auth.reset_password", "Set a new password with a reset link.", Access::Public).without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ResetPasswordInput) -> Result<Ok_> {
        let hash = new_password_hash(&input.password).await?;
        let mut tx = conn.begin().await?;
        let user_id: Option<Uuid> = sqlx::query_scalar(
            "update password_reset_tokens set used_at = now() where id = $1 and used_at is null and expires_at > $2 returning user_id",
        )
        .bind(sha256_hex(&input.token))
        .bind(ctx.now)
        .fetch_optional(&mut *tx)
        .await?;
        let user_id = user_id.ok_or_else(|| bad_request("This link has expired or was already used. Ask for a new one."))?;
        let user = user_by_id(&mut tx, user_id)
            .await?
            .ok_or_else(|| bad_request("This link has expired or was already used. Ask for a new one."))?;
        sqlx::query("update users set password_hash = $2, password_changed_at = now() where id = $1")
            .bind(user_id)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
        sqlx::query("delete from login_failures where subject = $1").bind(login_subject(&user.email)).execute(&mut *tx).await?;
        expire_account_links(&mut tx, user_id).await?;
        revoke_sessions(&mut tx, user_id, None).await?;
        let keys = revoke_api_keys(&mut tx, user_id).await?;
        audit(ctx, &mut tx, Entry::new("auth.password_reset", "user", user_id).metadata(serde_json::json!({ "apiKeysRevoked": keys })))
            .await?;
        tx.commit().await?;
        notify_keys_revoked(ctx, conn, &user, keys, "Your password was reset.").await?;
        Ok(Ok_::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockout_grows_and_caps() {
        assert_eq!(lock_minutes(4), 0);
        assert_eq!(lock_minutes(5), 1);
        assert_eq!(lock_minutes(7), 4);
        assert_eq!(lock_minutes(50), 60);
    }
}
