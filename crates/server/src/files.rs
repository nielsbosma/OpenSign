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
use opensign_core::services::transfers;
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
    let ip = security::client_ip(state, &parts.headers, peer).await;
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

/// A signed-in person on the file routes, which are outside the services: the same check as
/// `service::authorize`, including the two-factor setup an admin requires first.
fn member(ctx: &Ctx) -> Result<(), opensign_core::AppError> {
    let actor = ctx.actor()?;
    if actor.must_enable_two_factor {
        return Err(opensign_core::AppError::new(
            opensign_core::ErrorKind::Forbidden,
            "Set up two-factor authentication first. Your administrator requires it.",
        ));
    }
    Ok(())
}

/// May add files to documents. Decided before the upload is read, so nobody else can make the
/// server hold one in memory.
fn may_upload(ctx: &Ctx) -> Result<(), opensign_core::AppError> {
    member(ctx)?;
    if !ctx.can(opensign_core::Permission::DocumentsCreate) {
        return Err(opensign_core::error::forbidden());
    }
    Ok(())
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
        member(&caller.ctx)?;
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

#[derive(Deserialize)]
pub struct PreviewQuery {
    version: Option<i32>,
}

/// `GET /api/documents/<id>/preview[?version=<template version>]`: the combined PDF the editor places fields on.
pub async fn preview(State(state): State<AppState>, Path(id): Path<Uuid>, Query(q): Query<PreviewQuery>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Web, None).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        member(&caller.ctx)?;
        docs::preview_pdf(&caller.ctx, id, q.version).await
    }
    .await;
    match result {
        Ok((name, body)) => pdf(&name, body),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

async fn multipart_file(state: &AppState, parts: Parts, body: Body) -> Result<(String, Vec<u8>), opensign_core::AppError> {
    let mut multipart = Multipart::from_request(Request::from_parts(parts, body), state)
        .await
        .map_err(|_| bad_request("Send the file as multipart/form-data"))?;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("document.pdf").to_string();
            let bytes = field.bytes().await.map_err(|_| bad_request("The upload was interrupted or is too large"))?;
            return Ok((name, bytes.to_vec()));
        }
    }
    Err(bad_request("Attach the document as the `file` field"))
}

/// `POST /api/documents/<id>/files`, multipart with one `file` part.
pub async fn upload(State(state): State<AppState>, Path(id): Path<Uuid>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Web, Some(security::UPLOAD)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let fail = |e| ApiError(e, Some(caller.request_id.clone())).into_response();
    if let Err(e) = may_upload(&caller.ctx) {
        return fail(e);
    }
    let mut multipart = match Multipart::from_request(Request::from_parts(parts, body), &state).await {
        Ok(m) => m,
        Err(_) => return fail(bad_request("Send the file as multipart/form-data")),
    };
    let mut file = None;
    let mut options = docs::FileOptions::default();
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("file") if file.is_none() => {
                let name = field.file_name().unwrap_or("document.pdf").to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, bytes.to_vec())),
                    Err(_) => return fail(bad_request("The upload was interrupted or is too large")),
                }
            }
            Some(key @ ("textTags" | "formFields")) => {
                let on = key == "textTags";
                let value = field.text().await.unwrap_or_default();
                let enabled = !matches!(value.trim(), "false" | "0" | "no");
                if on { options.text_tags = enabled } else { options.form_fields = enabled }
            }
            _ => {}
        }
    }
    let Some((name, bytes)) = file else { return fail(bad_request("Attach the document as the `file` field")) };
    match docs::add_file(&caller.ctx, id, &name, bytes, options).await {
        Ok(output) => Json(output).into_response(),
        Err(e) => fail(e),
    }
}

