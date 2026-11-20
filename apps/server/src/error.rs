use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Unified server error mapped to structured JSON HTTP responses.
#[derive(Debug)]
pub enum ServerError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    PayloadTooLarge(String),
    RangeNotSatisfiable { total_bytes: u64 },
    Internal(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "bad request: {msg}"),
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            Self::Forbidden(msg) => write!(f, "forbidden: {msg}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::PayloadTooLarge(msg) => write!(f, "payload too large: {msg}"),
            Self::RangeNotSatisfiable { total_bytes } => {
                write!(f, "range not satisfiable for total_bytes={total_bytes}")
            }
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for ServerError {}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        match self {
            Self::RangeNotSatisfiable { total_bytes } => (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [("Content-Range", format!("bytes */{total_bytes}"))],
                Json(json!({ "error": "range not satisfiable" })),
            )
                .into_response(),
            other => {
                let (status, message) = match other {
                    Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
                    Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
                    Self::Forbidden(m) => (StatusCode::FORBIDDEN, m),
                    Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
                    Self::Conflict(m) => (StatusCode::CONFLICT, m),
                    Self::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, m),
                    Self::Internal(m) => {
                        tracing::error!(error = %m, "internal server error");
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "internal server error".to_string(),
                        )
                    }
                    Self::RangeNotSatisfiable { .. } => unreachable!(),
                };
                (status, Json(json!({ "error": message }))).into_response()
            }
        }
    }
}

impl From<rusqlite::Error> for ServerError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Internal(format!("database error: {err}"))
    }
}

impl From<std::io::Error> for ServerError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(format!("i/o error: {err}"))
    }
}
