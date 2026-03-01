use crate::model::{AdapterId, CanonicalState, IntentKind, JobId, TenantId};
use crate::ports::{AdapterExecutionError, CallbackError, RoutingError, StoreError};
use thiserror::Error;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("unsupported intent `{0}`")]
    UnsupportedIntent(IntentKind),
    #[error("adapter routing denied for tenant `{tenant_id}` and adapter `{adapter_id}`")]
    AdapterRoutingDenied {
        tenant_id: TenantId,
        adapter_id: AdapterId,
    },
    #[error("illegal lifecycle transition: {from:?} -> {to:?}")]
    IllegalTransition {
        from: Option<CanonicalState>,
        to: CanonicalState,
    },
    #[error("job `{0}` not found")]
    JobNotFound(JobId),
    #[error("intent not found `{0}`")]
    IntentNotFound(String),
    #[error("tenant mismatch for job `{job_id}`: expected `{expected}`, got `{actual}`")]
    TenantMismatch {
        job_id: JobId,
        expected: TenantId,
        actual: TenantId,
    },
    #[error("unauthorized replay by `{principal_id}`")]
    UnauthorizedReplay { principal_id: String },
    #[error("replay denied: {reason}")]
    ReplayDenied { reason: String },
    #[error("idempotency conflict for key `{key}`: {reason}")]
    IdempotencyConflict { key: String, reason: String },
    #[error("unauthorized manual action by `{principal_id}`")]
    UnauthorizedManualAction { principal_id: String },
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("routing error: {0}")]
    Routing(#[from] RoutingError),
    #[error("adapter execution error: {0}")]
    AdapterExecution(#[from] AdapterExecutionError),
    #[error("callback error: {0}")]
    Callback(#[from] CallbackError),
}