/// `POST /api/documents/<id>/files/<file id>/replace`, multipart with one `file` part.
pub async fn replace(State(state): State<AppState>, Path((id, file_id)): Path<(Uuid, Uuid)>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let caller = match caller(&state, &parts, Surface::Web, Some(security::UPLOAD)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        may_upload(&caller.ctx)?;
        let (name, bytes) = multipart_file(&state, parts, body).await?;
        docs::replace_file(&caller.ctx, id, file_id, &name, bytes).await
    }
    .await;
    match result {
        Ok(output) => Json(output).into_response(),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
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
        member(&caller.ctx)?;
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

/// `POST /api/sign/field-file`, multipart with `token`, `session` (optional), `field` and `file`:
/// the upload for one of the party's image or file fields.
pub async fn signer_field_file(State(state): State<AppState>, request: Request) -> Response {
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
    let (mut token, mut session, mut field_id, mut file) = (None, None, None, None);
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("token") => token = field.text().await.ok(),
            Some("session") => session = field.text().await.ok().filter(|s| !s.is_empty()),
            Some("field") => field_id = field.text().await.ok().and_then(|r| Uuid::parse_str(r.trim()).ok()),
            Some("file") => {
                let name = field.file_name().unwrap_or("upload").to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, bytes.to_vec())),
                    Err(_) => return fail(bad_request("The upload was interrupted or is too large")),
                }
            }
            _ => {}
        }
    }
    let (Some(token), Some(field_id), Some((name, bytes))) = (token, field_id, file) else {
        return fail(bad_request("Send token, field and file"));
    };
    let result = async {
        let mut tx = caller.ctx.db().begin().await?;
        let out =
            opensign_core::services::signing::upload_field_file(&caller.ctx, &mut tx, &token, session.as_deref(), field_id, &name, bytes)
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
        let (sha256, signatures) = docs::read_signatures_public(body.to_vec()).await?;
        let mut conn = caller.ctx.db().acquire().await?;
        docs::compare_with_seals(&caller.ctx, &mut conn, sha256, signatures).await
    }
    .await;
    match result {
        Ok(output) => Json(output).into_response(),
        Err(e) => ApiError(e, Some(caller.request_id)).into_response(),
    }
}

/// `GET /api/logo/<id>.png`: the organisation's logo, public because signers and email clients
/// load it without signing in. Only the current logo is served, and it was re-encoded on upload.
pub async fn logo(State(state): State<AppState>, Path(file): Path<String>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Signer, None).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let Some(id) = file.strip_suffix(".png").and_then(|id| Uuid::parse_str(id).ok()) else {
        return ApiError(opensign_core::error::not_found("Logo"), Some(caller.request_id)).into_response();
    };
    let result = async {
        let mut conn = caller.ctx.db().acquire().await?;
        let record = opensign_core::services::branding::logo_file(&caller.ctx, &mut conn, id).await?;
        read_file(&caller.ctx.app.storage, caller.ctx.keys(), &record).await
    }
    .await;
    match result {
        Ok(body) => {
            let mut response = Body::from(body).into_response();
            let h = response.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
            h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=31536000, immutable"));
            h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
            h.insert("cross-origin-resource-policy", HeaderValue::from_static("cross-origin"));
            response
        }
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
pub async fn eid_callback(State(app_state): State<AppState>, Query(q): Query<EidCallback>, parts: Parts) -> Response {
    let caller = match caller(&app_state, &parts, Surface::Signer, Some(security::SIGNER)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        let state = q.state.as_deref().filter(|s| !s.is_empty() && s.len() <= 200).ok_or_else(|| bad_request("Missing state"))?;
        let mut conn = caller.ctx.db().acquire().await?;
        let browser = auth::eid_cookie(&parts.headers, &app_state);
        opensign_core::services::eid::broker_callback(
            &caller.ctx,
            &mut conn,
            state,
            browser.as_deref(),
            q.code.as_deref(),
            q.error.as_deref(),
        )
        .await
    }
    .await;
    let location = match result {
        // In the fragment, so that it reaches no log; the page takes it off the address at once.
        Ok(opensign_core::services::eid::Returned { code: Some(code), .. }) => format!("/sign/eid#done={code}"),
        Ok(returned) => format!("/sign?eid={}", returned.id),
        Err(_) => "/sign?eid=invalid".to_string(),
    };
    let mut response = axum::http::StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().append(header::SET_COOKIE, auth::clear_eid_cookie(&app_state));
    response
}

/// `GET /api/auth/sso/callback`: the identity provider returns the browser here. A refusal goes to
/// the sign-in page as a fixed code it has a translated text for.
pub async fn sso_callback(State(app_state): State<AppState>, Query(q): Query<EidCallback>, parts: Parts) -> Response {
    let caller = match caller(&app_state, &parts, Surface::Web, Some(security::AUTH)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let browser = auth::sso_cookie(&parts.headers, &app_state);
    let outcome = opensign_core::services::sso::callback(
        &caller.ctx,
        q.state.as_deref().unwrap_or_default(),
        browser.as_deref(),
        q.code.as_deref(),
        q.error.as_deref(),
    )
    .await;
    let mut response = axum::http::StatusCode::SEE_OTHER.into_response();
    let location = match outcome {
        Ok(session) => {
            response.headers_mut().append(header::SET_COOKIE, auth::set_cookie(&app_state, &session.token, session.expires_at));
            "/".to_string()
        }
        Err(code) => format!("/login?sso={code}"),
    };
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().append(header::SET_COOKIE, auth::clear_sso_cookie(&app_state));
    response
}

fn review_headers(response: &mut Response) {
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    h.insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow, noarchive"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("sandbox; default-src 'none'"));
}

/// Agents read errors as text too, so a refusal says in words what happened.
fn review_error(error: opensign_core::AppError, request_id: String) -> Response {
    let status = axum::http::StatusCode::from_u16(error.kind.status()).unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    if error.kind == opensign_core::ErrorKind::Internal {
        return ApiError(error, Some(request_id)).into_response();
    }
    let mut response = (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], error.message).into_response();
    review_headers(&mut response);
    response
}

/// `GET /review/<token>`: the document as Markdown for the signer's AI agent. Read-only; every
/// fetch is counted and audited.
pub async fn review_text(State(state): State<AppState>, Path(token): Path<String>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Signer, Some(security::REVIEW)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        let mut tx = caller.ctx.db().begin().await?;
        let text = opensign_core::services::review::markdown(&caller.ctx, &mut tx, &token).await?;
        tx.commit().await?;
        Ok::<_, opensign_core::AppError>(text)
    }
    .await;
    match result {
        Ok(text) => {
            let mut response = ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response();
            review_headers(&mut response);
            response
        }
        Err(e) => review_error(e, caller.request_id),
    }
}

