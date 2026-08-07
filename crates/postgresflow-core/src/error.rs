use thiserror::Error;
use uuid::Uuid;

/// Primary error enum for `postgresflow` job queue operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Storage backend error wrapper.
    #[error("backend error: {0}")]
    Backend(#[from] Box<dyn std::error::Error + Send + Sync>),

    /// Job type has no registered handler.
    #[error("job type not registered: {0}")]
    UnknownJobType(String),

    /// Job with specified ID was not found.
    #[error("job not found: {0}")]
    JobNotFound(Uuid),

    /// Queue is empty or no jobs are ready for leasing.
    #[error("no runnable jobs available in queue: {0}")]
    QueueEmpty(String),

    /// Lease expired for specified job and worker.
    #[error("lease expired for job {job_id} (worker: {worker_id})")]
    LeaseExpired { job_id: Uuid, worker_id: String },

    /// JSON payload deserialization failure.
    #[error("payload deserialization error: {0}")]
    PayloadDeserialization(#[from] serde_json::Error),

    /// Invalid configuration or state transition error.
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// General internal error.
    #[error("internal queue error: {0}")]
    Internal(String),
}

/// Type alias for backward compatibility.
pub type QueueError = Error;
