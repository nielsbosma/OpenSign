use axum::http::{HeaderMap, HeaderValue, header};
use chrono::{DateTime, Utc};
use opensign_core::auth::{load_api_key, load_session};
use opensign_core::context::{Ctx, Surface};
use opensign_core::error::{AppError, ErrorKind, Result};
use opensign_core::settings::SecuritySettings;

use crate::AppState;

pub fn cookie_name(state: &AppState) -> &'static str {
    // The __Host- prefix makes browsers refuse the cookie unless it is Secure, host-only and Path=/.
    if state.https { "__Host-opensign_session" } else { "opensign_session" }
}

pub fn session_cookie(headers: &HeaderMap, state: &AppState) -> Option<String> {
    let name = cookie_name(state);
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name && !value.is_empty()).then(|| value.to_string())
    })
}

pub fn set_cookie(state: &AppState, token: &str, expires_at: DateTime<Utc>) -> HeaderValue {
    let max_age = (expires_at - Utc::now()).num_seconds().max(0);
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}", cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_cookie(state: &AppState) -> HeaderValue {
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}", cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn eid_cookie_name(state: &AppState) -> &'static str {
    if state.https { "__Host-opensign_eid" } else { "opensign_eid" }
}

/// Ties an eID broker flow to the browser it started in. It outlives the flow by nothing: the
/// session it belongs to lasts as long.
pub fn set_eid_cookie(state: &AppState, value: &str) -> HeaderValue {
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600{secure}", eid_cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_eid_cookie(state: &AppState) -> HeaderValue {
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}", eid_cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn eid_cookie(headers: &HeaderMap, state: &AppState) -> Option<String> {
    let name = eid_cookie_name(state);
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name && !value.is_empty()).then(|| value.to_string())
    })
}

fn sso_cookie_name(state: &AppState) -> &'static str {
    if state.https { "__Host-opensign_sso" } else { "opensign_sso" }
}

/// Ties a single sign-on to the browser it started in, so nobody can sign a visitor in to their own account.
pub fn set_sso_cookie(state: &AppState, value: &str) -> HeaderValue {
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600{secure}", sso_cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn clear_sso_cookie(state: &AppState) -> HeaderValue {
    let secure = if state.https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}", sso_cookie_name(state)))
        .unwrap_or_else(|_| HeaderValue::from_static(""))
}

pub fn sso_cookie(headers: &HeaderMap, state: &AppState) -> Option<String> {
    let name = sso_cookie_name(state);
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name && !value.is_empty()).then(|| value.to_string())
    })
}

/// The service context for a request: an API key in `Authorization: Bearer`, or the session cookie.
/// A bearer token that does not check out is refused outright rather than ignored.
pub async fn context(state: &AppState, headers: &HeaderMap, ip: Option<String>, surface: Surface, request_id: String) -> Result<Ctx> {
    let mut ctx = Ctx::new(state.app.clone(), surface);
    ctx.ip = ip;
    ctx.user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(|s| s.chars().take(300).collect());
    ctx.request_id = Some(request_id);
    if surface == Surface::Signer {
        // A signer is the holder of the signing link, never whoever happens to be signed in to the app in the same browser.
        return Ok(ctx);
    }
    let bearer = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::trim);
    if bearer.is_none() && session_cookie(headers, state).is_none() {
        return Ok(ctx);
    }
    let mut conn = state.app.db.acquire().await?;
    let security: SecuritySettings = state.app.settings.get(&mut conn, &state.app.keys).await?;
    if let Some(token) = bearer {
        let actor = load_api_key(&mut conn, token, ctx.ip.as_deref(), &security, ctx.now)
            .await?
            .ok_or_else(|| AppError::new(ErrorKind::Unauthorized, "The API key is not valid, has expired or was revoked"))?;
        if surface == Surface::Web {
            ctx.surface = Surface::Api;
        }
        return Ok(ctx.with_actor(actor));
    }
    if let Some(token) = session_cookie(headers, state)
        && let Some((actor, session_id)) = load_session(&mut conn, &token, &security, ctx.now).await?
    {
        ctx.session_id = Some(session_id);
        return Ok(ctx.with_actor(actor));
    }
    Ok(ctx)
}
