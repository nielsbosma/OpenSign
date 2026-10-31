//! OAuth 2.1 for the remote MCP server: dynamic client registration, authorization code with PKCE
//! (S256 only), rotating refresh tokens, and audience-bound access tokens. Clients are public; the
//! PKCE verifier proves possession instead of a client secret. Tokens are stored as SHA-256 hashes.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use opensign_crypto::{random_token, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::{actor_for, user_by_id};
use crate::context::{Actor, App, Credential};
use crate::error::{AppError, ErrorKind, Result, bad_request};
use crate::settings::SecuritySettings;

pub const SCOPE: &str = "opensign";
const CODE_MINUTES: i64 = 5;
const ACCESS_SECONDS: i64 = 3600;
const REFRESH_DAYS: i64 = 30;
const CONSENT_SECONDS: i64 = 600;
/// Clients register themselves without signing in, so the table needs a ceiling. Unused ones are
/// swept after a week.
const MAX_CLIENTS: i64 = 100_000;

pub fn mcp_url(app: &App) -> String {
    format!("{}/mcp", app.public_url())
}

/// OAuth errors carry an RFC 6749 code for the token endpoint to return.
pub fn oauth_error(code: &'static str, description: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::BadRequest, description).with_code(code)
}

/// Redirect URIs must be https, or http on the loopback interface for desktop and CLI clients.
fn check_redirect(uri: &str) -> Result<()> {
    let url = url::Url::parse(uri).map_err(|_| oauth_error("invalid_redirect_uri", "A redirect URI is not a URL"))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let ok = url.fragment().is_none() && url.username().is_empty() && (url.scheme() == "https" || url.scheme() == "http" && loopback);
    if !ok || uri.len() > 500 {
        return Err(oauth_error("invalid_redirect_uri", "Redirect URIs must be https, or http on localhost"));
    }
    Ok(())
}

/// `example.co.uk` for `a.b.example.co.uk`. A host with no public suffix, like an address or
/// `localhost`, is shown as it is.
pub fn registrable_domain(host: &str) -> String {
    psl::domain_str(host).unwrap_or(host).to_string()
}

/// RFC 7591 registration. Only public clients with the authorization code grant.
pub async fn register_client(conn: &mut PgConnection, request: &Value) -> Result<Value> {
    let clients: i64 = sqlx::query_scalar("select count(*) from oauth_clients").fetch_one(&mut *conn).await?;
    if clients >= MAX_CLIENTS {
        return Err(oauth_error("temporarily_unavailable", "Too many clients are registered. Try again later."));
    }
    let uris: Vec<String> =
        request["redirect_uris"].as_array().into_iter().flatten().filter_map(|u| u.as_str().map(str::to_string)).collect();
    if uris.is_empty() || uris.len() > 10 {
        return Err(oauth_error("invalid_redirect_uri", "Register between one and ten redirect URIs"));
    }
    for uri in &uris {
        check_redirect(uri)?;
    }
    if let Some(method) = request["token_endpoint_auth_method"].as_str()
        && method != "none"
    {
        return Err(oauth_error("invalid_client_metadata", "Only public clients (token_endpoint_auth_method none) are supported"));
    }
    let name: String = request["client_name"].as_str().unwrap_or("MCP client").chars().filter(|c| !c.is_control()).take(120).collect();
    let client_uri = request["client_uri"].as_str().filter(|u| u.starts_with("https://") && u.len() <= 300);
    let client_id = Uuid::new_v4().to_string();
    let info = json!({
        "client_id": client_id,
        "client_id_issued_at": Utc::now().timestamp(),
        "client_name": name,
        "client_uri": client_uri,
        "redirect_uris": uris,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": "none",
        "scope": SCOPE,
    });
    sqlx::query("insert into oauth_clients (client_id, name, redirect_uris, metadata) values ($1, $2, $3, $4)")
        .bind(&client_id)
        .bind(&name)
        .bind(&uris)
        .bind(&info)
        .execute(&mut *conn)
        .await?;
    Ok(info)
}