/// `GET /review/<token>/document.pdf`: the PDF as sent, for agents that read PDFs themselves.
pub async fn review_pdf(State(state): State<AppState>, Path(token): Path<String>, parts: Parts) -> Response {
    let caller = match caller(&state, &parts, Surface::Signer, Some(security::REVIEW)).await {
        Ok(c) => c,
        Err(r) => return *r,
    };
    let result = async {
        let mut tx = caller.ctx.db().begin().await?;
        let (record, body) = opensign_core::services::review::pdf(&caller.ctx, &mut tx, &token).await?;
        tx.commit().await?;
        Ok::<_, opensign_core::AppError>((record.name, body))
    }
    .await;
    match result {
        Ok((name, body)) => {
            let mut response = pdf(&name, body);
            response.headers_mut().insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow, noarchive"));
            response
        }
        Err(e) => review_error(e, caller.request_id),
    }
}

/// `GET /api/transfers/<token>`: a download link from `documents.download_url`.
pub async fn transfer_download(State(state): State<AppState>, Path(token): Path<String>, parts: Parts) -> Response {
    let ip = security::client_ip(&state, &parts.headers, parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0)).await;
    if !state.limiter.allow(security::UPLOAD, ip) {
        return security::too_many();
    }
    let result = async {
        let record = transfers::download(&state.app, &token).await?;
        let body = read_file(&state.app.storage, &state.app.keys, &record).await?;
        Ok::<_, opensign_core::AppError>((record.name, body))
    }
    .await;
    match result {
        Ok((name, body)) => pdf(&name, body),
        Err(e) => ApiError(e, None).into_response(),
    }
}

/// `PUT /api/transfers/<token>` with the file's raw bytes: an upload link from `documents.upload_url`.
pub async fn transfer_upload(State(state): State<AppState>, Path(token): Path<String>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let request_id = parts.extensions.get::<RequestId>().map(|r| r.0.clone()).unwrap_or_default();
    let ip = security::client_ip(&state, &parts.headers, parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0)).await;
    if !state.limiter.allow(security::UPLOAD, ip) {
        return security::too_many();
    }
    if let Err(e) = transfers::check_upload_link(&state.app, &token) {
        return ApiError(e, Some(request_id)).into_response();
    }
    let Ok(bytes) = to_bytes(body, transfers::MAX_UPLOAD_BYTES).await else {
        return ApiError(bad_request("The upload was interrupted or is too large"), Some(request_id)).into_response();
    };
    if bytes.is_empty() {
        return ApiError(bad_request("Send the file's bytes as the request body"), Some(request_id)).into_response();
    }
    match transfers::upload(state.app.clone(), &token, ip.map(|i| i.to_string()), request_id.clone(), bytes.to_vec()).await {
        Ok(output) => Json(output).into_response(),
        Err(e) => ApiError(e, Some(request_id)).into_response(),
    }
}
