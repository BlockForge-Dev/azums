use crate::model::{
    AdapterExecutionRequest, AdapterId, AdapterOutcome, AdapterRoute, CallbackJob, ExecutionJob,
    IntentId, IntentKind, JobId, NormalizedIntent, OperatorPrincipal, ReplayDecisionRecord,
    TenantId, TimestampMs,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("backend: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum RoutingError {
    #[error("no route for intent kind `{0}`")]
    NoRoute(String),
    #[error("adapter `{0}` is unavailable")]
    AdapterUnavailable(String),
    #[error("routing backend: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum AdapterExecutionError {
    #[error("adapter unavailable: {0}")]
    Unavailable(String),
    #[error("adapter timeout: {0}")]
    Timeout(String),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("contract violation: {0}")]
    ContractViolation(String),
    #[error("unsupported intent: {0}")]
    UnsupportedIntent(String),
    #[error("adapter unauthorized: {0}")]
    Unauthorized(String),
}

#[derive(Debug, Error)]
pub enum CallbackError {
    #[error("callback backend: {0}")]
    Backend(String),
}

#[async_trait]
pub trait DurableStore: Send + Sync {
    async fn persist_intent(&self, intent: &NormalizedIntent) -> Result<(), StoreError>;
    async fn get_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<NormalizedIntent>, StoreError>;
    async fn lookup_intent_by_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
    ) -> Result<Option<IntentId>, StoreError>;
    async fn bind_intent_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
        intent_id: &IntentId,
    ) -> Result<IntentId, StoreError>;

    async fn persist_job(&self, job: &ExecutionJob) -> Result<(), StoreError>;
    async fn update_job(&self, job: &ExecutionJob) -> Result<(), StoreError>;
    async fn get_job(&self, job_id: &JobId) -> Result<Option<ExecutionJob>, StoreError>;
    async fn get_latest_job_for_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<ExecutionJob>, StoreError>;

    async fn record_transition(
        &self,
        transition: &crate::model::StateTransition,
    ) -> Result<(), StoreError>;
    async fn append_receipt(&self, receipt: &crate::model::ReceiptEntry) -> Result<(), StoreError>;
    async fn record_replay_decision(&self, record: &ReplayDecisionRecord)
        -> Result<(), StoreError>;

    async fn enqueue_dispatch(
        &self,
        job_id: &JobId,
        not_before_ms: Option<TimestampMs>,
    ) -> Result<(), StoreError>;
    async fn enqueue_callback_job(&self, callback: &CallbackJob) -> Result<(), StoreError>;
}

pub trait AdapterRouter: Send + Sync {
    fn supported_intent(&self, kind: &IntentKind) -> bool;
    fn resolve_adapter(&self, intent: &NormalizedIntent) -> Result<AdapterRoute, RoutingError>;
    fn adapter_executor(
        &self,
        adapter_id: &AdapterId,
    ) -> Result<Arc<dyn AdapterExecutor>, RoutingError>;
}

#[async_trait]
pub trait AdapterExecutor: Send + Sync {
    async fn execute(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<AdapterOutcome, AdapterExecutionError>;
}

pub trait Authorizer: Send + Sync {
    fn can_route_adapter(&self, tenant_id: &TenantId, adapter_id: &AdapterId) -> bool;
    fn can_replay(&self, principal: &OperatorPrincipal, tenant_id: &TenantId) -> bool;
    fn can_trigger_manual_action(
        &self,
        principal: &OperatorPrincipal,
        tenant_id: &TenantId,
    ) -> bool;
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> TimestampMs;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> TimestampMs {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}
