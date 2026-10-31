//! Drop-in replacements for other signing services' APIs. Each maps its requests onto the same
//! services as the native API, so permissions, validation and audit are identical; only the shapes
//! differ. Both take an OpenSign API key in place of the vendor's credentials, and never a cookie.

pub mod docusign;
pub mod scrive;

use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use opensign_core::context::{Ctx, Surface};
use opensign_core::error::{AppError, ErrorKind, bad_request, unauthorized};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{AppState, RequestId, auth, security};

pub struct Api {
    pub state: AppState,
    pub ctx: Ctx,
    pub request_id: String,
}

impl Api {
    /// Authenticates with an API key only. Cookies are ignored, so a browser session cannot be
    /// ridden into these endpoints from another site.
    pub async fn new(state: &AppState, parts: &Parts, token: Option<String>) -> Result<Api, (AppError, String)> {
        let request_id = parts.extensions.get::<RequestId>().map(|r| r.0.clone()).unwrap_or_default();
        let Some(token) = token.filter(|t| !t.is_empty()) else { return Err((unauthorized(), request_id)) };
        let mut headers = HeaderMap::new();
        let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) else { return Err((unauthorized(), request_id)) };
        headers.insert(header::AUTHORIZATION, value);
        if let Some(agent) = parts.headers.get(header::USER_AGENT) {
            headers.insert(header::USER_AGENT, agent.clone());
        }
        let peer = parts.extensions.get::<axum::extract::ConnectInfo<std::net::SocketAddr>>().map(|c| c.0);
        let ip = security::client_ip(state, &parts.headers, peer).await;
        match auth::context(state, &headers, ip.map(|i| i.to_string()), Surface::Compat, request_id.clone()).await {
            Ok(ctx) if ctx.actor.is_some() => Ok(Api { state: state.clone(), ctx, request_id }),
            Ok(_) => Err((unauthorized(), request_id)),
            Err(e) => Err((e, request_id)),
        }
    }

    /// Runs a service by name, as the REST API does. Only tool services are reachable with a key.
    pub async fn call(&self, name: &str, input: Value) -> Result<Value, AppError> {
        let service = self.state.services.get(name).ok_or_else(|| AppError::new(ErrorKind::Internal, "missing service"))?;
        if !service.meta().is_tool() {
            return Err(AppError::new(ErrorKind::Forbidden, "This call is not available with an API key"));
        }
        service.invoke(&self.ctx, input).await
    }

    pub async fn document(&self, id: Uuid) -> Result<Value, AppError> {
        self.call("documents.get", json!({ "id": id })).await
    }

    pub async fn upload(&self, id: Uuid, name: &str, bytes: &[u8]) -> Result<Value, AppError> {
        self.call(
            "documents.upload",
            json!({ "documentId": id, "name": name, "contentBase64": B64.encode(bytes), "textTags": false, "formFields": false }),
        )
        .await
    }

    /// The sealed PDF once complete, otherwise the file as sent (or the draft's files merged).
    pub async fn pdf(&self, id: Uuid) -> Result<(String, Vec<u8>), AppError> {
        let doc = self.document(id).await?;
        let version = if doc["status"] == "completed" { "signed" } else { "original" };
        if doc["status"] == "draft" {
            return opensign_core::services::documents::preview_pdf(&self.ctx, id, None).await;
        }
        let out = self.call("documents.download", json!({ "id": id, "version": version })).await?;
        let bytes = B64.decode(out["contentBase64"].as_str().unwrap_or_default()).map_err(AppError::internal)?;
        Ok((out["name"].as_str().unwrap_or("document.pdf").to_string(), bytes))
    }

    pub async fn handed_over_links(&self, id: Uuid) -> Result<Vec<(Uuid, String)>, AppError> {
        let mut conn = self.ctx.db().acquire().await?;
        opensign_core::services::documents::handed_over_links(&self.ctx, &mut conn, id).await
    }
}

pub fn uuid(value: &str, what: &str) -> Result<Uuid, AppError> {
    Uuid::try_parse(value.trim()).map_err(|_| AppError::new(ErrorKind::NotFound, format!("{what} not found")))
}

pub fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// Splits a full name the way vendor APIs want it: first word, then the rest.
pub fn split_name(name: &str) -> (String, String) {
    let name = name.trim();
    match name.split_once(' ') {
        Some((first, rest)) => (first.to_string(), rest.trim().to_string()),
        None => (name.to_string(), String::new()),
    }
}

pub fn too_large() -> AppError {
    bad_request("The request is too large")
}

/// Logs an internal error with its reference, as the native API does.
pub fn log_internal(error: &AppError, request_id: &str) -> String {
    if error.kind == ErrorKind::Internal {
        tracing::error!(request_id, error = %error, cause = ?error.cause, "internal error in a compatibility API");
        format!("{} (reference {request_id})", error.message)
    } else {
        error.message.clone()
    }
}

/// Pages of the combined PDF, by the PDF's own dimensions in points.
pub async fn page_sizes(pdf: Vec<u8>) -> Result<Vec<(f64, f64)>, AppError> {
    tokio::task::spawn_blocking(move || opensign_pdf::page_sizes(&pdf)).await.map_err(AppError::internal)?.map_err(AppError::internal)
}
