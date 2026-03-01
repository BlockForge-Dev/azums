use async_trait::async_trait;
use execution_core::engine::ExecutionCore;
use execution_core::error::CoreError;
use execution_core::model::{
    AdapterExecutionRequest, AdapterId, AdapterOutcome, CallbackJob, CanonicalState, ExecutionJob,
    IntentId, IntentKind, LeaseId, LeasedJob, NormalizedIntent, OperatorPrincipal, OperatorRole,
    ReceiptEntry, ReplayCommand, ReplayDecisionRecord, StateTransition, TenantId,
};
use execution_core::policy::{ReplayPolicy, RetryPolicy};
use execution_core::ports::{
    AdapterExecutionError, AdapterExecutor, AdapterRouter, Authorizer, Clock, DurableStore,
    RoutingError, StoreError,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct InMemoryStore {
    intents: Mutex<HashMap<(String, String), NormalizedIntent>>,
    idempotency: Mutex<HashMap<(String, String), String>>,
    jobs: Mutex<HashMap<String, ExecutionJob>>,
    latest: Mutex<HashMap<(String, String), String>>,
    transitions: Mutex<Vec<StateTransition>>,
    receipts: Mutex<Vec<ReceiptEntry>>,
    replay_decisions: Mutex<Vec<ReplayDecisionRecord>>,
    dispatches: Mutex<Vec<(String, Option<u64>)>>,
    callbacks: Mutex<Vec<CallbackJob>>,
    op_log: Mutex<Vec<String>>,
}

impl InMemoryStore {
    async fn latest_job(&self, tenant: &TenantId, intent: &IntentId) -> Option<ExecutionJob> {
        self.get_latest_job_for_intent(tenant, intent)
            .await
            .ok()
            .flatten()
    }
}

#[async_trait]
impl DurableStore for InMemoryStore {
    async fn persist_intent(&self, intent: &NormalizedIntent) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push("persist_intent".to_owned());
        self.intents.lock().unwrap().insert(
            (intent.tenant_id.to_string(), intent.intent_id.to_string()),
            intent.clone(),
        );
        Ok(())
    }

    async fn get_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<NormalizedIntent>, StoreError> {
        Ok(self
            .intents
            .lock()
            .unwrap()
            .get(&(tenant_id.to_string(), intent_id.to_string()))
            .cloned())
    }

    async fn lookup_intent_by_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
    ) -> Result<Option<IntentId>, StoreError> {
        Ok(self
            .idempotency
            .lock()
            .unwrap()
            .get(&(tenant_id.to_string(), idempotency_key.to_owned()))
            .cloned()
            .map(IntentId::from))
    }

    async fn bind_intent_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
        intent_id: &IntentId,
    ) -> Result<IntentId, StoreError> {
        let mut guard = self.idempotency.lock().unwrap();
        let key = (tenant_id.to_string(), idempotency_key.to_owned());
        let entry = guard
            .entry(key)
            .or_insert_with(|| intent_id.to_string())
            .clone();
        Ok(IntentId::from(entry))
    }

    async fn persist_job(&self, job: &ExecutionJob) -> Result<(), StoreError> {
        self.op_log.lock().unwrap().push("persist_job".to_owned());
        self.jobs
            .lock()
            .unwrap()
            .insert(job.job_id.to_string(), job.clone());
        self.latest.lock().unwrap().insert(
            (job.tenant_id.to_string(), job.intent_id.to_string()),
            job.job_id.to_string(),
        );
        Ok(())
    }

    async fn update_job(&self, job: &ExecutionJob) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push(format!("update_job:{:?}", job.state));
        self.jobs
            .lock()
            .unwrap()
            .insert(job.job_id.to_string(), job.clone());
        self.latest.lock().unwrap().insert(
            (job.tenant_id.to_string(), job.intent_id.to_string()),
            job.job_id.to_string(),
        );
        Ok(())
    }

    async fn get_job(
        &self,
        job_id: &execution_core::model::JobId,
    ) -> Result<Option<ExecutionJob>, StoreError> {
        Ok(self.jobs.lock().unwrap().get(job_id.as_str()).cloned())
    }

    async fn get_latest_job_for_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<ExecutionJob>, StoreError> {
        let key = (tenant_id.to_string(), intent_id.to_string());
        let maybe = self.latest.lock().unwrap().get(&key).cloned();
        Ok(maybe.and_then(|job_id| self.jobs.lock().unwrap().get(&job_id).cloned()))
    }

    async fn record_transition(&self, transition: &StateTransition) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push(format!("transition:{:?}", transition.to_state));
        self.transitions.lock().unwrap().push(transition.clone());
        Ok(())
    }

    async fn append_receipt(&self, receipt: &ReceiptEntry) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push(format!("receipt:{:?}", receipt.state));
        self.receipts.lock().unwrap().push(receipt.clone());
        Ok(())
    }

    async fn record_replay_decision(
        &self,
        record: &ReplayDecisionRecord,
    ) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push(format!("replay_decision:{}", record.allowed));
        self.replay_decisions.lock().unwrap().push(record.clone());
        Ok(())
    }

    async fn enqueue_dispatch(
        &self,
        job_id: &execution_core::model::JobId,
        not_before_ms: Option<u64>,
    ) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push("enqueue_dispatch".to_owned());
        self.dispatches
            .lock()
            .unwrap()
            .push((job_id.to_string(), not_before_ms));
        Ok(())
    }

    async fn enqueue_callback_job(&self, callback: &CallbackJob) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push("enqueue_callback".to_owned());
        self.callbacks.lock().unwrap().push(callback.clone());
        Ok(())
    }
}

