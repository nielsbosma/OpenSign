//! Remote MCP over Streamable HTTP (JSON responses, no sessions) at `/mcp`, and the OAuth 2.1
//! endpoints MCP clients discover through the RFC 8414 and RFC 9728 metadata documents. Every
//! tool is a service the user's role may call, so the UI, the API and MCP share one rulebook.

use std::collections::HashMap;

use axum::Router;
use axum::body::to_bytes;
use axum::extract::{RawQuery, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use opensign_core::context::{Ctx, Surface};
use opensign_core::error::{AppError, ErrorKind};
use opensign_core::oauth::{self, Authorize, AuthorizeRequest, SCOPE};
use opensign_core::service::{Kind, can_access};
use opensign_core::settings::SecuritySettings;
use serde_json::{Value, json};

use crate::error::json_error;
use crate::rest::query_pairs;
use crate::security::{self, Limit};
use crate::{AppState, RequestId};

const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const BODY_LIMIT: usize = 40 * 1024 * 1024;
const TOKEN: Limit = Limit { name: "oauth", per_minute: 30, burst: 20 };

const INSTRUCTIONS: &str = "OpenSign is an electronic signature service. Documents start as drafts: create one, upload a PDF or Word \
file (base64), add parties (signers, approvers, viewers) and fields placed as fractions of the page, then send. Signers get \
a personal link by email or SMS, or you can fetch a link to hand over. Completed documents are sealed PDFs with an evidence \
appendix; download them with documents_download. Every action is recorded in the audit log under the connected user.";

fn base(state: &AppState) -> String {
    state.app.public_url()
}

fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn ip_of(request: &Request) -> Option<std::net::IpAddr> {
    security::client_ip(request.headers(), security::peer(request))
}

async fn authorization_server(State(state): State<AppState>) -> Json<Value> {
    let base = base(&state);
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "revocation_endpoint": format!("{base}/oauth/revoke"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none"],
        "revocation_endpoint_auth_methods_supported": ["none"],
        "scopes_supported": [SCOPE],
    }))
}

async fn protected_resource(State(state): State<AppState>) -> Json<Value> {
    let base = base(&state);
    Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "scopes_supported": [SCOPE],
        "bearer_methods_supported": ["header"],
        "resource_name": "OpenSign",
    }))
}

fn oauth_failure(e: AppError) -> Response {
    let status = if e.kind == ErrorKind::Internal { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::BAD_REQUEST };
    if e.kind == ErrorKind::Internal {
        tracing::error!(error = %e, cause = ?e.cause, "oauth endpoint failed");
    }
    let code = e.code.unwrap_or(if e.kind == ErrorKind::Internal { "server_error" } else { "invalid_request" });
    no_store((status, Json(json!({ "error": code, "error_description": e.message }))).into_response())
}

async fn register(State(state): State<AppState>, request: Request) -> Response {
    if !state.limiter.allow(security::AUTH, ip_of(&request)) {
        return security::too_many();
    }
    let Ok(body) = to_bytes(request.into_body(), 64 * 1024).await else {
        return oauth_failure(oauth::oauth_error("invalid_client_metadata", "Too large"));
    };
    let Ok(metadata) = serde_json::from_slice::<Value>(&body) else {
        return oauth_failure(oauth::oauth_error("invalid_client_metadata", "The body is not JSON"));
    };
    let result = async {
        let mut conn = state.app.db.acquire().await?;
        oauth::register_client(&mut conn, &metadata).await
    }
    .await;
    match result {
        Ok(info) => no_store((StatusCode::CREATED, Json(info)).into_response()),
        Err(e) => oauth_failure(e),
    }
}

async fn authorize(State(state): State<AppState>, RawQuery(query): RawQuery) -> Response {
    let params: HashMap<String, String> = query_pairs(query.as_deref().unwrap_or_default()).into_iter().collect();
    let get = |k: &str| params.get(k).map(String::as_str);
    let request = AuthorizeRequest {
        response_type: get("response_type"),
        client_id: get("client_id"),
        redirect_uri: get("redirect_uri"),
        code_challenge: get("code_challenge"),
        code_challenge_method: get("code_challenge_method"),
        state: get("state"),
        resource: get("resource"),
        scope: get("scope"),
    };
    let result = async {
        let mut conn = state.app.db.acquire().await?;
        oauth::authorize(&state.app, &mut conn, request).await
    }
    .await;
    match result {
        Ok(Authorize::Consent(url) | Authorize::Redirect(url)) => Redirect::to(&url).into_response(),
        Err(e) if e.kind == ErrorKind::Internal => oauth_failure(e),
        Err(e) => json_error(StatusCode::BAD_REQUEST, "bad_request", &e.message),
    }
}

