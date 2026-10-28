use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{constant_time_eq, random_token, sha256_hex};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::context::{Actor, Credential};
use crate::error::{AppError, ErrorKind, Result};
use crate::permissions::{Permission, Role};
use crate::settings::SecuritySettings;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: Option<String>,
    pub role: String,
    pub active: bool,
    pub totp_secret: Option<String>,
    pub totp_pending_secret: Option<String>,
    pub totp_enabled_at: Option<DateTime<Utc>>,
    pub totp_last_step: Option<i64>,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub has_passkey: bool,
}

pub async fn user_by_id(conn: &mut PgConnection, id: Uuid) -> Result<Option<UserRow>> {
    Ok(sqlx::query_as("select * from users where id = $1").bind(id).fetch_optional(&mut *conn).await?)
}

pub async fn user_by_email(conn: &mut PgConnection, email: &str) -> Result<Option<UserRow>> {
    Ok(sqlx::query_as("select * from users where lower(email) = lower($1)").bind(email.trim()).fetch_optional(&mut *conn).await?)
}

pub fn normalize_email(email: &str) -> Result<String> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !email.contains(char::is_whitespace)
        })
        && !email.chars().any(|c| c.is_control() || matches!(c, '<' | '>' | ',' | ';' | '"'));
    if !valid {
        return Err(AppError::new(ErrorKind::BadRequest, "Enter a valid email address"));
    }
    Ok(email)
}

pub fn actor_for(user: &UserRow, security: &SecuritySettings, credential: Credential) -> Option<Actor> {
    if !user.active {
        return None;
    }
    let role = Role::parse(&user.role)?;
    Some(Actor {
        user_id: user.id,
        email: user.email.clone(),
        name: user.name.clone(),
        role,
        permissions: role.permissions(),
        must_enable_two_factor: security.enforce_two_factor && user.totp_enabled_at.is_none() && !user.has_passkey,
        credential,
    })
}

pub struct NewSession {
    pub token: String,
    pub id: String,
    pub expires_at: DateTime<Utc>,
}

/// The cookie carries the token; only its SHA-256 is stored, so a database read cannot be replayed.
pub async fn create_session(
    conn: &mut PgConnection,
    user_id: Uuid,
    security: &SecuritySettings,
    ip: Option<&str>,
    user_agent: Option<&str>,
    now: DateTime<Utc>,
) -> Result<NewSession> {
    let token = random_token();
    let id = sha256_hex(&token);
    let expires_at = now + Duration::hours(security.session_hours.clamp(1, 24 * 30) as i64);
    sqlx::query(
        "insert into sessions (id, user_id, expires_at, last_seen_at, ip, user_agent, elevated_until) values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(expires_at)
    .bind(now)
    .bind(ip)
    .bind(user_agent.map(|ua| ua.chars().take(400).collect::<String>()))
    .bind(now + Duration::minutes(10))
    .execute(&mut *conn)
    .await?;
    Ok(NewSession { token, id, expires_at })
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
}

/// Resolves a session cookie. Expired (absolute or idle) sessions are deleted on sight.
pub async fn load_session(
    conn: &mut PgConnection,
    token: &str,
    security: &SecuritySettings,
    now: DateTime<Utc>,
) -> Result<Option<(Actor, String)>> {
    if token.len() != 43 {
        return Ok(None);
    }
    let id = sha256_hex(token);
    let Some(session) = sqlx::query_as::<_, SessionRow>("select id, user_id, expires_at, last_seen_at from sessions where id = $1")
        .bind(&id)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let idle = Duration::minutes(security.idle_minutes.clamp(5, 24 * 60) as i64);
    if session.expires_at <= now || session.last_seen_at + idle <= now {
        sqlx::query("delete from sessions where id = $1").bind(&session.id).execute(&mut *conn).await?;
        return Ok(None);
    }
    let Some(user) = user_by_id(conn, session.user_id).await? else { return Ok(None) };
    let Some(actor) = actor_for(&user, security, Credential::Session) else { return Ok(None) };
    if now - session.last_seen_at > Duration::seconds(60) {
        sqlx::query("update sessions set last_seen_at = $2 where id = $1").bind(&session.id).bind(now).execute(&mut *conn).await?;
    }
    Ok(Some((actor, session.id)))
}

pub async fn revoke_sessions(conn: &mut PgConnection, user_id: Uuid, except: Option<&str>) -> Result<u64> {
    let result = sqlx::query("delete from sessions where user_id = $1 and ($2::text is null or id <> $2)")
        .bind(user_id)
        .bind(except)
        .execute(&mut *conn)
        .await?;
    sqlx::query("update oauth_grants set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

pub const API_KEY_PREFIX: &str = "osk_";

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    user_id: Uuid,
    key_hash: String,
    permissions: Vec<String>,
    allowed_ips: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
}

/// `osk_<prefix>_<secret>`. The prefix finds the row; the whole key is compared by hash. The
/// actor gets the intersection of the key's scopes and the owner's current role.
pub async fn load_api_key(
    conn: &mut PgConnection,
    key: &str,
    ip: Option<&str>,
    security: &SecuritySettings,
    now: DateTime<Utc>,
) -> Result<Option<Actor>> {
    let Some(rest) = key.strip_prefix(API_KEY_PREFIX) else { return Ok(None) };
    let Some((prefix, _)) = rest.split_once('_') else { return Ok(None) };
    let Some(row) = sqlx::query_as::<_, ApiKeyRow>(
        "select id, user_id, key_hash, permissions, allowed_ips, expires_at, revoked_at, last_used_at from api_keys where prefix = $1",
    )
    .bind(prefix)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    if !constant_time_eq(sha256_hex(key).as_bytes(), row.key_hash.as_bytes())
        || row.revoked_at.is_some()
        || row.expires_at.is_some_and(|e| e <= now)
    {
        return Ok(None);
    }
    if !row.allowed_ips.is_empty() && !ip.is_some_and(|ip| row.allowed_ips.iter().any(|a| a == ip)) {
        return Ok(None);
    }
    let Some(user) = user_by_id(conn, row.user_id).await? else { return Ok(None) };
    let Some(mut actor) = actor_for(&user, security, Credential::ApiKey(row.id)) else { return Ok(None) };
    let scopes: std::collections::BTreeSet<Permission> = row.permissions.iter().filter_map(|p| Permission::parse(p)).collect();
    actor.permissions = actor.permissions.intersection(&scopes).copied().collect();
    actor.must_enable_two_factor = false;
    if row.last_used_at.is_none_or(|t| now - t > Duration::seconds(60)) {
        sqlx::query("update api_keys set last_used_at = $2 where id = $1").bind(row.id).bind(now).execute(&mut *conn).await?;
    }
    Ok(Some(actor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_emails() {
        assert_eq!(normalize_email(" Anna@Example.COM ").ok().as_deref(), Some("anna@example.com"));
        for bad in ["", "anna", "anna@", "@example.com", "anna@example", "a b@example.com", "a@example.com,b@example.com", "\"a\"@x.se"] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
    }
}
