use std::net::IpAddr;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use ipnet::IpNet;
use opensign_crypto::{burn_password_check, constant_time_eq, hash_password, random_token, sha256_hex, verify_password};
use sqlx::PgConnection;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::context::{Actor, Credential, Ctx};
use crate::error::{AppError, ErrorKind, Result, bad_request};
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
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub language: Option<String>,
    pub has_passkey: bool,
    /// Signs in only through the identity provider: no password, passkey, API key or MCP.
    pub sso: bool,
}

impl UserRow {
    /// For anything an account of this kind does not have. Refuse before a password check, which
    /// would count as a failed sign-in and could lock the account.
    pub fn refuse_sso(&self) -> Result<()> {
        if self.sso {
            return Err(AppError::new(
                ErrorKind::Forbidden,
                "This account signs in through your organisation's single sign-on, so this is not available.",
            )
            .with_code("sso_account"));
        }
        Ok(())
    }
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

/// Argon2 takes 64 MiB and a core for a moment. A few at a time on blocking threads keeps a flood of
/// sign-ins from using up the memory or stalling everything else on the async workers.
static PASSWORD_WORK: Semaphore = Semaphore::const_new(4);
const PASSWORD_QUEUE: StdDuration = StdDuration::from_secs(10);
/// Each one waiting holds a database connection, so a flood must not be able to take the whole pool.
const PASSWORD_WAITING: usize = 8;
static WAITING: AtomicUsize = AtomicUsize::new(0);

struct InQueue<'a>(&'a AtomicUsize);

impl Drop for InQueue<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn join_queue(waiting: &AtomicUsize, limit: usize) -> Option<InQueue<'_>> {
    let place = InQueue(waiting);
    (waiting.fetch_add(1, Ordering::SeqCst) < limit).then_some(place)
}

async fn password_work<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    let busy = || AppError::new(ErrorKind::Unavailable, "Too many sign-ins right now. Try again in a moment.");
    let in_queue = join_queue(&WAITING, PASSWORD_WAITING).ok_or_else(busy)?;
    let permit = tokio::time::timeout(PASSWORD_QUEUE, PASSWORD_WORK.acquire()).await.map_err(|_| busy())?.map_err(|_| busy())?;
    drop(in_queue);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(AppError::internal)
}

pub async fn check_password(hash: &str, password: &str) -> Result<bool> {
    let (hash, password) = (hash.to_string(), password.to_string());
    password_work(move || verify_password(&hash, &password)).await
}

/// Takes as long as checking a real password, so an unknown account does not answer sooner.
pub async fn burn_password(password: &str) -> Result<()> {
    let password = password.to_string();
    password_work(move || burn_password_check(&password)).await
}

pub async fn new_password_hash(password: &str) -> Result<String> {
    let password = password.to_string();
    password_work(move || hash_password(&password)).await?.map_err(|e| bad_request(e.to_string()))
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
        // For a single sign-on account this depends on the session; see `load_session`.
        must_enable_two_factor: security.enforce_two_factor && !user.sso && user.totp_enabled_at.is_none() && !user.has_passkey,
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
    .bind(now + Duration::minutes(STEP_UP_MINUTES))
    .execute(&mut *conn)
    .await?;
    Ok(NewSession { token, id, expires_at })
}

/// How long a confirmation of who you are lasts, and how long signing in counts as one.
pub const STEP_UP_MINUTES: i64 = 10;