async fn token(State(state): State<AppState>, request: Request) -> Response {
    if !state.limiter.allow(TOKEN, ip_of(&request)) {
        return security::too_many();
    }
    let Ok(body) = to_bytes(request.into_body(), 16 * 1024).await else {
        return oauth_failure(oauth::oauth_error("invalid_request", "Too large"));
    };
    let form: HashMap<String, String> = query_pairs(&String::from_utf8_lossy(&body)).into_iter().collect();
    let get = |k: &str| form.get(k).map(String::as_str).filter(|v| !v.is_empty());
    let result = async {
        let client_id = get("client_id").ok_or_else(|| oauth::oauth_error("invalid_client", "client_id is required"))?;
        let mut tx = state.app.db.begin().await?;
        let now = chrono::Utc::now();
        let tokens = match get("grant_type") {
            Some("authorization_code") => {
                let code = get("code").ok_or_else(|| oauth::oauth_error("invalid_request", "code is required"))?;
                let verifier = get("code_verifier").ok_or_else(|| oauth::oauth_error("invalid_request", "code_verifier is required"))?;
                oauth::exchange_code(&mut tx, client_id, code, verifier, get("redirect_uri"), now).await
            }
            Some("refresh_token") => {
                let refresh = get("refresh_token").ok_or_else(|| oauth::oauth_error("invalid_request", "refresh_token is required"))?;
                oauth::refresh(&mut tx, client_id, refresh, now).await
            }
            _ => Err(oauth::oauth_error("unsupported_grant_type", "Use authorization_code or refresh_token")),
        };
        // A rejected refresh may have revoked a grant on reuse; keep that.
        tx.commit().await?;
        tokens
    }
    .await;
    match result {
        Ok(tokens) => no_store(Json(tokens).into_response()),
        Err(e) => oauth_failure(e),
    }
}

async fn revoke(State(state): State<AppState>, request: Request) -> Response {
    if !state.limiter.allow(TOKEN, ip_of(&request)) {
        return security::too_many();
    }
    let Ok(body) = to_bytes(request.into_body(), 16 * 1024).await else { return StatusCode::BAD_REQUEST.into_response() };
    let form: HashMap<String, String> = query_pairs(&String::from_utf8_lossy(&body)).into_iter().collect();
    if let Some(token) = form.get("token")
        && let Ok(mut conn) = state.app.db.acquire().await
        && let Err(e) = oauth::revoke(&mut conn, form.get("client_id").map(String::as_str), token, chrono::Utc::now()).await
    {
        tracing::warn!(error = %e, "token revocation failed");
    }
    no_store(StatusCode::OK.into_response())
}

fn unauthorized(state: &AppState, message: &str) -> Response {
    let mut response = (StatusCode::UNAUTHORIZED, Json(json!({ "error": "invalid_token", "error_description": message }))).into_response();
    let challenge = format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"", base(state));
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

/// The caller of an MCP request: an OAuth access token, or an API key for clients configured with
/// a static header.
async fn mcp_context(state: &AppState, request: &Parts) -> Result<Ctx, Box<Response>> {
    let request_id = request.extensions.get::<RequestId>().map(|r| r.0.clone()).unwrap_or_default();
    let peer = request.extensions.get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0);
    let Some(token) = request.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "))
    else {
        return Err(Box::new(unauthorized(state, "Sign in to OpenSign to connect")));
    };
    let token = token.trim().to_string();
    let mut ctx = Ctx::new(state.app.clone(), Surface::Mcp);
    ctx.ip = security::client_ip(&request.headers, peer).map(|i| i.to_string());
    ctx.user_agent = request.headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(|s| s.chars().take(300).collect());
    ctx.request_id = Some(request_id);
    let actor = async {
        let mut conn = state.app.db.acquire().await?;
        let security: SecuritySettings = state.app.settings.get(&mut conn, &state.app.keys).await?;
        match oauth::load_access_token(&mut conn, &state.app, &token, &security, ctx.now).await? {
            Some(actor) => Ok::<_, AppError>(Some(actor)),
            None => opensign_core::auth::load_api_key(&mut conn, &token, ctx.ip.as_deref(), &security, ctx.now).await,
        }
    }
    .await
    .map_err(|e| {
        tracing::error!(error = %e, cause = ?e.cause, "could not check an MCP token");
        Box::new(json_error(StatusCode::INTERNAL_SERVER_ERROR, "internal", "Something went wrong on our side"))
    })?;
    let Some(actor) = actor else { return Err(Box::new(unauthorized(state, "The token is invalid or expired"))) };
    if actor.must_enable_two_factor {
        return Err(Box::new(json_error(StatusCode::FORBIDDEN, "forbidden", "Set up two-factor authentication in OpenSign first")));
    }
    Ok(ctx.with_actor(actor))
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tools(state: &AppState, ctx: &Ctx) -> Vec<Value> {
    let mut list: Vec<Value> = state
        .services
        .values()
        .filter(|s| s.meta().tool && can_access(s.meta().access, ctx))
        .map(|s| {
            let meta = s.meta();
            let mut schema = serde_json::to_value(s.input_schema()).unwrap_or_else(|_| json!({ "type": "object" }));
            if let Some(o) = schema.as_object_mut() {
                o.remove("$schema");
                o.remove("title");
                o.entry("type").or_insert(json!("object"));
                o.entry("properties").or_insert(json!({}));
            }
            json!({
                "name": meta.tool_name(),
                "description": meta.description,
                "inputSchema": schema,
                "annotations": {
                    "readOnlyHint": meta.kind == Kind::Query,
                    "destructiveHint": matches!(meta.name, "documents.delete" | "documents.cancel" | "webhooks.delete" | "api_keys.revoke" | "users.update"),
                    "openWorldHint": matches!(meta.name, "documents.send" | "documents.remind" | "documents.detect_fields" | "webhooks.test" | "webhooks.resend" | "webhooks.resend_failed"),
                },
            })
        })
        .collect();
    list.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    list
}

