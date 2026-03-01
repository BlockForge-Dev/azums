use thiserror::Error;

#[derive(Debug, Error)]
pub enum CallbackCoreError {
    #[error("store error: {0}")]
    Store(String),
    #[error("invalid callback payload: {0}")]
    InvalidPayload(String),
    #[error("callback transport error: {0}")]
    Transport(String),
    #[error("callback security error: {0}")]
    Security(String),
    #[error("callback configuration error: {0}")]
    Configuration(String),
}
