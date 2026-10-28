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
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), cause: None }
    }

    pub fn internal(cause: impl Into<anyhow::Error>) -> Self {
        Self { kind: ErrorKind::Internal, message: "Something went wrong on our side".into(), cause: Some(cause.into()) }
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
        if let sqlx::Error::Database(db) = &error {
            if db.is_unique_violation() {
                return conflict("That already exists");
            }
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
