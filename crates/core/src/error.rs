use std::collections::BTreeMap;
use std::fmt::Display;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    TooManyRequests,
    Unavailable,
    Internal,
}

impl ErrorKind {
    pub fn status(self) -> u16 {
        match self {
            ErrorKind::BadRequest => 400,
            ErrorKind::Unauthorized => 401,
            ErrorKind::Forbidden => 403,
            ErrorKind::NotFound => 404,
            ErrorKind::Conflict => 409,
            ErrorKind::TooManyRequests => 429,
            ErrorKind::Unavailable => 503,
            ErrorKind::Internal => 500,
        }
    }
}

/// An error whose message is safe to show the caller. Anything unexpected becomes `Internal`
/// with a generic message; the cause is logged, never returned.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    #[source]
    pub cause: Option<anyhow::Error>,
    /// A protocol error code for callers that speak one, e.g. OAuth's `invalid_grant`.
    pub code: Option<&'static str>,
    /// The message before its placeholders were filled. See `params`.
    pub template: Option<Box<Template>>,
}

/// What the web app translates a filled-in message from.
#[derive(Debug)]
pub struct Template {
    pub text: String,
    pub params: BTreeMap<&'static str, String>,
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), cause: None, code: None, template: None }
    }

    pub fn with_cause(mut self, cause: impl Into<anyhow::Error>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// Fills the `{name}` placeholders of the message. The app translates the unfilled text, keyed by
    /// its English wording; API callers read the filled English.
    pub fn params<const N: usize>(mut self, params: [(&'static str, &dyn Display); N]) -> Self {
        let mut message = self.message.clone();
        let mut filled = BTreeMap::new();
        for (name, value) in params {
            let value = value.to_string();
            message = message.replace(&format!("{{{name}}}"), &value);
            filled.insert(name, value);
        }
        let text = std::mem::replace(&mut self.message, message);
        self.template = Some(Box::new(Template { text, params: filled }));
        self
    }

    pub fn internal(cause: impl Into<anyhow::Error>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: "Something went wrong on our side".into(),
            cause: Some(cause.into()),
            code: None,
            template: None,
        }
    }
}

pub fn bad_request(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::BadRequest, message)
}

pub fn unauthorized() -> AppError {
    AppError::new(ErrorKind::Unauthorized, "Sign in first")
}

pub fn forbidden() -> AppError {
    AppError::new(ErrorKind::Forbidden, "You do not have permission to do that")
}

pub fn not_found(what: &str) -> AppError {
    AppError::new(ErrorKind::NotFound, format!("{what} not found"))
}

pub fn conflict(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::Conflict, message)
}

pub fn too_many(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::TooManyRequests, message)
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &error
            && db.is_unique_violation()
        {
            return conflict("That already exists");
        }
        AppError::internal(error)
    }
}

impl From<opensign_db::DbError> for AppError {
    fn from(error: opensign_db::DbError) -> Self {
        AppError::internal(error)
    }
}

impl From<opensign_crypto::CryptoError> for AppError {
    fn from(error: opensign_crypto::CryptoError) -> Self {
        AppError::internal(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_fill_the_message_and_keep_the_text() {
        let e = bad_request("{name} can use at most {max} MB").params([("name", &"Ada"), ("max", &5)]);
        assert_eq!(e.message, "Ada can use at most 5 MB");
        let template = e.template.expect("params keep the text");
        assert_eq!(template.text, "{name} can use at most {max} MB");
        assert_eq!(template.params["max"], "5");
    }
}
