use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReconError {
    #[error("backend: {0}")]
    Backend(String),
    #[error("rule pack unavailable for adapter `{0}`")]
    RulePackUnavailable(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
}

impl ReconError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Backend(_))
    }
}
