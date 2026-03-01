use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatusApiError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("service unavailable: {0}")]
    Unavailable(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for StatusApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            StatusApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            StatusApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            StatusApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            StatusApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            StatusApiError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            StatusApiError::Unavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            StatusApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorBody {
            ok: false,
            error: message,
        });
        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    ok: bool,
    error: String,
}