async fn handle_message(state: &AppState, ctx: &Ctx, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message["method"].as_str().unwrap_or_default();
    let id = id?;
    Some(match method {
        "initialize" => {
            let requested = message["params"]["protocolVersion"].as_str().unwrap_or_default();
            let version = PROTOCOL_VERSIONS.iter().find(|v| **v == requested).unwrap_or(&PROTOCOL_VERSIONS[0]);
            rpc_result(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "opensign", "title": "OpenSign", "version": opensign_core::env::app_version() },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(&id, json!({})),
        "tools/list" => rpc_result(&id, json!({ "tools": tools(state, ctx) })),
        "tools/call" => {
            let name = message["params"]["name"].as_str().unwrap_or_default();
            let service =
                state.services.values().find(|s| s.meta().tool && s.meta().tool_name() == name && can_access(s.meta().access, ctx));
            let Some(service) = service else { return Some(rpc_error(&id, -32602, &format!("Unknown tool {name}"))) };
            let arguments = message["params"]["arguments"].clone();
            let arguments = if arguments.is_null() { json!({}) } else { arguments };
            match service.invoke(ctx, arguments).await {
                Ok(result) => rpc_result(
                    &id,
                    json!({
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default() }],
                        "structuredContent": { "result": result },
                        "isError": false,
                    }),
                ),
                Err(e) => {
                    let text = if e.kind == ErrorKind::Internal {
                        tracing::error!(tool = name, error = %e, cause = ?e.cause, "MCP tool failed");
                        "The tool failed unexpectedly".to_string()
                    } else {
                        e.message.clone()
                    };
                    rpc_result(&id, json!({ "content": [{ "type": "text", "text": text }], "isError": true }))
                }
            }
        }
        "resources/list" => rpc_result(&id, json!({ "resources": [] })),
        "prompts/list" => rpc_result(&id, json!({ "prompts": [] })),
        _ => rpc_error(&id, -32601, &format!("Method {method} is not supported")),
    })
}

async fn mcp(State(state): State<AppState>, request: Request) -> Response {
    // Browsers attach Origin; a page on another site must not reach a local or remote MCP server.
    if let Some(origin) = request.headers().get(header::ORIGIN).and_then(|o| o.to_str().ok())
        && origin.trim_end_matches('/') != base(&state)
    {
        return json_error(StatusCode::FORBIDDEN, "forbidden", "Cross-origin request refused");
    }
    if !state.limiter.allow(security::GENERAL, ip_of(&request)) {
        return security::too_many();
    }
    let (parts, body) = request.into_parts();
    let ctx = match mcp_context(&state, &parts).await {
        Ok(ctx) => ctx,
        Err(response) => return *response,
    };
    let Ok(body) = to_bytes(body, BODY_LIMIT).await else {
        return json_error(StatusCode::PAYLOAD_TOO_LARGE, "bad_request", "The request is too large");
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, Json(rpc_error(&Value::Null, -32700, "Parse error"))).into_response();
    };
    let reply = match &payload {
        Value::Array(batch) if !batch.is_empty() && batch.len() <= 50 => {
            let mut out = Vec::new();
            for message in batch {
                if let Some(r) = handle_message(&state, &ctx, message).await {
                    out.push(r);
                }
            }
            (!out.is_empty()).then_some(Value::Array(out))
        }
        Value::Object(_) => handle_message(&state, &ctx, &payload).await,
        _ => Some(rpc_error(&Value::Null, -32600, "Invalid request")),
    };
    match reply {
        Some(reply) => no_store(Json(reply).into_response()),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

async fn method_not_allowed() -> Response {
    let mut response = json_error(StatusCode::METHOD_NOT_ALLOWED, "bad_request", "Use POST");
    response.headers_mut().insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(authorization_server))
        .route("/.well-known/oauth-protected-resource", get(protected_resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(protected_resource))
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token))
        .route("/oauth/revoke", post(revoke))
        .route("/mcp", post(mcp).get(method_not_allowed).delete(method_not_allowed))
}