struct SequencedAdapter {
    outcomes: Mutex<Vec<AdapterOutcome>>,
}

impl SequencedAdapter {
    fn new(outcomes: Vec<AdapterOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes),
        }
    }
}

#[async_trait]
impl AdapterExecutor for SequencedAdapter {
    async fn execute(
        &self,
        _request: &AdapterExecutionRequest,
    ) -> Result<AdapterOutcome, AdapterExecutionError> {
        let mut outcomes = self.outcomes.lock().unwrap();
        if outcomes.is_empty() {
            return Err(AdapterExecutionError::Unavailable("no outcome".to_owned()));
        }
        Ok(outcomes.remove(0))
    }
}

struct TestRouter {
    adapter_id: AdapterId,
    adapter: Arc<SequencedAdapter>,
}

impl AdapterRouter for TestRouter {
    fn supported_intent(&self, kind: &IntentKind) -> bool {
        kind.as_str() == "transfer.v1"
    }

    fn resolve_adapter(
        &self,
        _intent: &NormalizedIntent,
    ) -> Result<execution_core::model::AdapterRoute, RoutingError> {
        Ok(execution_core::model::AdapterRoute {
            adapter_id: self.adapter_id.clone(),
            rule: "kind=transfer.v1".to_owned(),
        })
    }

    fn adapter_executor(
        &self,
        adapter_id: &AdapterId,
    ) -> Result<Arc<dyn AdapterExecutor>, RoutingError> {
        if adapter_id != &self.adapter_id {
            return Err(RoutingError::AdapterUnavailable(adapter_id.to_string()));
        }
        Ok(self.adapter.clone())
    }
}

struct TestAuthorizer {
    allow_route: bool,
    allow_replay: bool,
    allow_manual: bool,
}

impl Authorizer for TestAuthorizer {
    fn can_route_adapter(&self, _tenant_id: &TenantId, _adapter_id: &AdapterId) -> bool {
        self.allow_route
    }

    fn can_replay(&self, _principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        self.allow_replay
    }

    fn can_trigger_manual_action(
        &self,
        _principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        self.allow_manual
    }
}

struct TestClock(AtomicU64);

