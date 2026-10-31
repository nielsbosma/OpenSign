use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{random_token, sha256_hex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use super::auth::{notify_keys_revoked, send_in_background};
use super::{Empty, Ok_, settings};
use crate::audit::{Entry, audit};
use crate::auth::{
    API_KEY_PREFIX, expire_account_links, ips_no_wider, new_password_hash, normalize_allowed_ips, normalize_email, require_step_up,
    revoke_api_keys, revoke_sessions, user_by_email, user_by_id,
};
use crate::context::{Credential, Ctx};
use crate::email::{self, Message};
use crate::error::{AppError, ErrorKind, Result, bad_request, conflict, forbidden, not_found};
use crate::permissions::{Permission, Role};
use crate::service::{Access, Meta, Service};
use crate::settings::{EmailSettings, OrgSettings};

const INVITE_DAYS: i64 = 7;
const EMAIL_CHANGE_HOURS: i64 = 24;

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub active: bool,
    pub two_factor_enabled: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct List;

#[async_trait]
impl Service for List {
    type Input = Empty;
    type Output = Vec<UserView>;
    const META: Meta = Meta::query(
        "users.list",
        "List everyone with an account, their role and whether they use two-factor.",
        Access::Permission(Permission::UsersRead),
    )
    .tool();

    async fn run(_ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<UserView>> {
        Ok(sqlx::query_as(
            "select id, email, name, role, active, totp_enabled_at is not null as two_factor_enabled, last_login_at, created_at from users order by lower(name)",
        )
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct InviteInput {
    pub email: String,
    pub name: Option<String>,
    pub role: Role,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub expires_at: DateTime<Utc>,
}

pub struct Invite;

#[async_trait]
impl Service for Invite {
    type Input = InviteInput;
    type Output = InviteView;
    const META: Meta = Meta::mutation(
        "users.invite",
        "Invite someone by email with a role. They choose their own password.",
        Access::Permission(Permission::UsersManage),
    )
    .tool()
    .open_world()
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: InviteInput) -> Result<InviteView> {
        let email = normalize_email(&input.email)?;
        if user_by_email(conn, &email).await?.is_some() {
            return Err(conflict("Someone with that email already has an account"));
        }
        let token = random_token();
        let id = Uuid::now_v7();
        let expires_at = ctx.now + Duration::days(INVITE_DAYS);
        let mut tx = conn.begin().await?;
        sqlx::query("update invites set revoked_at = now() where lower(email) = $1 and accepted_at is null and revoked_at is null")
            .bind(&email)
            .execute(&mut *tx)
            .await?;
        sqlx::query("insert into invites (id, email, name, role, token_hash, invited_by, expires_at) values ($1, $2, $3, $4, $5, $6, $7)")
            .bind(id)
            .bind(&email)
            .bind(input.name.as_deref().map(str::trim))
            .bind(input.role.as_str())
            .bind(sha256_hex(&token))
            .bind(ctx.actor()?.user_id)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
        audit(ctx, &mut tx, Entry::new("users.invite", "invite", id).after(serde_json::json!({ "email": email, "role": input.role })))
            .await?;
        tx.commit().await?;
        let email_settings: EmailSettings = settings(ctx, conn).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        let inviter = ctx.actor()?.name.clone();
        email::send(
            &ctx.app,
            &email_settings,
            &org,
            &Message {
                to: email.clone(),
                to_name: input.name.clone(),
                subject: format!("{inviter} invited you to {}", if org.name.is_empty() { "OpenSign" } else { &org.name }),
                paragraphs: vec![format!(
                    "{inviter} invited you to sign documents with {} in OpenSign. The invitation is valid for {INVITE_DAYS} days.",
                    org.name
                )],
                button: Some(("Accept Invitation".into(), format!("{}/invite#{token}", ctx.app.public_url()))),
                footer: None,
                attachments: vec![],
                lang: crate::i18n::Lang::En,
            },
        )
        .await?;
        Ok(InviteView { id, email, role: input.role.as_str().into(), expires_at })
    }
}

pub struct ListInvites;

#[async_trait]
impl Service for ListInvites {
    type Input = Empty;
    type Output = Vec<InviteView>;
    const META: Meta =
        Meta::query("users.list_invites", "List invitations that have not been accepted yet.", Access::Permission(Permission::UsersManage))
            .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<InviteView>> {
        let rows: Vec<(Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "select id, email, role, expires_at from invites where accepted_at is null and revoked_at is null and expires_at > $1 order by created_at desc",
        )
        .bind(ctx.now)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows.into_iter().map(|(id, email, role, expires_at)| InviteView { id, email, role, expires_at }).collect())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct IdInput {
    pub id: Uuid,
}

pub struct RevokeInvite;

#[async_trait]
impl Service for RevokeInvite {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "users.revoke_invite",
        "Cancel an invitation so its link stops working.",
        Access::Permission(Permission::UsersManage),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let done = sqlx::query("update invites set revoked_at = now() where id = $1 and accepted_at is null and revoked_at is null")
            .bind(input.id)
            .execute(&mut *conn)
            .await?;
        if done.rows_affected() == 0 {
            return Err(not_found("Invitation"));
        }
        audit(ctx, conn, Entry::new("users.revoke_invite", "invite", input.id)).await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct AcceptInput {
    pub token: String,
    pub name: String,
    pub password: String,
}

pub struct AcceptInvite;

#[async_trait]
impl Service for AcceptInvite {
    type Input = AcceptInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation("users.accept_invite", "Create an account from an invitation link.", Access::Public);

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: AcceptInput) -> Result<Ok_> {
        let hash = new_password_hash(&input.password).await?;
        let name = input.name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(bad_request("Enter your name"));
        }
        let invite: Option<(Uuid, String, String)> = sqlx::query_as(
            "select i.id, i.email, i.role from invites i where i.token_hash = $1 and i.accepted_at is null and i.revoked_at is null and i.expires_at > $2
               and (i.invited_by is null or exists (select 1 from users u where u.id = i.invited_by and u.active)) for update of i",
        )
        .bind(sha256_hex(&input.token))
        .bind(ctx.now)
        .fetch_optional(&mut *conn)
        .await?;
        let (invite_id, email, role) =
            invite.ok_or_else(|| bad_request("This invitation has expired or was already used. Ask for a new one."))?;
        if user_by_email(conn, &email).await?.is_some() {
            return Err(conflict("You already have an account. Sign in instead."));
        }
        let id = Uuid::now_v7();
        sqlx::query("insert into users (id, email, name, password_hash, role) values ($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(&email)
            .bind(name)
            .bind(hash)
            .bind(&role)
            .execute(&mut *conn)
            .await?;
        sqlx::query("update invites set accepted_at = now(), accepted_user_id = $2 where id = $1")
            .bind(invite_id)
            .bind(id)
            .execute(&mut *conn)
            .await?;
        audit(ctx, conn, Entry::new("users.accept_invite", "user", id).after(serde_json::json!({ "email": email, "role": role }))).await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct UpdateInput {
    pub id: Uuid,
    pub name: Option<String>,
    pub role: Option<Role>,
    pub active: Option<bool>,
}

pub struct Update;

/// Locks every admin row, in one order, so two admins demoting each other at once take turns and
/// the second one sees the first one's change.
async fn other_active_admins(conn: &mut PgConnection, except: Uuid) -> Result<usize> {
    let admins: Vec<Uuid> =
        sqlx::query_scalar("select id from users where role = 'admin' and active order by id for update").fetch_all(&mut *conn).await?;
    Ok(admins.into_iter().filter(|id| *id != except).count())
}

#[async_trait]
impl Service for Update {
    type Input = UpdateInput;
    type Output = UserView;
    const META: Meta = Meta::mutation(
        "users.update",
        "Change someone's name or role, or deactivate or reactivate their account. Use users.change_email for their email.",
        Access::Permission(Permission::UsersManage),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateInput) -> Result<UserView> {
        let user = user_by_id(conn, input.id).await?.ok_or_else(|| not_found("User"))?;
        let actor = ctx.actor()?;
        let role = input.role.map(|r| r.as_str().to_string()).unwrap_or(user.role.clone());
        let active = input.active.unwrap_or(user.active);
        if user.id == actor.user_id && (!active || role != user.role) {
            return Err(bad_request("You cannot change your own role or deactivate yourself. Ask another admin."));
        }
        if user.role == "admin" && (role != "admin" || !active) && other_active_admins(conn, user.id).await? == 0 {
            return Err(bad_request("There must always be at least one active admin"));
        }
        let name = input.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&user.name).to_string();
        sqlx::query("update users set name = $2, role = $3, active = $4, updated_at = now() where id = $1")
            .bind(user.id)
            .bind(&name)
            .bind(&role)
            .bind(active)
            .execute(&mut *conn)
            .await?;
        let mut invites_revoked = 0;
        if !active || role != user.role {
            revoke_sessions(conn, user.id, None).await?;
            sqlx::query("update api_keys set revoked_at = now() where user_id = $1 and revoked_at is null and $2")
                .bind(user.id)
                .bind(!active)
                .execute(&mut *conn)
                .await?;
            // What they invited people to do is no longer theirs to grant.
            invites_revoked =
                sqlx::query("update invites set revoked_at = now() where invited_by = $1 and accepted_at is null and revoked_at is null")
                    .bind(user.id)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected();
        }
        audit(
            ctx,
            conn,
            Entry::new("users.update", "user", user.id)
                .before(serde_json::json!({ "name": user.name, "role": user.role, "active": user.active }))
                .after(serde_json::json!({ "name": name, "role": role, "active": active }))
                .metadata(serde_json::json!({ "invitesRevoked": invites_revoked })),
        )
        .await?;
        Ok(UserView {
            id: user.id,
            email: user.email,
            name,
            role,
            active,
            two_factor_enabled: user.totp_enabled_at.is_some(),
            last_login_at: user.last_login_at,
            created_at: user.created_at,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct ChangeEmailInput {
    pub id: Uuid,
    /// The new address. It only takes effect once someone opens the link sent to it.
    pub email: String,
}

pub struct ChangeEmail;

#[async_trait]
impl Service for ChangeEmail {
    type Input = ChangeEmailInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "users.change_email",
        "Change someone's sign-in email. A confirmation link goes to the new address, and the change applies once it is opened.",
        Access::Permission(Permission::UsersManage),
    )
    .tool()
    .destructive()
    .open_world()
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: ChangeEmailInput) -> Result<Ok_> {
        let actor = ctx.actor()?;
        if actor.credential != Credential::Session {
            return Err(AppError::new(ErrorKind::Forbidden, "Change email addresses in the app, signed in"));
        }
        let user = user_by_id(conn, input.id).await?.ok_or_else(|| not_found("User"))?;
        let email = normalize_email(&input.email)?;
        if email.eq_ignore_ascii_case(&user.email) {
            return Err(bad_request("That is already their email"));
        }
        if user_by_email(conn, &email).await?.is_some() {
            return Err(conflict("Someone with that email already has an account"));
        }
        let recent: i64 =
            sqlx::query_scalar("select count(*) from email_change_tokens where user_id = $1 and created_at > now() - interval '1 hour'")
                .bind(user.id)
                .fetch_one(&mut *conn)
                .await?;
        if recent >= 5 {
            return Err(AppError::new(ErrorKind::TooManyRequests, "Too many email changes for this person. Try again in an hour."));
        }
        let mut tx = conn.begin().await?;
        sqlx::query("update email_change_tokens set used_at = now() where user_id = $1 and used_at is null")
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        let token = random_token();
        sqlx::query("insert into email_change_tokens (id, user_id, new_email, requested_by, expires_at) values ($1, $2, $3, $4, $5)")
            .bind(sha256_hex(&token))
            .bind(user.id)
            .bind(&email)
            .bind(actor.user_id)
            .bind(ctx.now + Duration::hours(EMAIL_CHANGE_HOURS))
            .execute(&mut *tx)
            .await?;
        audit(
            ctx,
            &mut tx,
            Entry::new("users.email_change_requested", "user", user.id)
                .before(json!({ "email": user.email }))
                .after(json!({ "email": email })),
        )
        .await?;
        tx.commit().await?;
        let email_settings: EmailSettings = settings(ctx, conn).await?;
        let org: OrgSettings = settings(ctx, conn).await?;
        email::send(
            &ctx.app,
            &email_settings,
            &org,
            &Message {
                to: email.clone(),
                to_name: Some(user.name.clone()),
                subject: "Confirm your new OpenSign email".into(),
                paragraphs: vec![
                    format!("Hi {},", user.name),
                    format!(
                        "Your OpenSign sign-in email is being changed to this address. Confirm it within {EMAIL_CHANGE_HOURS} hours to finish the change."
                    ),
                ],
                button: Some(("Confirm Email".into(), format!("{}/confirm-email#{token}", ctx.app.public_url()))),
                footer: Some("If you did not expect this, ignore this email; nothing changes.".into()),
                attachments: vec![],
                lang: crate::i18n::Lang::En,
            },
        )
        .await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
pub struct TokenInput {
    pub token: String,
}

#[derive(Serialize, JsonSchema)]
pub struct EmailView {
    pub email: String,
}

pub struct ConfirmEmailChange;

#[async_trait]
impl Service for ConfirmEmailChange {
    type Input = TokenInput;
    type Output = EmailView;
    const META: Meta = Meta::mutation(
        "auth.confirm_email_change",
        "Finish changing a sign-in email with the link sent to the new address.",
        Access::Public,
    )
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: TokenInput) -> Result<EmailView> {
        let mut tx = conn.begin().await?;
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "update email_change_tokens set used_at = now() where id = $1 and used_at is null and expires_at > $2 returning user_id, new_email",
        )
        .bind(sha256_hex(&input.token))
        .bind(ctx.now)
        .fetch_optional(&mut *tx)
        .await?;
        let expired = || bad_request("This link has expired or was already used. Ask for a new one.");
        let (user_id, email) = row.ok_or_else(expired)?;
        let user = user_by_id(&mut tx, user_id).await?.filter(|u| u.active).ok_or_else(expired)?;
        if user_by_email(&mut tx, &email).await?.is_some_and(|u| u.id != user.id) {
            return Err(conflict("Someone with that email already has an account"));
        }
        sqlx::query("update users set email = $2, updated_at = now() where id = $1").bind(user.id).bind(&email).execute(&mut *tx).await?;
        expire_account_links(&mut tx, user.id).await?;
        audit(
            ctx,
            &mut tx,
            Entry::new("users.email_changed", "user", user.id).before(json!({ "email": user.email })).after(json!({ "email": email })),
        )
        .await?;
        tx.commit().await?;
        send_in_background(
            ctx,
            conn,
            Message {
                to: user.email.clone(),
                to_name: Some(user.name.clone()),
                subject: "Your OpenSign email was changed".into(),
                paragraphs: vec![
                    format!("Hi {},", user.name),
                    format!("Your OpenSign sign-in email is now {email}. This address no longer signs in."),
                ],
                button: None,
                footer: Some("If you did not expect this, contact your OpenSign admin right away.".into()),
                attachments: vec![],
                lang: crate::i18n::Lang::En,
            },
        )
        .await?;
        Ok(EmailView { email })
    }
}

pub struct ResetTwoFactor;

#[async_trait]
impl Service for ResetTwoFactor {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "users.reset_two_factor",
        "Turn off someone's two-factor when they lost their phone. They set it up again at next sign-in if it is required.",
        Access::Permission(Permission::UsersManage),
    )
    .not_a_tool("Turning off someone's two-factor weakens their sign-in, so an admin does it in the app.")
    .without_transaction();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        if input.id == ctx.actor()?.user_id {
            return Err(bad_request("Ask another admin to reset your own two-factor"));
        }
        let mut tx = conn.begin().await?;
        let user = user_by_id(&mut tx, input.id).await?.ok_or_else(|| not_found("User"))?;
        sqlx::query(
            "update users set totp_secret = null, totp_pending_secret = null, totp_enabled_at = null, totp_last_step = null, has_passkey = false where id = $1",
        )
        .bind(input.id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("delete from recovery_codes where user_id = $1").bind(input.id).execute(&mut *tx).await?;
        let passkeys = sqlx::query("delete from passkeys where user_id = $1").bind(input.id).execute(&mut *tx).await?.rows_affected();
        revoke_sessions(&mut tx, input.id, None).await?;
        let keys = revoke_api_keys(&mut tx, input.id).await?;
        audit(
            ctx,
            &mut tx,
            Entry::new("users.reset_two_factor", "user", input.id).metadata(json!({ "passkeysRemoved": passkeys, "apiKeysRevoked": keys })),
        )
        .await?;
        tx.commit().await?;
        notify_keys_revoked(ctx, conn, &user, keys, "An admin reset your two-factor sign-in.").await?;
        Ok(Ok_::default())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiKeyInput {
    pub name: String,
    /// What the key may do. It never gets more than your own role allows, now or later.
    pub permissions: Vec<Permission>,
    pub expires_in_days: Option<u32>,
    /// Addresses or CIDR ranges the key may be used from. Empty allows any address.
    #[serde(default)]
    pub allowed_ips: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedApiKey {
    pub id: Uuid,
    /// Shown once. Only a hash is stored.
    pub key: String,
}

pub struct CreateApiKey;

#[async_trait]
impl Service for CreateApiKey {
    type Input = CreateApiKeyInput;
    type Output = CreatedApiKey;
    const META: Meta =
        Meta::mutation("api_keys.create", "Create an API key for integrations.", Access::Permission(Permission::ApiKeysManage))
            .not_a_tool("Returns a new credential, which must not end up in an agent's context.");

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: CreateApiKeyInput) -> Result<CreatedApiKey> {
        let actor = ctx.actor()?;
        if actor.credential != Credential::Session {
            return Err(AppError::new(ErrorKind::Forbidden, "Create API keys in the app, signed in"));
        }
        require_step_up(ctx, conn).await?;
        let name = key_name(&input.name)?;
        let permissions = key_permissions(actor, &input.permissions)?;
        let allowed_ips = normalize_allowed_ips(&input.allowed_ips)?;
        let prefix: String = hex::encode(opensign_crypto::random_bytes::<6>());
        let key = format!("{API_KEY_PREFIX}{prefix}_{}", random_token());
        let id = Uuid::now_v7();
        let expires_at = input.expires_in_days.map(|d| ctx.now + Duration::days(d.clamp(1, 3650) as i64));
        sqlx::query("insert into api_keys (id, name, prefix, key_hash, user_id, permissions, allowed_ips, expires_at) values ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(id)
            .bind(&name)
            .bind(&prefix)
            .bind(sha256_hex(&key))
            .bind(actor.user_id)
            .bind(&permissions)
            .bind(&allowed_ips)
            .bind(expires_at)
            .execute(&mut *conn)
            .await?;
        audit(
            ctx,
            conn,
            Entry::new("api_keys.create", "api_key", id)
                .after(serde_json::json!({ "name": name, "permissions": permissions, "allowedIps": allowed_ips, "expiresAt": expires_at })),
        )
        .await?;
        Ok(CreatedApiKey { id, key })
    }
}

fn key_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad_request("Name the key, e.g. after the system that uses it"));
    }
    Ok(name.to_string())
}

fn key_permissions(actor: &crate::context::Actor, permissions: &[Permission]) -> Result<Vec<String>> {
    if permissions.is_empty() {
        return Err(bad_request("Give the key at least one permission"));
    }
    if permissions.iter().any(|p| !actor.can(*p)) {
        return Err(forbidden());
    }
    let set: BTreeSet<Permission> = permissions.iter().copied().collect();
    Ok(set.into_iter().map(|p| p.as_str().to_string()).collect())
}

#[derive(Serialize, JsonSchema, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyView {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub permissions: Vec<String>,
    pub allowed_ips: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const API_KEY_COLUMNS: &str = "id, name, prefix, permissions, allowed_ips, expires_at, last_used_at, created_at";

pub struct ListApiKeys;

#[async_trait]
impl Service for ListApiKeys {
    type Input = Empty;
    type Output = Vec<ApiKeyView>;
    const META: Meta =
        Meta::query("api_keys.list", "List your API keys (never the keys themselves).", Access::Permission(Permission::ApiKeysManage))
            .tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, _: Empty) -> Result<Vec<ApiKeyView>> {
        Ok(sqlx::query_as(&format!(
            "select {API_KEY_COLUMNS} from api_keys where user_id = $1 and revoked_at is null order by created_at desc"
        ))
        .bind(ctx.actor()?.user_id)
        .fetch_all(&mut *conn)
        .await?)
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApiKeyInput {
    pub id: Uuid,
    /// A new name. Omit to keep the current one.
    pub name: Option<String>,
    /// Replaces the key's scopes. You can only grant what your own role allows. Omit to keep them.
    pub permissions: Option<Vec<Permission>>,
    /// Replaces the addresses or CIDR ranges the key may be used from; empty allows any address. Omit to keep them.
    pub allowed_ips: Option<Vec<String>>,
}

pub struct UpdateApiKey;

#[async_trait]
impl Service for UpdateApiKey {
    type Input = UpdateApiKeyInput;
    type Output = ApiKeyView;
    const META: Meta = Meta::mutation(
        "api_keys.update",
        "Rename one of your API keys or change its permissions or IP allowlist. The key itself stays the same and is never shown again. Through an API key or MCP you can only narrow a key's access; widening it needs the app, signed in.",
        Access::Permission(Permission::ApiKeysManage),
    ).tool();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: UpdateApiKeyInput) -> Result<ApiKeyView> {
        let actor = ctx.actor()?;
        let before: ApiKeyView = sqlx::query_as(&format!(
            "select {API_KEY_COLUMNS} from api_keys where id = $1 and user_id = $2 and revoked_at is null for update"
        ))
        .bind(input.id)
        .bind(actor.user_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| not_found("API key"))?;

        let name = match &input.name {
            Some(n) => key_name(n)?,
            None => before.name.clone(),
        };
        let permissions = match &input.permissions {
            Some(p) => key_permissions(actor, p)?,
            None => before.permissions.clone(),
        };
        let allowed_ips = match &input.allowed_ips {
            Some(ips) => normalize_allowed_ips(ips)?,
            None => before.allowed_ips.clone(),
        };
        let widens = permissions.iter().any(|p| !before.permissions.contains(p)) || !ips_no_wider(&before.allowed_ips, &allowed_ips);
        if widens && actor.credential != Credential::Session {
            return Err(AppError::new(ErrorKind::Forbidden, "Widen an API key's access in the app, signed in"));
        }
        let same = |a: &[String], b: &[String]| a.iter().collect::<BTreeSet<_>>() == b.iter().collect::<BTreeSet<_>>();
        if name == before.name && same(&permissions, &before.permissions) && same(&allowed_ips, &before.allowed_ips) {
            return Ok(before);
        }

        let after: ApiKeyView = sqlx::query_as(&format!(
            "update api_keys set name = $3, permissions = $4, allowed_ips = $5 where id = $1 and user_id = $2 returning {API_KEY_COLUMNS}"
        ))
        .bind(input.id)
        .bind(actor.user_id)
        .bind(&name)
        .bind(&permissions)
        .bind(&allowed_ips)
        .fetch_one(&mut *conn)
        .await?;
        audit(
            ctx,
            conn,
            Entry::new("api_keys.update", "api_key", input.id)
                .before(serde_json::json!({ "name": before.name, "permissions": before.permissions, "allowedIps": before.allowed_ips }))
                .after(serde_json::json!({ "name": after.name, "permissions": after.permissions, "allowedIps": after.allowed_ips })),
        )
        .await?;
        Ok(after)
    }
}

pub struct RevokeApiKey;

#[async_trait]
impl Service for RevokeApiKey {
    type Input = IdInput;
    type Output = Ok_;
    const META: Meta = Meta::mutation(
        "api_keys.revoke",
        "Revoke one of your API keys; it stops working at once.",
        Access::Permission(Permission::ApiKeysManage),
    )
    .tool()
    .destructive();

    async fn run(ctx: &Ctx, conn: &mut PgConnection, input: IdInput) -> Result<Ok_> {
        let done = sqlx::query("update api_keys set revoked_at = now() where id = $1 and user_id = $2 and revoked_at is null")
            .bind(input.id)
            .bind(ctx.actor()?.user_id)
            .execute(&mut *conn)
            .await?;
        if done.rows_affected() == 0 {
            return Err(not_found("API key"));
        }
        audit(ctx, conn, Entry::new("api_keys.revoke", "api_key", input.id)).await?;
        Ok(Ok_::default())
    }
}
