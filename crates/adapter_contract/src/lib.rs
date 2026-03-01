use async_trait::async_trait;
use execution_core::{
    AdapterExecutionError, AdapterExecutionRequest, AdapterExecutor, AdapterId, AdapterOutcome,
    AdapterRoute, AdapterRouter, AuthContext, IntentKind, NormalizedIntent, RoutingError,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterProgressState {
    Submitted,
    Confirming,
    Landed,
    Finalized,
    FailedRetryable,
    FailedTerminal,
    Blocked,
    ManualInterventionRequired,
}

#[derive(Debug, Clone)]
pub struct AdapterStatusSnapshot {
    pub state: AdapterProgressState,
    pub code: String,
    pub message: String,
    pub provider_reference: Option<String>,
    pub details: BTreeMap<String, String>,
}

impl AdapterStatusSnapshot {
    pub fn from_outcome(outcome: &AdapterOutcome) -> Self {
        match outcome {
            AdapterOutcome::InProgress {
                provider_reference,
                poll_after_ms,
                ..
            } => Self {
                state: AdapterProgressState::Confirming,
                code: "adapter.in_progress".to_owned(),
                message: "adapter execution is still in progress".to_owned(),
                provider_reference: provider_reference.clone(),
                details: poll_after_ms
                    .map(|poll_after_ms| {
                        BTreeMap::from([("poll_after_ms".to_owned(), poll_after_ms.to_string())])
                    })
                    .unwrap_or_default(),
            },
            AdapterOutcome::Succeeded {
                provider_reference, ..
            } => Self {
                state: AdapterProgressState::Finalized,
                code: "adapter.succeeded".to_owned(),
                message: "adapter execution succeeded".to_owned(),
                provider_reference: provider_reference.clone(),
                details: BTreeMap::new(),
            },
            AdapterOutcome::RetryableFailure {
                code,
                message,
                provider_details: _,
                retry_after_ms,
            } => Self {
                state: AdapterProgressState::FailedRetryable,
                code: code.clone(),
                message: message.clone(),
                provider_reference: None,
                details: retry_after_ms
                    .map(|retry_after_ms| {
                        BTreeMap::from([("retry_after_ms".to_owned(), retry_after_ms.to_string())])
                    })
                    .unwrap_or_default(),
            },
            AdapterOutcome::TerminalFailure { code, message, .. } => Self {
                state: AdapterProgressState::FailedTerminal,
                code: code.clone(),
                message: message.clone(),
                provider_reference: None,
                details: BTreeMap::new(),
            },
            AdapterOutcome::Blocked { code, message } => Self {
                state: AdapterProgressState::Blocked,
                code: code.clone(),
                message: message.clone(),
                provider_reference: None,
                details: BTreeMap::new(),
            },
            AdapterOutcome::ManualReview { code, message } => Self {
                state: AdapterProgressState::ManualInterventionRequired,
                code: code.clone(),
                message: message.clone(),
                provider_reference: None,
                details: BTreeMap::new(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterStatusHandle {
    pub intent_id: String,
    pub provider_reference: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AdapterExecutionContext {
    pub request_id: Option<String>,
    pub correlation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub auth_context: Option<AuthContext>,
    pub policy: BTreeMap<String, String>,
    pub metadata: BTreeMap<String, String>,
}

impl AdapterExecutionContext {
    pub fn from_request(request: &AdapterExecutionRequest) -> Self {
        let correlation_id = request
            .correlation_id
            .clone()
            .or_else(|| request.metadata.get("correlation_id").cloned());
        let idempotency_key = request
            .idempotency_key
            .clone()
            .or_else(|| request.metadata.get("idempotency_key").cloned());
        let request_id = request
            .request_id
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| request.metadata.get("request_id").cloned());
        let policy = request
            .metadata
            .iter()
            .filter(|(k, _)| k.starts_with("policy."))
            .map(|(k, v)| (k.trim_start_matches("policy.").to_owned(), v.clone()))
            .collect();

        let mut metadata = request.metadata.clone();
        if let Some(value) = request_id.as_ref() {
            metadata
                .entry("request_id".to_owned())
                .or_insert_with(|| value.clone());
        }
        if let Some(value) = correlation_id.as_ref() {
            metadata
                .entry("correlation_id".to_owned())
                .or_insert_with(|| value.clone());
        }
        if let Some(value) = idempotency_key.as_ref() {
            metadata
                .entry("idempotency_key".to_owned())
                .or_insert_with(|| value.clone());
        }
        if let Some(auth) = request.auth_context.as_ref() {
            if let Some(principal_id) = auth.principal_id.as_ref() {
                metadata
                    .entry("submitter.principal_id".to_owned())
                    .or_insert_with(|| principal_id.clone());
            }
            if let Some(submitter_kind) = auth.submitter_kind.as_ref() {
                metadata
                    .entry("submitter.kind".to_owned())
                    .or_insert_with(|| submitter_kind.clone());
            }
        }

        Self {
            request_id,
            correlation_id,
            idempotency_key,
            auth_context: request.auth_context.clone(),
            policy,
            metadata,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AdapterResumeContext {
    pub correlation_id: Option<String>,
    pub retry_attempt: u32,
    pub previous_provider_reference: Option<String>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AdapterExecutionEnvelope {
    pub status: AdapterStatusSnapshot,
    pub outcome: AdapterOutcome,
}

#[async_trait]
pub trait DomainAdapter: Send + Sync {
    async fn validate(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<(), AdapterExecutionError>;

    async fn execute(
        &self,
        request: &AdapterExecutionRequest,
        context: &AdapterExecutionContext,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError>;

    async fn resume(
        &self,
        request: &AdapterExecutionRequest,
        _context: &AdapterResumeContext,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        let context = AdapterExecutionContext::from_request(request);
        self.execute(request, &context).await
    }

    async fn fetch_status(
        &self,
        _handle: &AdapterStatusHandle,
    ) -> Result<AdapterStatusSnapshot, AdapterExecutionError> {
        Err(AdapterExecutionError::Unavailable(
            "adapter does not support fetch_status".to_owned(),
        ))
    }
}

#[derive(Clone)]
pub struct DomainAdapterExecutor {
    adapter: Arc<dyn DomainAdapter>,
}

impl DomainAdapterExecutor {
    pub fn new(adapter: Arc<dyn DomainAdapter>) -> Self {
        Self { adapter }
    }

    pub fn adapter(&self) -> Arc<dyn DomainAdapter> {
        self.adapter.clone()
    }
}

#[async_trait]
impl AdapterExecutor for DomainAdapterExecutor {
    async fn execute(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<AdapterOutcome, AdapterExecutionError> {
        self.adapter.validate(request).await?;
        let context = AdapterExecutionContext::from_request(request);
        let result = self.adapter.execute(request, &context).await?;
        enforce_result_contract(&result)?;
        Ok(result.outcome)
    }
}

#[derive(Default)]
pub struct AdapterRegistry {
    routes: HashMap<String, (AdapterId, String)>,
    executors: HashMap<String, Arc<dyn AdapterExecutor>>,
    domain_adapters: HashMap<String, Arc<dyn DomainAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_route(
        &mut self,
        intent_kind: impl Into<String>,
        adapter_id: AdapterId,
        rule: impl Into<String>,
    ) {
        self.routes
            .insert(intent_kind.into(), (adapter_id, rule.into()));
    }

    pub fn register_executor(&mut self, adapter_id: AdapterId, executor: Arc<dyn AdapterExecutor>) {
        self.executors.insert(adapter_id.to_string(), executor);
    }

    pub fn register_adapter_for_intent(
        &mut self,
        intent_kind: impl Into<String>,
        adapter_id: AdapterId,
        rule: impl Into<String>,
        executor: Arc<dyn AdapterExecutor>,
    ) {
        self.register_route(intent_kind, adapter_id.clone(), rule);
        self.register_executor(adapter_id, executor);
    }

    pub fn register_domain_adapter(
        &mut self,
        adapter_id: AdapterId,
        adapter: Arc<dyn DomainAdapter>,
    ) {
        let key = adapter_id.to_string();
        self.domain_adapters.insert(key.clone(), adapter.clone());
        self.executors
            .insert(key, Arc::new(DomainAdapterExecutor::new(adapter)));
    }

    pub fn register_domain_adapter_for_intent(
        &mut self,
        intent_kind: impl Into<String>,
        adapter_id: AdapterId,
        rule: impl Into<String>,
        adapter: Arc<dyn DomainAdapter>,
    ) {
        self.register_route(intent_kind, adapter_id.clone(), rule);
        self.register_domain_adapter(adapter_id, adapter);
    }

    pub fn domain_adapter(
        &self,
        adapter_id: &AdapterId,
    ) -> Result<Arc<dyn DomainAdapter>, RoutingError> {
        self.domain_adapters
            .get(adapter_id.as_str())
            .cloned()
            .ok_or_else(|| RoutingError::AdapterUnavailable(adapter_id.to_string()))
    }
}

impl AdapterRouter for AdapterRegistry {
    fn supported_intent(&self, kind: &IntentKind) -> bool {
        self.routes.contains_key(kind.as_str())
    }

    fn resolve_adapter(&self, intent: &NormalizedIntent) -> Result<AdapterRoute, RoutingError> {
        let (adapter_id, rule) = self
            .routes
            .get(intent.kind.as_str())
            .ok_or_else(|| RoutingError::NoRoute(intent.kind.to_string()))?;
        Ok(AdapterRoute {
            adapter_id: adapter_id.clone(),
            rule: rule.clone(),
        })
    }

    fn adapter_executor(
        &self,
        adapter_id: &AdapterId,
    ) -> Result<Arc<dyn AdapterExecutor>, RoutingError> {
        self.executors
            .get(adapter_id.as_str())
            .cloned()
            .ok_or_else(|| RoutingError::AdapterUnavailable(adapter_id.to_string()))
    }
}

fn enforce_result_contract(result: &AdapterExecutionEnvelope) -> Result<(), AdapterExecutionError> {
    if state_matches_outcome(result.status.state, &result.outcome) {
        return Ok(());
    }

    Err(AdapterExecutionError::ContractViolation(format!(
        "adapter status `{}` does not match outcome category `{}`",
        result.status.state.as_str(),
        outcome_label(&result.outcome)
    )))
}

fn state_matches_outcome(state: AdapterProgressState, outcome: &AdapterOutcome) -> bool {
    match state {
        AdapterProgressState::Submitted
        | AdapterProgressState::Confirming
        | AdapterProgressState::Landed => matches!(outcome, AdapterOutcome::InProgress { .. }),
        AdapterProgressState::Finalized => matches!(outcome, AdapterOutcome::Succeeded { .. }),
        AdapterProgressState::FailedRetryable => {
            matches!(outcome, AdapterOutcome::RetryableFailure { .. })
        }
        AdapterProgressState::FailedTerminal => {
            matches!(outcome, AdapterOutcome::TerminalFailure { .. })
        }
        AdapterProgressState::Blocked => matches!(outcome, AdapterOutcome::Blocked { .. }),
        AdapterProgressState::ManualInterventionRequired => {
            matches!(outcome, AdapterOutcome::ManualReview { .. })
        }
    }
}

fn outcome_label(outcome: &AdapterOutcome) -> &'static str {
    match outcome {
        AdapterOutcome::InProgress { .. } => "in_progress",
        AdapterOutcome::Succeeded { .. } => "succeeded",
        AdapterOutcome::RetryableFailure { .. } => "retryable_failure",
        AdapterOutcome::TerminalFailure { .. } => "terminal_failure",
        AdapterOutcome::Blocked { .. } => "blocked",
        AdapterOutcome::ManualReview { .. } => "manual_review",
    }
}

impl AdapterProgressState {
    fn as_str(self) -> &'static str {
        match self {
            AdapterProgressState::Submitted => "submitted",
            AdapterProgressState::Confirming => "confirming",
            AdapterProgressState::Landed => "landed",
            AdapterProgressState::Finalized => "finalized",
            AdapterProgressState::FailedRetryable => "failed_retryable",
            AdapterProgressState::FailedTerminal => "failed_terminal",
            AdapterProgressState::Blocked => "blocked",
            AdapterProgressState::ManualInterventionRequired => "manual_intervention_required",
        }
    }
}
