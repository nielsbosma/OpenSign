use axum::Json;
use axum::body::to_bytes;
use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use opensign_core::context::{Credential, Surface};
use opensign_core::error::{AppError, ErrorKind, bad_request};
use serde_json::Value;

use crate::error::{ApiError, json_error};
use crate::security::{self, Limit};
use crate::{AppState, RequestId, auth};

const RPC_BODY_LIMIT: usize = 40 * 1024 * 1024;

fn limit_for(name: &str) -> Option<Limit> {
    match name {
        "auth.login"
        | "auth.verify_two_factor"
        | "auth.passkey_begin"
        | "auth.passkey_sign_in"
        | "auth.request_password_reset"
        | "auth.reset_password"
        | "users.accept_invite"
        | "users.change_email"
        | "auth.confirm_email_change"
        | "account.change_password"
        | "account.two_factor_confirm"
        | "setup.create_first_admin" => Some(security::AUTH),
        "signing.request_code" | "signing.verify_code" | "share_links.request_code" | "share_links.confirm" => Some(security::CODES),
        "share_links.open" => Some(security::SIGNER),
        n if n.starts_with("signing.") => Some(security::SIGNER),
        "documents.upload" => Some(security::UPLOAD),
        "documents.verify" => Some(security::VERIFY),
        _ => None,
    }
}

/// `POST /api/rpc/<service>`: every service by its dotted name, with JSON in and out. The web app,
/// the signing page and API clients all use it; MCP and the assistant call the same services.
pub async fn call(State(state): State<AppState>, Path(name): Path<String>, request: Request) -> Response {
    let request_id = request.extensions().get::<RequestId>().map(|r| r.0.clone()).unwrap_or_default();
    let Some(service) = state.services.get(name.as_str()) else {
        return json_error(StatusCode::NOT_FOUND, "not_found", "There is no such call");
    };
    let ip = security::client_ip(request.headers(), security::peer(&request));
    if let Some(limit) = limit_for(&name)
        && !state.limiter.allow(limit, ip)
    {
        return security::too_many();
    }
    let surface = if name.starts_with("signing.") { Surface::Signer } else { Surface::Web };
    let headers = request.headers().clone();
    let ctx = match auth::context(&state, &headers, ip.map(|i| i.to_string()), surface, request_id.clone()).await {
        Ok(ctx) => ctx,
        Err(e) => return ApiError(e, Some(request_id)).into_response(),
    };
    if let Some(actor) = &ctx.actor
        && actor.credential != Credential::Session
        && !service.meta().tool
    {
        return ApiError(AppError::new(ErrorKind::Forbidden, "This call is only available signed in to the app"), Some(request_id))
            .into_response();
    }
    let body = match to_bytes(request.into_body(), RPC_BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "bad_request", "The request is too large"),
    };
    let input: Value = if body.iter().all(u8::is_ascii_whitespace) {
        Value::Null
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => return ApiError(bad_request("The body is not valid JSON"), Some(request_id)).into_response(),
        }
    };
    let mut output = match service.invoke(&ctx, input).await {
        Ok(output) => output,
        Err(e) => return ApiError(e, Some(request_id)).into_response(),
    };

    let mut response_headers = Vec::new();
    match name.as_str() {
        "auth.login" | "auth.verify_two_factor" | "auth.passkey_sign_in" if output["status"] == "signed_in" => {
            let token = output["token"].as_str().unwrap_or_default().to_string();
            let expires = output["expiresAt"]
                .as_str()
                .or(output["expires_at"].as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(chrono::Utc::now);
            response_headers.push((header::SET_COOKIE, auth::set_cookie(&state, &token, expires)));
            if let Some(map) = output.as_object_mut() {
                map.remove("token");
            }
        }
        "auth.logout" => response_headers.push((header::SET_COOKIE, auth::clear_cookie(&state))),
        _ => {}
    }
    let mut response = Json(output).into_response();
    for (key, value) in response_headers {
        response.headers_mut().append(key, value);
    }
    response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    response
}
