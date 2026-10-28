use axum::Json;
use axum::body::{Body, to_bytes};
use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequest, Multipart, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use opensign_core::context::{Ctx, Surface};
use opensign_core::error::bad_request;
use opensign_core::services::documents::{self as docs, Version};
use opensign_core::services::signing::signer_file;
use opensign_core::storage::read_file;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::security::{self, Limit};
use crate::{AppState, RequestId, auth};

const VERIFY_LIMIT: usize = 40 * 1024 * 1024;

pub fn content_disposition(name: &str) -> HeaderValue {
    let ascii: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    HeaderValue::from_str(&format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

/// PDFs are served as downloads under a sandboxing policy, so a file opened directly in the
/// browser cannot run script in our origin.
fn pdf(name: &str, body: Vec<u8>) -> Response {
    let mut response = Body::from(body).into_response();
    let h = response.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/pdf"));
    h.insert(header::CONTENT_DISPOSITION, content_disposition(name));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
    response
}

struct Caller {
    ctx: Ctx,
    request_id: String,
}

async fn caller(state: &AppState, parts: &Parts, surface: Surface, limit: Option<Limit>) -> Result<Caller, Box<Response>> {
    let request_id = parts.extensions.get::<RequestId>().map(|r| r.0.clone()).unwrap_or_default();
    let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let ip = security::client_ip(&parts.headers, peer);
    if let Some(limit) = limit
        && !state.limiter.allow(limit, ip)
    {
        return Err(Box::new(security::too_many()));
    }
    auth::context(state, &parts.headers, ip.map(|i| i.to_string()), surface, request_id.clone())
        .await
        .map(|ctx| Caller { ctx, request_id: request_id.clone() })
        .map_err(|e| Box::new(ApiError(e, Some(request_id)).into_response()))
}

#[derive(Deserialize)]
pub struct VersionQuery {
    version: Option<String>,
}

/// `GET /api/documents/<id>/file?version=signed|original`
pub async fn document_file(State(state): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<VersionQuery>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Web, None).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let version = match q.version.as_deref() {
        None | Some("signed") => Version::Signed,
        Some("original") => Version::Original,
        Some(_) => return ApiError(bad_request("version is signed or original"), None).into_response(),
    };
    let result = async {
        caller.ctx.actor()?;
        let mut conn = caller.ctx.db().acquire().await?;
        let record = docs::document_file(&caller.ctx, &mut conn, id, version).await?;
        let body = read_file(&caller.ctx.app.storage, caller.ctx.keys(), &record).await?;
        Ok::<_, opensign_core::AppError>((record.name, body))
    }
    .await;
    match result {
        Ok((name, body)) => pdf(&name, body),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

/// `GET /api/documents/<id>/preview`: the combined PDF the editor places fields on.
pub async fn preview(State(state): State<AppState>, Path(id): Path<Uuid>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Web, None).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        caller.ctx.actor()?;
        docs::preview_pdf(&caller.ctx, id).await
    }
    .await;
    match result {
        Ok((name, body)) => pdf(&name, body),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

/// `POST /api/documents/<id>/files`, multipart with one `file` part.
pub async fn upload(State(state): State<AppState>, Path(id): Path<Uuid>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Web, Some(security::UPLOAD)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fail = |e| ApiError(e, Some(caller.request_id.clone())).into_response();
    let mut multipart = match Multipart::from_request(Request::from_parts(parts, body), &state).await {
        Ok(m) => m,
        Err(_) => return fail(bad_request("Send the file as multipart/form-data")),
    };
    let mut file = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("document.pdf").to_string();
            match field.bytes().await {
                Ok(bytes) => file = Some((name, bytes.to_vec())),
                Err(_) => return fail(bad_request("The upload was interrupted or is too large")),
            }
            break;
        }
    }
    let Some((name, bytes)) = file else { return fail(bad_request("Attach the document as the `file` field")) };
    if let Err(e) = caller.ctx.actor() {
        return fail(e);
    }
    if !caller.ctx.can(opensign_core::Permission::DocumentsCreate) {
        return fail(opensign_core::error::forbidden());
    }
    match docs::add_file(&caller.ctx, id, &name, bytes).await {
        Ok(output) => Json(output).into_response(),
        Err(e) => fail(e),
    }
}

/// A party's upload, as a download in a sandbox: images and PDFs never render in our origin.
fn attachment(name: &str, content_type: &str, body: Vec<u8>) -> Response {
    let mut response = Body::from(body).into_response();
    let h = response.headers_mut();
    let content_type = match content_type {
        "application/pdf" => "application/pdf",
        "image/png" => "image/png",
        _ => "application/octet-stream",
    };
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    h.insert(header::CONTENT_DISPOSITION, content_disposition(name));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
    response
}

/// `GET /api/documents/<id>/attachments/<request id>`
pub async fn document_attachment(State(state): State<AppState>, Path((id, request)): Path<(Uuid, Uuid)>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Web, None).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        caller.ctx.actor()?;
        let mut conn = caller.ctx.db().acquire().await?;
        let record = docs::attachment_file(&caller.ctx, &mut conn, id, request).await?;
        let body = read_file(&caller.ctx.app.storage, caller.ctx.keys(), &record).await?;
        Ok::<_, opensign_core::AppError>((record.name, record.content_type, body))
    }
    .await;
    match result {
        Ok((name, content_type, body)) => attachment(&name, &content_type, body),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

/// `POST /api/sign/attachment`, multipart with `token`, `session` (optional), `request` and `file`.
pub async fn signer_attachment(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Signer, Some(security::UPLOAD)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fail = |e| ApiError(e, Some(caller.request_id.clone())).into_response();
    let mut multipart = match Multipart::from_request(Request::from_parts(parts, body), &state).await {
        Ok(m) => m,
        Err(_) => return fail(bad_request("Send the file as multipart/form-data")),
    };
    let (mut token, mut session, mut request_id, mut file) = (None, None, None, None);
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("token") => token = field.text().await.ok(),
            Some("session") => session = field.text().await.ok().filter(|s| !s.is_empty()),
            Some("request") => request_id = field.text().await.ok().and_then(|r| Uuid::parse_str(r.trim()).ok()),
            Some("file") => {
                let name = field.file_name().unwrap_or("attachment").to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, bytes.to_vec())),
                    Err(_) => return fail(bad_request("The upload was interrupted or is too large")),
                }
            }
            _ => {}
        }
    }
    let (Some(token), Some(request_id), Some((name, bytes))) = (token, request_id, file) else {
        return fail(bad_request("Send token, request and file"));
    };
    let result = async {
        let mut tx = caller.ctx.db().begin().await?;
        let out =
            opensign_core::services::signing::upload_attachment(&caller.ctx, &mut tx, &token, session.as_deref(), request_id, &name, bytes)
                .await?;
        tx.commit().await?;
        Ok::<_, opensign_core::AppError>(out)
    }
    .await;
    match result {
        Ok(out) => Json(out).into_response(),
        Err(e) => fail(e),
    }
}

