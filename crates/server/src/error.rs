use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use opensign_core::{AppError, ErrorKind};
use serde_json::json;

pub fn json_error(status: StatusCode, kind: &str, message: &str) -> Response {
    (status, Json(json!({ "error": message, "kind": kind }))).into_response()
}

/// The message of an `AppError` is safe to show; an internal error's cause is logged with a
/// reference and never returned.
pub struct ApiError(pub AppError, pub Option<String>);

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        ApiError(error, None)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let ApiError(error, request_id) = self;
        let status = StatusCode::from_u16(error.kind.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let kind = serde_json::to_value(error.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        if error.kind == ErrorKind::Internal {
            tracing::error!(request_id = request_id.as_deref().unwrap_or("-"), error = %error, cause = ?error.cause, "internal error");
            return match &request_id {
                Some(id) => {
                    let text = format!("{} (reference {{id}})", error.message);
                    let message = format!("{} (reference {id})", error.message);
                    (status, Json(json!({ "error": message, "kind": kind, "text": text, "params": { "id": id } }))).into_response()
                }
                None => json_error(status, &kind, &error.message),
            };
        }
        let mut body = json!({ "error": error.message, "kind": kind });
        if let Some(code) = error.code {
            body["code"] = json!(code);
        }
        if let Some(template) = &error.template {
            body["text"] = json!(template.text);
            body["params"] = json!(template.params);
        }
        (status, Json(body)).into_response()
    }
}
