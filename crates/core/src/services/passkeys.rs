use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{random_bytes, sha256_hex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use super::Ok_;
use super::auth::{LoginResult, confirm_password, start_session};
use crate::audit::{Entry, audit};
use crate::auth::{STEP_UP_MINUTES, user_by_id};
use crate::context::Ctx;
use crate::error::{AppError, ErrorKind, Result, bad_request, not_found, unauthorized};
use crate::service::{Access, Meta, Service};
use crate::webauthn::{self, EDDSA, ES256, RS256, RelyingParty};

const CHALLENGE_MINUTES: i64 = 5;
const MAX_PER_USER: i64 = 10;

fn rp(ctx: &Ctx) -> Result<RelyingParty> {
    RelyingParty::from_url(&ctx.app.public_url())
}

async fn new_challenge(ctx: &Ctx, conn: &mut PgConnection, user: Option<Uuid>, purpose: &str) -> Result<(String, [u8; 32])> {
    let challenge = random_bytes::<32>();
    let state = webauthn::encode(&challenge);
    sqlx::query("delete from webauthn_challenges where expires_at < $1").bind(ctx.now).execute(&mut *conn).await?;
    sqlx::query("insert into webauthn_challenges (id, user_id, purpose, expires_at) values ($1, $2, $3, $4)")
        .bind(sha256_hex(&state))
        .bind(user)
        .bind(purpose)
        .bind(ctx.now + Duration::minutes(CHALLENGE_MINUTES))
        .execute(&mut *conn)
        .await?;
    Ok((state, challenge))
}

/// Consumes a challenge: each one is good for a single attempt.
async fn take_challenge(ctx: &Ctx, conn: &mut PgConnection, state: &str, purposes: &[&str]) -> Result<(Vec<u8>, Option<Uuid>)> {
    let expired = || AppError::new(ErrorKind::Unauthorized, "The passkey request has expired. Try again.");
    let row: Option<(Option<Uuid>, DateTime<Utc>)> =
        sqlx::query_as("delete from webauthn_challenges where id = $1 and purpose = any($2) returning user_id, expires_at")
            .bind(sha256_hex(state))
            .bind(purposes)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((user, expires_at)) = row else { return Err(expired()) };
    if expires_at <= ctx.now {
        return Err(expired());
    }
    Ok((webauthn::decode(state).map_err(|_| expired())?, user))
}

/// A request for one of the user's own passkeys, by name, so the browser does not offer others.
async fn own_passkey_ceremony(ctx: &Ctx, conn: &mut PgConnection, user: Uuid, purpose: &str) -> Result<Ceremony> {
    let (state, challenge) = new_challenge(ctx, conn, Some(user), purpose).await?;
    let ids: Vec<Vec<u8>> =
        sqlx::query_scalar("select credential_id from passkeys where user_id = $1").bind(user).fetch_all(&mut *conn).await?;
    Ok(Ceremony {
        state,
        options: json!({
            "challenge": webauthn::encode(&challenge),
            "rpId": rp(ctx)?.id,
            "timeout": CHALLENGE_MINUTES * 60_000,
            "userVerification": "required",
            "allowCredentials": ids.iter().map(|id| json!({ "type": "public-key", "id": webauthn::encode(id) })).collect::<Vec<_>>(),
        }),
    })
}

pub(crate) async fn begin_second_factor(ctx: &Ctx, conn: &mut PgConnection, user: Uuid) -> Result<Ceremony> {
    own_passkey_ceremony(ctx, conn, user, "second_factor").await
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Ceremony {
    /// Send back unchanged with the browser's response.
    pub state: String,
    /// `PublicKeyCredentialCreationOptions` or `...RequestOptions`, with binary values base64url-encoded.
    pub options: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BeginInput {
    /// Your password, to confirm it is you before a new way to sign in is added.
    pub current_password: Option<String>,
}

pub struct BeginRegistration;

#[async_trait]
impl Service for BeginRegistration {
    type Input = BeginInput;
    type Output = Ceremony;
    const META: Meta = Meta::mutation("account.passkey_begin", "Start adding a passkey to your account.", Access::Authenticated)
        .not_a_tool("Passkeys are credentials registered by the browser.")
        .before_two_factor()
        .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: BeginInput) -> Result<Ceremony> {
        let actor = ctx.actor()?;
        let user = user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?;
        user.refuse_sso()?;
        if user.password_hash.is_none() {
            crate::auth::require_step_up(ctx, conn).await?;
        } else if !confirm_password(ctx, conn, &user, input.current_password.as_deref()).await? {
            return Err(bad_request("Your password is not right"));
        }
        let count: i64 = sqlx::query_scalar("select count(*) from passkeys where user_id = $1").bind(user.id).fetch_one(&mut *conn).await?;
        if count >= MAX_PER_USER {
            return Err(bad_request(format!("You can have at most {MAX_PER_USER} passkeys")));
        }
        let existing: Vec<Vec<u8>> =
            sqlx::query_scalar("select credential_id from passkeys where user_id = $1").bind(user.id).fetch_all(&mut *conn).await?;
        let (state, challenge) = new_challenge(ctx, conn, Some(user.id), "register").await?;
        let rp = rp(ctx)?;
        let org: crate::settings::OrgSettings = super::settings(ctx, conn).await?;
        Ok(Ceremony {
            state,
            options: json!({
                "challenge": webauthn::encode(&challenge),
                "rp": { "id": rp.id, "name": if org.name.is_empty() { "OpenSign".to_string() } else { org.name } },
                "user": { "id": webauthn::encode(user.id.as_bytes()), "name": user.email, "displayName": user.name },
                "pubKeyCredParams": ([ES256, EDDSA, RS256].map(|alg| json!({ "type": "public-key", "alg": alg }))),
                "timeout": CHALLENGE_MINUTES * 60_000,
                "attestation": "none",
                "authenticatorSelection": { "residentKey": "required", "requireResidentKey": true, "userVerification": "required" },
                "excludeCredentials": existing.iter().map(|id| json!({ "type": "public-key", "id": webauthn::encode(id) })).collect::<Vec<_>>(),
            }),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FinishRegistrationInput {
    pub state: String,
    /// A name to tell your passkeys apart, e.g. "MacBook" or "Phone".
    pub name: String,
    /// base64url
    pub client_data_json: String,
    /// base64url
    pub attestation_object: String,
    #[serde(default)]
    pub transports: Vec<String>,
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PasskeyView {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

async fn mark_user(conn: &mut PgConnection, user: Uuid) -> Result<()> {
    sqlx::query("update users set has_passkey = exists (select 1 from passkeys where user_id = $1) where id = $1")
        .bind(user)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub struct FinishRegistration;

#[async_trait]
impl Service for FinishRegistration {
    type Input = FinishRegistrationInput;
    type Output = PasskeyView;
    const META: Meta = Meta::mutation("account.passkey_add", "Finish adding a passkey with the browser's response.", Access::Authenticated)
        .not_a_tool("Passkeys are credentials registered by the browser.")
        .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: FinishRegistrationInput) -> Result<PasskeyView> {
        let actor = ctx.actor()?;
        user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?.refuse_sso()?;
        let (challenge, owner) = take_challenge(ctx, conn, &input.state, &["register"]).await?;
        if owner != Some(actor.user_id) {
            return Err(unauthorized());
        }
        let name: String = input.name.trim().chars().filter(|c| !c.is_control()).take(60).collect();
        let name = if name.is_empty() { "Passkey".to_string() } else { name };
        let credential = webauthn::verify_registration(
            &rp(ctx)?,
            &challenge,
            &webauthn::decode(&input.client_data_json)?,
            &webauthn::decode(&input.attestation_object)?,
        )?;
        let transports: Vec<String> = input
            .transports
            .iter()
            .filter(|t| matches!(t.as_str(), "usb" | "nfc" | "ble" | "internal" | "hybrid" | "smart-card"))
            .cloned()
            .collect();
        let id = Uuid::now_v7();
        let inserted = sqlx::query(
            "insert into passkeys (id, user_id, credential_id, public_key, sign_count, name, transports) values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (credential_id) do nothing",
        )
        .bind(id)
        .bind(actor.user_id)
        .bind(&credential.id)
        .bind(&credential.public_key)
        .bind(i64::from(credential.sign_count))
        .bind(&name)
        .bind(&transports)
        .execute(&mut *conn)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(bad_request("This passkey is already registered"));
        }
        mark_user(conn, actor.user_id).await?;
        audit(ctx, conn, Entry::new("account.passkey_added", "user", actor.user_id).metadata(json!({ "passkey": id, "name": name })))
            .await?;
        Ok(PasskeyView { id, name, created_at: ctx.now, last_used_at: None })
    }
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = super::Empty;
    type Output = Vec<PasskeyView>;
    const META: Meta = Meta::query("account.passkeys", "Your passkeys.", Access::Authenticated)
        .not_a_tool("Passkeys are credentials, managed in the app.")
        .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<Vec<PasskeyView>> {
        Ok(sqlx::query_as("select id, name, created_at, last_used_at from passkeys where user_id = $1 order by created_at")
            .bind(ctx.actor()?.user_id)
            .fetch_all(&mut *conn)
            .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveInput {
    pub id: Uuid,
}

pub struct Remove;

#[async_trait]
impl Service for Remove {
    type Input = RemoveInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation("account.passkey_remove", "Remove one of your passkeys.", Access::Authenticated)
        .not_a_tool("Passkeys are credentials, managed in the app.")
        .before_two_factor();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: RemoveInput) -> Result<Ok_> {
        let actor = ctx.actor()?;
        let removed = sqlx::query("delete from passkeys where id = $1 and user_id = $2")
            .bind(input.id)
            .bind(actor.user_id)
            .execute(&mut *conn)
            .await?;
        if removed.rows_affected() == 0 {
            return Err(not_found("Passkey"));
        }
        mark_user(conn, actor.user_id).await?;
        audit(ctx, conn, Entry::new("account.passkey_removed", "user", actor.user_id).metadata(json!({ "passkey": input.id }))).await?;
        Ok(Ok_::default())
    }
}

pub struct BeginSignIn;

#[async_trait]
impl Service for BeginSignIn {
    type Input = super::Empty;
    type Output = Ceremony;
    const META: Meta = Meta::mutation("auth.passkey_begin", "Start signing in with a passkey.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<Ceremony> {
        let (state, challenge) = new_challenge(ctx, conn, None, "sign_in").await?;
        Ok(Ceremony {
            state,
            options: json!({
                "challenge": webauthn::encode(&challenge),
                "rpId": rp(ctx)?.id,
                "timeout": CHALLENGE_MINUTES * 60_000,
                "userVerification": "required",
                "allowCredentials": [],
            }),
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignInInput {
    pub state: String,
    /// The credential's rawId, base64url.
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    pub user_handle: Option<String>,
}

/// Checks a browser's response against a stored passkey and moves its counter on.
async fn verify_passkey(ctx: &Ctx, conn: &mut PgConnection, challenge: &[u8], input: &SignInInput, passkey: StoredPasskey) -> Result<()> {
    let verified = webauthn::verify_assertion(
        &rp(ctx)?,
        challenge,
        &passkey.public_key,
        u32::try_from(passkey.sign_count).unwrap_or(u32::MAX),
        &webauthn::decode(&input.client_data_json)?,
        &webauthn::decode(&input.authenticator_data)?,
        &webauthn::decode(&input.signature)?,
    );
    let new_count = match verified {
        Ok(c) => c,
        Err(e) => {
            audit(ctx, conn, Entry::new("auth.passkey_failed", "user", passkey.user_id).metadata(json!({ "passkey": passkey.id }))).await?;
            return Err(e);
        }
    };
    sqlx::query("update passkeys set sign_count = greatest(sign_count, $2), last_used_at = $3 where id = $1")
        .bind(passkey.id)
        .bind(i64::from(new_count))
        .bind(ctx.now)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct StoredPasskey {
    id: Uuid,
    user_id: Uuid,
    public_key: Vec<u8>,
    sign_count: i64,
}

async fn stored_passkey(conn: &mut PgConnection, input: &SignInInput) -> Result<Option<StoredPasskey>> {
    Ok(sqlx::query_as("select id, user_id, public_key, sign_count from passkeys where credential_id = $1")
        .bind(webauthn::decode(&input.credential_id)?)
        .fetch_optional(&mut *conn)
        .await?)
}

pub struct SignIn;

#[async_trait]
impl Service for SignIn {
    type Input = SignInInput;
    type Output = LoginResult;
    const META: Meta = Meta::mutation(
        "auth.passkey_sign_in",
        "Finish signing in with a passkey, on its own or after the password when the account asks for it. A passkey verifies you on the device, so no second code is asked for.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: SignInInput) -> Result<LoginResult> {
        let failed = || AppError::new(ErrorKind::Unauthorized, "That passkey cannot sign in here");
        let (challenge, bound_to) = take_challenge(ctx, conn, &input.state, &["sign_in", "second_factor"]).await?;
        let Some(passkey) = stored_passkey(conn, &input).await? else { return Err(failed()) };
        if bound_to.is_some_and(|user| user != passkey.user_id) {
            return Err(failed());
        }
        if let Some(handle) = &input.user_handle
            && !handle.is_empty()
            && webauthn::decode(handle)? != passkey.user_id.as_bytes()
        {
            return Err(failed());
        }
        let user = user_by_id(conn, passkey.user_id).await?.filter(|u| u.active).ok_or_else(failed)?;
        // Guessing someone's password locks passwords, not this: the owner still has to be able to get in.
        verify_passkey(ctx, conn, &challenge, &input, passkey).await?;
        start_session(ctx, conn, &user).await
    }
}

pub struct BeginStepUp;

#[async_trait]
impl Service for BeginStepUp {
    type Input = super::Empty;
    type Output = StepUpOptions;
    const META: Meta = Meta::mutation(
        "account.step_up_begin",
        "Start confirming it is you before something that needs it, with your password or one of your passkeys.",
        Access::Authenticated,
    )
    .not_a_tool("Confirming who you are takes the person, not an agent.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: super::Empty) -> Result<StepUpOptions> {
        let user = user_by_id(conn, ctx.actor()?.user_id).await?.ok_or_else(unauthorized)?;
        user.refuse_sso()?;
        let passkey = if user.has_passkey { Some(own_passkey_ceremony(ctx, conn, user.id, "step_up").await?) } else { None };
        Ok(StepUpOptions { has_password: user.password_hash.is_some(), passkey })
    }
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepUpOptions {
    pub has_password: bool,
    /// Present when the account has a passkey.
    pub passkey: Option<Ceremony>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepUpInput {
    /// Your password. Leave out when you confirm with a passkey.
    pub current_password: Option<String>,
    /// The browser's passkey response. Leave out when you confirm with your password.
    pub passkey: Option<SignInInput>,
}

pub struct StepUp;

#[async_trait]
impl Service for StepUp {
    type Input = StepUpInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "account.step_up",
        "Confirm it is you with your password or a passkey, for the next ten minutes in this session. Needed before creating an API key or connecting an app.",
        Access::Authenticated,
    )
    .not_a_tool("Confirming who you are takes the person, not an agent.")
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: StepUpInput) -> Result<Ok_> {
        let actor = ctx.actor()?;
        let session = ctx.session_id.as_deref().ok_or_else(crate::error::forbidden)?;
        let user = user_by_id(conn, actor.user_id).await?.ok_or_else(unauthorized)?;
        user.refuse_sso()?;
        let method = if let Some(passkey) = &input.passkey {
            let failed = || AppError::new(ErrorKind::Unauthorized, "That passkey does not belong to your account");
            let (challenge, owner) = take_challenge(ctx, conn, &passkey.state, &["step_up"]).await?;
            let stored = stored_passkey(conn, passkey).await?.filter(|p| p.user_id == user.id).ok_or_else(failed)?;
            if owner != Some(user.id) {
                return Err(failed());
            }
            verify_passkey(ctx, conn, &challenge, passkey, stored).await?;
            "passkey"
        } else {
            if !confirm_password(ctx, conn, &user, input.current_password.as_deref()).await? {
                return Err(bad_request("Your password is not right"));
            }
            "password"
        };
        sqlx::query("update sessions set elevated_until = $2 where id = $1")
            .bind(session)
            .bind(ctx.now + Duration::minutes(STEP_UP_MINUTES))
            .execute(&mut *conn)
            .await?;
        audit(ctx, conn, Entry::new("account.stepped_up", "user", user.id).metadata(json!({ "method": method }))).await?;
        Ok(Ok_::default())
    }
}
