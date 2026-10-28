pub mod auth;
pub mod compat;
pub mod error;
pub mod files;
pub mod mcp;
pub mod openapi;
pub mod rest;
pub mod rpc;
pub mod security;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use opensign_core::context::App;
use opensign_core::service::DynService;
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};

use crate::error::json_error;

#[derive(Clone)]
pub struct AppState {
    pub app: Arc<App>,
    pub services: Arc<HashMap<&'static str, Box<dyn DynService>>>,
    pub limiter: Arc<security::RateLimiter>,
    pub https: bool,
}

impl AppState {
    pub fn new(app: Arc<App>) -> Self {
        let services = opensign_core::services::registry().into_iter().map(|s| (s.meta().name, s)).collect();
        let https = app.env.app_url.as_deref().is_some_and(|u| u.starts_with("https://"));
        Self { app, services: Arc::new(services), limiter: Arc::default(), https }
    }
}

#[derive(Clone)]
pub struct RequestId(pub String);

async fn request_id(mut request: Request, next: Next) -> Response {
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    request.extensions_mut().insert(RequestId(id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "version": opensign_core::env::app_version() }))
}

async fn ready(State(state): State<AppState>) -> Response {
    let ok = sqlx::query("select 1").execute(&state.app.db).await.is_ok();
    let status = if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({ "ready": ok }))).into_response()
}

async fn api_not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "Not found")
}

/// The built web app: next to the binary in the image, or `web/dist` in a checkout.
fn web_dist() -> Option<PathBuf> {
    // Only a built app: serving `web/` itself would publish the sources.
    let configured = std::env::var("WEB_DIST").ok().map(PathBuf::from);
    configured.into_iter().chain([PathBuf::from("web/dist"), PathBuf::from("/app/web")]).find(|p| p.join("index.html").is_file())
}

async fn no_cache_html(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let is_html = response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if is_html {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    }
    response
}

/// Every service's name, kind and JSON schemas, for generating the web app's typed client.
pub fn service_schemas() -> serde_json::Value {
    let mut services = serde_json::Map::new();
    for service in opensign_core::services::registry() {
        let meta = service.meta();
        services.insert(
            meta.name.to_string(),
            json!({
                "kind": if meta.kind == opensign_core::service::Kind::Query { "query" } else { "mutation" },
                "input": service.input_schema(),
                "output": service.output_schema(),
            }),
        );
    }
    serde_json::Value::Object(services)
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/rpc/{name}", post(rpc::call))
        .route("/documents/{id}/file", get(files::document_file))
        .route("/documents/{id}/preview", get(files::preview))
        .route("/documents/{id}/files", post(files::upload).layer(DefaultBodyLimit::max(110 * 1024 * 1024)))
        .route("/sign/file", post(files::signer))
        .route("/verify", post(files::verify))
        .nest("/v1", rest::router(&state))
        .nest("/v2", compat::scrive::router())
        .fallback(api_not_found)
        .layer(middleware::from_fn_with_state(state.clone(), security::same_origin))
        .layer(middleware::from_fn_with_state(state.clone(), security::general_limit));

    let docusign = compat::docusign::router().layer(middleware::from_fn_with_state(state.clone(), security::general_limit));
    let mut app = Router::new().nest("/api", api).nest("/restapi/v2.1", docusign).merge(mcp::router());
    if let Some(dist) = web_dist() {
        let spa = ServeDir::new(&dist).fallback(ServeFile::new(dist.join("index.html")));
        app = app.fallback_service(spa).layer(middleware::from_fn(no_cache_html));
    }
    app.layer(middleware::from_fn_with_state(state.clone(), security::headers)).layer(middleware::from_fn(request_id)).with_state(state)
}