impl TestClock {
    fn new(seed: u64) -> Self {
        Self(AtomicU64::new(seed))
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

fn intent(now_ms: u64) -> NormalizedIntent {
    NormalizedIntent {
        request_id: None,
        intent_id: IntentId::new(),
        tenant_id: TenantId::from("tenant_a"),
        kind: IntentKind::new("transfer.v1"),
        payload: serde_json::json!({"amount":"10"}),
        correlation_id: None,
        idempotency_key: None,
        auth_context: None,
        metadata: BTreeMap::new(),
        received_at_ms: now_ms,
    }
}

#[tokio::test]
async fn terminal_transition_happens_before_callback_enqueue() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![AdapterOutcome::Succeeded {
            provider_reference: Some("ok".to_owned()),
            details: BTreeMap::new(),
        }])),
    });
    let core = ExecutionCore::new(
        store.clone(),
        router,
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: true,
            allow_manual: true,
        }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(TestClock::new(1_000)),
    );

    let submitted = core.submit_intent(intent(1_000)).await.unwrap();
    let lease = LeasedJob {
        lease_id: LeaseId::new(),
        job: submitted.job,
        leased_at_ms: 1_001,
        lease_expires_at_ms: 2_000,
    };
    let dispatched = core.dispatch_job(lease).await.unwrap();
    assert_eq!(dispatched.job.state, CanonicalState::Succeeded);

    let log = store.op_log.lock().unwrap().clone();
    let t_idx = log
        .iter()
        .position(|v| v == "transition:Succeeded")
        .unwrap();
    let c_idx = log.iter().position(|v| v == "enqueue_callback").unwrap();
    assert!(t_idx < c_idx);
}

#[tokio::test]
async fn retryable_failure_then_terminal_on_exhaustion() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![
            AdapterOutcome::RetryableFailure {
                code: "timeout".to_owned(),
                message: "first timeout".to_owned(),
                retry_after_ms: None,
                provider_details: None,
            },
            AdapterOutcome::RetryableFailure {
                code: "timeout".to_owned(),
                message: "second timeout".to_owned(),
                retry_after_ms: None,
                provider_details: None,
            },
        ])),
    });
    let core = ExecutionCore::new(
        store.clone(),
        router,
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: true,
            allow_manual: true,
        }),
        RetryPolicy {
            max_attempts: 2,
            base_delay_ms: 1_000,
            max_delay_ms: 5_000,
            jitter_percent: 0,
        },
        ReplayPolicy::default(),
        Arc::new(TestClock::new(2_000)),
    );

    let submitted = core.submit_intent(intent(2_000)).await.unwrap();
    let first = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job.clone(),
            leased_at_ms: 2_001,
            lease_expires_at_ms: 3_000,
        })
        .await
        .unwrap();
    assert_eq!(first.job.state, CanonicalState::RetryScheduled);

    let retry_job = store
        .latest_job(&first.job.tenant_id, &first.job.intent_id)
        .await
        .unwrap();
    let second = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: retry_job,
            leased_at_ms: 2_010,
            lease_expires_at_ms: 3_010,
        })
        .await
        .unwrap();
    assert_eq!(second.job.state, CanonicalState::DeadLettered);
}