#[derive(Deserialize)]
pub struct SignerFileInput {
    token: String,
    session: Option<String>,
}

/// `POST /api/sign/file` with `{ token, session }`: the PDF behind a signing link. A POST keeps the
/// token out of URLs and logs.
pub async fn signer(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Signer, Some(security::SIGNER)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fail = |e| ApiError(e, Some(caller.request_id.clone())).into_response();
    let Ok(body) = to_bytes(body, 16 * 1024).await else { return fail(bad_request("The request is too large")) };
    let Ok(input) = serde_json::from_slice::<SignerFileInput>(&body) else { return fail(bad_request("Send { token, session }")) };
    let result = async {
        let mut conn = caller.ctx.db().acquire().await?;
        let record = signer_file(&caller.ctx, &mut conn, &input.token, input.session.as_deref()).await?;
        let body = read_file(&caller.ctx.app.storage, caller.ctx.keys(), &record).await?;
        Ok::<_, opensign_core::AppError>((record.name, body))
    }
    .await;
    match result {
        Ok((name, body)) => pdf(&name, body),
        Err(e) => fail(e),
    }
}

/// `POST /api/verify` with a PDF body: anyone holding a signed file may check it.
pub async fn verify(State(state): State<AppState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Web, Some(security::VERIFY)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let Ok(body) = to_bytes(body, VERIFY_LIMIT).await else {
        return ApiError(bad_request("The file is too large"), None).into_response();
    };
    let result = async {
        let mut conn = caller.ctx.db().acquire().await?;
        docs::verify_pdf(&mut conn, body.to_vec()).await
    }
    .await;
    match result {
        Ok(output) => Json(output).into_response(),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

#[derive(Deserialize)]
pub struct EidCallback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

/// `GET /api/eid/callback`: the eID broker or TIC Identity returns the browser here. The signing page picks the
/// result up with the session id; the page itself still holds the signing link.
pub async fn eid_callback(State(state): State<AppState>, Query(q): Query<EidCallback>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Signer, Some(security::SIGNER)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        let state = q.state.as_deref().filter(|s| !s.is_empty() && s.len() <= 200).ok_or_else(|| bad_request("Missing state"))?;
        let mut conn = caller.ctx.db().acquire().await?;
        opensign_core::services::eid::broker_callback(&caller.ctx, &mut conn, state, q.code.as_deref(), q.error.as_deref()).await
    }
    .await;
    let location = match result {
        Ok(id) => format!("/sign?eid={id}"),
        Err(_) => "/sign?eid=invalid".to_string(),
    };
    let mut response = axum::http::StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
