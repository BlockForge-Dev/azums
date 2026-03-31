use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExceptionIntelligenceError {
    #[error("backend: {0}")]
    Backend(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),
}