#[tokio::test]
async fn replay_is_authorized_and_replayable_only() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![
            AdapterOutcome::TerminalFailure {
                code: "tx_invalid".to_owned(),
                message: "rejected".to_owned(),
                provider_details: None,
            },
        ])),
    });
    let allowed_core = ExecutionCore::new(
        store.clone(),
        router,
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: true,
            allow_manual: true,
        }),
        RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        },
        ReplayPolicy::default(),
        Arc::new(TestClock::new(3_000)),
    );
    let submitted = allowed_core.submit_intent(intent(3_000)).await.unwrap();
    let failed = allowed_core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job,
            leased_at_ms: 3_001,
            lease_expires_at_ms: 4_000,
        })
        .await
        .unwrap();
    assert_eq!(failed.job.state, CanonicalState::FailedTerminal);

    let denied_core = ExecutionCore::new(
        store.clone(),
        Arc::new(TestRouter {
            adapter_id: AdapterId::from("adapter_exec"),
            adapter: Arc::new(SequencedAdapter::new(vec![])),
        }),
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: false,
            allow_manual: false,
        }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(TestClock::new(4_000)),
    );

    let denied = denied_core
        .request_replay(ReplayCommand {
            tenant_id: failed.job.tenant_id.clone(),
            intent_id: failed.job.intent_id.clone(),
            requested_by: OperatorPrincipal {
                principal_id: "op-denied".to_owned(),
                role: OperatorRole::Operator,
            },
            reason: "replay".to_owned(),
        })
        .await;
    assert!(matches!(denied, Err(CoreError::UnauthorizedReplay { .. })));

    let replayed = allowed_core
        .request_replay(ReplayCommand {
            tenant_id: failed.job.tenant_id.clone(),
            intent_id: failed.job.intent_id.clone(),
            requested_by: OperatorPrincipal {
                principal_id: "op-admin".to_owned(),
                role: OperatorRole::Admin,
            },
            reason: "fixed downstream".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(replayed.replay_job.state, CanonicalState::Queued);
    assert_eq!(replayed.replay_job.replay_count, 1);
}

#[tokio::test]
async fn in_progress_is_non_terminal_until_finalized() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![
            AdapterOutcome::InProgress {
                provider_reference: Some("sig-pending".to_owned()),
                details: BTreeMap::new(),
                poll_after_ms: Some(250),
            },
            AdapterOutcome::Succeeded {
                provider_reference: Some("sig-final".to_owned()),
                details: BTreeMap::new(),
            },
        ])),
    });
    let core = ExecutionCore::new(
        store.clone(),
        router,
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: true,
            allow_manual: true,
        }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(TestClock::new(10_000)),
    );

    let submitted = core.submit_intent(intent(10_000)).await.unwrap();
    let first = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job,
            leased_at_ms: 10_001,
            lease_expires_at_ms: 11_000,
        })
        .await
        .unwrap();
    assert_eq!(first.job.state, CanonicalState::RetryScheduled);
    assert_eq!(first.job.attempt, 0);
    assert!(store.callbacks.lock().unwrap().is_empty());

    let retry_job = store
        .latest_job(&first.job.tenant_id, &first.job.intent_id)
        .await
        .unwrap();
    let second = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: retry_job,
            leased_at_ms: 10_010,
            lease_expires_at_ms: 11_010,
        })
        .await
        .unwrap();
    assert_eq!(second.job.state, CanonicalState::Succeeded);
    assert_eq!(second.job.attempt, 1);
    assert_eq!(store.callbacks.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn submit_is_deduped_by_tenant_and_idempotency_key() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![AdapterOutcome::Succeeded {
            provider_reference: Some("ok".to_owned()),
            details: BTreeMap::new(),
        }])),
    });
    let core = ExecutionCore::new(
        store.clone(),
        router,
        Arc::new(TestAuthorizer {
            allow_route: true,
            allow_replay: true,
            allow_manual: true,
        }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(TestClock::new(20_000)),
    );

    let mut first_intent = intent(20_000);
    first_intent.idempotency_key = Some("dup-key-1".to_owned());
    let first = core.submit_intent(first_intent).await.unwrap();

    let mut second_intent = intent(20_001);
    second_intent.idempotency_key = Some("dup-key-1".to_owned());
    let second = core.submit_intent(second_intent).await.unwrap();

    assert_eq!(first.job.job_id, second.job.job_id);
    assert!(second.route_rule.starts_with("idempotency_reuse:"));

    let transitions = store.transitions.lock().unwrap().len();
    assert_eq!(
        transitions, 3,
        "duplicate submit should not create new lifecycle transitions"
    );
    let dispatches = store.dispatches.lock().unwrap().len();
    assert_eq!(
        dispatches, 1,
        "duplicate submit should not enqueue a second dispatch job"
    );
}