/// For the few calls that hand out lasting access: a stolen cookie alone is not enough. The web app
/// sees the code, asks for the password or a passkey through `account.step_up`, and tries again.
pub async fn require_step_up(ctx: &Ctx, conn: &mut PgConnection) -> Result<()> {
    let needed = || AppError::new(ErrorKind::Forbidden, "Confirm it is you to continue").with_code("step_up_required");
    let Some(session) = &ctx.session_id else { return Err(needed()) };
    let until: Option<Option<DateTime<Utc>>> =
        sqlx::query_scalar("select elevated_until from sessions where id = $1").bind(session).fetch_optional(&mut *conn).await?;
    if until.flatten().is_some_and(|t| t > ctx.now) { Ok(()) } else { Err(needed()) }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    sso_mfa: Option<bool>,
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
    let Some(session) =
        sqlx::query_as::<_, SessionRow>("select id, user_id, expires_at, last_seen_at, sso_mfa from sessions where id = $1")
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
    let Some(mut actor) = actor_for(&user, security, Credential::Session) else { return Ok(None) };
    // Only the provider can prove a second factor here, and only at sign-in.
    actor.must_enable_two_factor |= user.sso && security.enforce_two_factor && session.sso_mfa != Some(true);
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

pub async fn revoke_api_keys(conn: &mut PgConnection, user_id: Uuid) -> Result<u64> {
    let result = sqlx::query("update api_keys set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected())
}

/// Links sent to a mailbox or for a password that no longer applies must not outlive the change.
pub async fn expire_account_links(conn: &mut PgConnection, user_id: Uuid) -> Result<()> {
    sqlx::query("update password_reset_tokens set used_at = now() where user_id = $1 and used_at is null")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("update email_change_tokens set used_at = now() where user_id = $1 and used_at is null")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
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
    if !ip_allowed(&row.allowed_ips, ip) {
        return Ok(None);
    }
    let Some(user) = user_by_id(conn, row.user_id).await?.filter(|u| !u.sso) else { return Ok(None) };
    let Some(mut actor) = actor_for(&user, security, Credential::ApiKey(row.id)) else { return Ok(None) };
    let scopes: std::collections::BTreeSet<Permission> = row.permissions.iter().filter_map(|p| Permission::parse(p)).collect();
    actor.permissions = actor.permissions.intersection(&scopes).copied().collect();
    actor.must_enable_two_factor = false;
    if row.last_used_at.is_none_or(|t| now - t > Duration::seconds(60)) {
        sqlx::query("update api_keys set last_used_at = $2 where id = $1").bind(row.id).bind(now).execute(&mut *conn).await?;
    }
    Ok(Some(actor))
}

pub const MAX_ALLOWED_IPS: usize = 50;

pub(crate) fn parse_net(entry: &str) -> Option<IpNet> {
    entry.parse::<IpNet>().ok().or_else(|| entry.parse::<IpAddr>().ok().map(IpNet::from))
}

/// Validates an allowlist of addresses and CIDR ranges and returns it in canonical form.
pub fn normalize_allowed_ips(entries: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for raw in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let net =
            parse_net(raw).ok_or_else(|| bad_request("{value} is not an IP address or CIDR range").params([("value", &raw)]))?.trunc();
        let canonical = if net.prefix_len() == net.max_prefix_len() { net.addr().to_string() } else { net.to_string() };
        if !out.contains(&canonical) {
            out.push(canonical);
        }
    }
    if out.len() > MAX_ALLOWED_IPS {
        return Err(bad_request("Allow at most {max} addresses or ranges").params([("max", &MAX_ALLOWED_IPS)]));
    }
    Ok(out)
}

pub fn ip_allowed(allowed: &[String], ip: Option<&str>) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let Some(ip) = ip.and_then(|ip| ip.parse::<IpAddr>().ok()).map(|ip| ip.to_canonical()) else { return false };
    allowed.iter().filter_map(|a| parse_net(a)).any(|net| net.contains(&ip))
}

/// True when every address `new` admits is already admitted by `old`.
pub fn ips_no_wider(old: &[String], new: &[String]) -> bool {
    if old.is_empty() {
        return true;
    }
    if new.is_empty() {
        return false;
    }
    let old: Vec<IpNet> = old.iter().filter_map(|a| parse_net(a)).collect();
    new.iter().all(|n| parse_net(n).is_some_and(|n| old.iter().any(|o| o.contains(&n))))
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

    #[test]
    fn matches_allowed_ips() {
        let allowed = normalize_allowed_ips(&[" 203.0.113.7 ".into(), "198.51.100.9/24".into(), "2001:db8::/32".into()]).expect("valid");
        assert_eq!(allowed, ["203.0.113.7", "198.51.100.0/24", "2001:db8::/32"]);
        assert!(ip_allowed(&allowed, Some("203.0.113.7")));
        assert!(ip_allowed(&allowed, Some("198.51.100.200")));
        assert!(ip_allowed(&allowed, Some("::ffff:198.51.100.1")));
        assert!(ip_allowed(&allowed, Some("2001:db8:1::1")));
        assert!(!ip_allowed(&allowed, Some("203.0.113.8")));
        assert!(!ip_allowed(&allowed, None));
        assert!(!ip_allowed(&allowed, Some("not an ip")));
        assert!(ip_allowed(&[], None));
        assert!(normalize_allowed_ips(&["10.0.0.0/33".into()]).is_err());
        assert!(normalize_allowed_ips(&["example.com".into()]).is_err());
    }

    #[test]
    fn compares_allowlists() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(ips_no_wider(&s(&[]), &s(&["10.0.0.1"])));
        assert!(!ips_no_wider(&s(&["10.0.0.1"]), &s(&[])));
        assert!(ips_no_wider(&s(&["10.0.0.0/8"]), &s(&["10.1.0.0/16", "10.0.0.5"])));
        assert!(!ips_no_wider(&s(&["10.1.0.0/16"]), &s(&["10.0.0.0/8"])));
        assert!(!ips_no_wider(&s(&["10.0.0.1"]), &s(&["10.0.0.1", "10.0.0.2"])));
    }
}

#[cfg(test)]
mod queue_tests {
    use super::*;

    #[test]
    fn the_password_queue_is_bounded_and_gives_places_back() {
        let waiting = AtomicUsize::new(0);
        let first = join_queue(&waiting, 1);
        assert!(first.is_some());
        assert!(join_queue(&waiting, 1).is_none());
        drop(first);
        assert!(join_queue(&waiting, 1).is_some());
    }
}