#[derive(sqlx::FromRow)]
pub struct Client {
    pub client_id: String,
    pub name: String,
    pub redirect_uris: Vec<String>,
    pub metadata: Value,
}

pub async fn client(conn: &mut PgConnection, client_id: &str) -> Result<Option<Client>> {
    Ok(sqlx::query_as("select client_id, name, redirect_uris, metadata from oauth_clients where client_id = $1")
        .bind(client_id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// What the consent page shows and approves. Signed and short-lived, so abandoned requests leave
/// nothing in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pending {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: Option<String>,
    pub resource: Option<String>,
}

pub struct AuthorizeRequest<'a> {
    pub response_type: Option<&'a str>,
    pub client_id: Option<&'a str>,
    pub redirect_uri: Option<&'a str>,
    pub code_challenge: Option<&'a str>,
    pub code_challenge_method: Option<&'a str>,
    pub state: Option<&'a str>,
    pub resource: Option<&'a str>,
    pub scope: Option<&'a str>,
}

pub enum Authorize {
    /// Send the browser to the consent page.
    Consent(String),
    /// Report an error to the client at its redirect URI.
    Redirect(String),
}

/// Validates an authorization request. Errors go back to the browser as `Err`, or to the redirect
/// URI of a client someone has already approved.
pub async fn authorize(app: &App, conn: &mut PgConnection, r: AuthorizeRequest<'_>) -> Result<Authorize> {
    let client_id = r.client_id.ok_or_else(|| bad_request("client_id is missing"))?;
    let client = client(conn, client_id).await?.ok_or_else(|| bad_request("This MCP client is not registered. Connect it again."))?;
    let redirect_uri = match r.redirect_uri {
        Some(uri) if client.redirect_uris.iter().any(|u| u == uri) => uri.to_string(),
        None if client.redirect_uris.len() == 1 => client.redirect_uris[0].clone(),
        _ => return Err(bad_request("The redirect URI is not registered for this client")),
    };
    // Anyone can register a client, so sending the browser to its address on a bad request, before
    // anyone has trusted it, would make this origin an open redirect. Such errors show here instead.
    let trusted: bool = sqlx::query_scalar("select exists (select 1 from oauth_grants where client_id = $1)")
        .bind(&client.client_id)
        .fetch_one(&mut *conn)
        .await?;
    let fail = |code: &str, description: &str| {
        if !trusted {
            return Err(oauth_error("invalid_request", format!("{description} ({code})")));
        }
        let mut url = url::Url::parse(&redirect_uri).map_err(|_| bad_request("Invalid redirect URI"))?;
        url.query_pairs_mut().append_pair("error", code).append_pair("error_description", description);
        if let Some(state) = r.state {
            url.query_pairs_mut().append_pair("state", state);
        }
        Ok(Authorize::Redirect(url.to_string()))
    };
    if r.response_type != Some("code") {
        return fail("unsupported_response_type", "Only the authorization code flow is supported");
    }
    let Some(challenge) = r.code_challenge.filter(|c| (43..=128).contains(&c.len())) else {
        return fail("invalid_request", "PKCE is required");
    };
    if r.code_challenge_method != Some("S256") {
        return fail("invalid_request", "Only the S256 PKCE method is supported");
    }
    if r.scope.is_some_and(|s| s.split_whitespace().any(|s| s != SCOPE)) {
        return fail("invalid_scope", "The only scope is opensign");
    }
    if r.resource.is_some_and(|res| res.trim_end_matches('/') != mcp_url(app)) {
        return fail("invalid_target", "This server only issues tokens for its own MCP endpoint");
    }
    let pending = Pending {
        client_id: client.client_id,
        redirect_uri,
        code_challenge: challenge.to_string(),
        state: r.state.map(|s| s.chars().take(500).collect()),
        resource: r.resource.map(str::to_string),
    };
    let token = app.keys.sign_token("oauth-consent", &pending, CONSENT_SECONDS, Utc::now().timestamp());
    Ok(Authorize::Consent(format!("{}/oauth/consent?request={}", app.public_url(), urlencode(&token))))
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub fn decode_pending(app: &App, request: &str, now: DateTime<Utc>) -> Result<Pending> {
    app.keys
        .verify_token("oauth-consent", request, now.timestamp())
        .map_err(|_| bad_request("This connection request has expired. Start connecting again from your MCP client."))
}

pub fn redirect_with(pending: &Pending, params: &[(&str, &str)]) -> Result<String> {
    let mut url = url::Url::parse(&pending.redirect_uri).map_err(|_| bad_request("Invalid redirect URI"))?;
    for (k, v) in params {
        url.query_pairs_mut().append_pair(k, v);
    }
    if let Some(state) = &pending.state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Ok(url.to_string())
}

/// Issues a one-time code for the signed-in user who approved the request.
pub async fn issue_code(conn: &mut PgConnection, pending: &Pending, user_id: Uuid, now: DateTime<Utc>) -> Result<String> {
    let client = client(conn, &pending.client_id).await?.ok_or_else(|| bad_request("Unknown client"))?;
    if !client.redirect_uris.contains(&pending.redirect_uri) {
        return Err(bad_request("Unknown redirect URI"));
    }
    let code = random_token();
    sqlx::query(
        "insert into oauth_codes (id, client_id, user_id, code_challenge, redirect_uri, scopes, resource, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(sha256_hex(&code))
    .bind(&pending.client_id)
    .bind(user_id)
    .bind(&pending.code_challenge)
    .bind(&pending.redirect_uri)
    .bind(vec![SCOPE.to_string()])
    .bind(&pending.resource)
    .bind(now + Duration::minutes(CODE_MINUTES))
    .execute(&mut *conn)
    .await?;
    Ok(code)
}

#[derive(Serialize)]
pub struct Tokens {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: &'static str,
}

fn tokens() -> (String, String) {
    (format!("osa_{}", random_token()), format!("osr_{}", random_token()))
}

fn pkce_matches(verifier: &str, challenge: &str) -> bool {
    let valid = (43..=128).contains(&verifier.len()) && verifier.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b));
    valid && opensign_crypto::constant_time_eq(URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())).as_bytes(), challenge.as_bytes())
}

#[derive(sqlx::FromRow)]
struct CodeRow {
    client_id: String,
    user_id: Uuid,
    code_challenge: String,
    redirect_uri: String,
    scopes: Vec<String>,
    resource: Option<String>,
    expires_at: DateTime<Utc>,
}

/// The code is deleted as it is read, so it works once even under concurrent requests.
pub async fn exchange_code(
    conn: &mut PgConnection,
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Tokens> {
    let invalid = || oauth_error("invalid_grant", "The authorization code is invalid or expired");
    let row: CodeRow = sqlx::query_as(
        "delete from oauth_codes where id = $1 returning client_id, user_id, code_challenge, redirect_uri, scopes, resource, expires_at",
    )
    .bind(sha256_hex(code))
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(invalid)?;
    if row.client_id != client_id || row.expires_at <= now || redirect_uri.is_some_and(|r| r != row.redirect_uri) {
        return Err(invalid());
    }
    if !pkce_matches(verifier, &row.code_challenge) {
        return Err(oauth_error("invalid_grant", "The PKCE verifier does not match"));
    }
    let (access, refresh) = tokens();
    sqlx::query(
        "insert into oauth_grants (id, client_id, user_id, scopes, resource, access_token_hash, refresh_token_hash, access_expires_at, refresh_expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(Uuid::now_v7())
    .bind(client_id)
    .bind(row.user_id)
    .bind(&row.scopes)
    .bind(&row.resource)
    .bind(sha256_hex(&access))
    .bind(sha256_hex(&refresh))
    .bind(now + Duration::seconds(ACCESS_SECONDS))
    .bind(now + Duration::days(REFRESH_DAYS))
    .execute(&mut *conn)
    .await?;
    Ok(Tokens { access_token: access, token_type: "Bearer", expires_in: ACCESS_SECONDS, refresh_token: refresh, scope: SCOPE })
}

#[derive(sqlx::FromRow)]
struct GrantRow {
    id: Uuid,
    client_id: String,
    user_id: Uuid,
    resource: Option<String>,
    refresh_expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
}

/// Refresh tokens rotate on every use. A refresh token presented again after rotation means it
/// leaked, so the whole grant is revoked.
pub async fn refresh(conn: &mut PgConnection, client_id: &str, refresh_token: &str, now: DateTime<Utc>) -> Result<Tokens> {
    let invalid = || oauth_error("invalid_grant", "The refresh token is invalid or expired");
    let hash = sha256_hex(refresh_token);
    let row: Option<GrantRow> = sqlx::query_as(
        "select id, client_id, user_id, resource, refresh_expires_at, revoked_at, last_used_at from oauth_grants where refresh_token_hash = $1 for update",
    )
    .bind(&hash)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(GrantRow { id, client_id: owner, user_id, refresh_expires_at: expires, revoked_at: revoked, .. }) = row else {
        sqlx::query("update oauth_grants set revoked_at = coalesce(revoked_at, $2) where $1 = any(previous_refresh_hashes)")
            .bind(&hash)
            .bind(now)
            .execute(&mut *conn)
            .await?;
        return Err(invalid());
    };
    if owner != client_id || revoked.is_some() || expires <= now {
        return Err(invalid());
    }
    let Some(user) = user_by_id(conn, user_id).await? else { return Err(invalid()) };
    if !user.active {
        return Err(invalid());
    }
    let (access, refresh) = tokens();
    sqlx::query(
        "update oauth_grants set access_token_hash = $2, refresh_token_hash = $3, access_expires_at = $4, refresh_expires_at = $5,
             previous_refresh_hashes = (array_append(previous_refresh_hashes, refresh_token_hash))[greatest(1, cardinality(previous_refresh_hashes) - 18):]
         where id = $1",
    )
    .bind(id)
    .bind(sha256_hex(&access))
    .bind(sha256_hex(&refresh))
    .bind(now + Duration::seconds(ACCESS_SECONDS))
    .bind(now + Duration::days(REFRESH_DAYS))
    .execute(&mut *conn)
    .await?;
    Ok(Tokens { access_token: access, token_type: "Bearer", expires_in: ACCESS_SECONDS, refresh_token: refresh, scope: SCOPE })
}

pub async fn revoke(conn: &mut PgConnection, client_id: Option<&str>, token: &str, now: DateTime<Utc>) -> Result<()> {
    let hash = sha256_hex(token);
    sqlx::query(
        "update oauth_grants set revoked_at = $3 where revoked_at is null and ($1::text is null or client_id = $1)
           and (access_token_hash = $2 or refresh_token_hash = $2)",
    )
    .bind(client_id)
    .bind(hash)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The user behind an MCP access token, with their role's full permissions. Tokens are bound to
/// this server's MCP endpoint and stop working when the user is deactivated or must set up 2FA.
pub async fn load_access_token(
    conn: &mut PgConnection,
    app: &App,
    token: &str,
    security: &SecuritySettings,
    now: DateTime<Utc>,
) -> Result<Option<Actor>> {
    if !token.starts_with("osa_") {
        return Ok(None);
    }
    let row: Option<GrantRow> = sqlx::query_as(
        "select id, client_id, user_id, resource, refresh_expires_at, revoked_at, last_used_at from oauth_grants
         where access_token_hash = $1 and revoked_at is null and access_expires_at > $2",
    )
    .bind(sha256_hex(token))
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(GrantRow { id, user_id, resource, last_used_at: last_used, .. }) = row else { return Ok(None) };
    if resource.is_some_and(|r| r.trim_end_matches('/') != mcp_url(app)) {
        return Ok(None);
    }
    let Some(user) = user_by_id(conn, user_id).await?.filter(|u| !u.sso) else { return Ok(None) };
    let Some(actor) = actor_for(&user, security, Credential::OAuth) else { return Ok(None) };
    if last_used.is_none_or(|t| now - t > Duration::seconds(60)) {
        sqlx::query("update oauth_grants set last_used_at = $2 where id = $1").bind(id).bind(now).execute(&mut *conn).await?;
    }
    Ok(Some(actor))
}
