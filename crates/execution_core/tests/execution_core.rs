use async_trait::async_trait;
use execution_core::engine::ExecutionCore;
use execution_core::error::CoreError;
use execution_core::model::{
    AdapterExecutionRequest, AdapterId, AdapterOutcome, AuthContext, CallbackJob, CanonicalState,
    ExecutionJob, IdempotencyBinding, IntentId, IntentKind, LeaseId, LeasedJob, NormalizedIntent,
    OperatorPrincipal, OperatorRole, ReceiptEntry, ReconIntakeSignal, ReconIntakeSignalKind,
    ReplayCommand, ReplayDecisionRecord, StateTransition, TenantId,
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
    idempotency: Mutex<HashMap<(String, String), IdempotencyBinding>>,
    jobs: Mutex<HashMap<String, ExecutionJob>>,
    latest: Mutex<HashMap<(String, String), String>>,
    transitions: Mutex<Vec<StateTransition>>,
    receipts: Mutex<Vec<ReceiptEntry>>,
    recon_signals: Mutex<Vec<ReconIntakeSignal>>,
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

    fn latest_receipt(&self) -> ReceiptEntry {
        self.receipts
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("expected at least one receipt")
    }

    fn signal_kinds(&self) -> Vec<ReconIntakeSignalKind> {
        self.recon_signals
            .lock()
            .unwrap()
            .iter()
            .map(|signal| signal.signal_kind)
            .collect()
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
    ) -> Result<Option<IdempotencyBinding>, StoreError> {
        Ok(self
            .idempotency
            .lock()
            .unwrap()
            .get(&(tenant_id.to_string(), idempotency_key.to_owned()))
            .cloned())
    }

    async fn bind_intent_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
        intent_id: &IntentId,
        request_fingerprint: &str,
    ) -> Result<IdempotencyBinding, StoreError> {
        let mut guard = self.idempotency.lock().unwrap();
        let key = (tenant_id.to_string(), idempotency_key.to_owned());
        let entry = guard
            .entry(key)
            .or_insert_with(|| IdempotencyBinding {
                intent_id: intent_id.clone(),
                request_fingerprint: Some(request_fingerprint.to_owned()),
            })
            .clone();
        Ok(entry)
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

    async fn record_recon_intake_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<(), StoreError> {
        self.op_log
            .lock()
            .unwrap()
            .push(format!("recon_signal:{}", signal.signal_kind.as_str()));
        self.recon_signals.lock().unwrap().push(signal.clone());
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

fn intent_with_idempotency(now_ms: u64, idempotency_key: &str, amount: &str) -> NormalizedIntent {
    let mut value = intent(now_ms);
    value.idempotency_key = Some(idempotency_key.to_owned());
    value.payload = serde_json::json!({ "amount": amount });
    value
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
async fn idempotent_submit_reuses_existing_job_for_same_request() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![])),
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

    let first = core
        .submit_intent(intent_with_idempotency(10_000, "idem-1", "10"))
        .await
        .unwrap();
    let second = core
        .submit_intent(intent_with_idempotency(10_001, "idem-1", "10"))
        .await
        .unwrap();

    assert_eq!(first.job.intent_id, second.job.intent_id);
    assert_eq!(first.job.job_id, second.job.job_id);
}

#[tokio::test]
async fn idempotency_conflict_rejects_changed_request_shape() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![])),
    });
    let core = ExecutionCore::new(
        store,
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

    core.submit_intent(intent_with_idempotency(20_000, "idem-2", "10"))
        .await
        .unwrap();

    let err = core
        .submit_intent(intent_with_idempotency(20_001, "idem-2", "99"))
        .await
        .unwrap_err();

    match err {
        CoreError::IdempotencyConflict { key, reason } => {
            assert_eq!(key, "idem-2");
            assert!(reason.contains("different normalized request"));
        }
        other => panic!("expected idempotency conflict, got {other:?}"),
    }
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
async fn recon_receipts_capture_references_and_emit_signals() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![
            AdapterOutcome::InProgress {
                provider_reference: Some("sig-pending".to_owned()),
                details: BTreeMap::from([
                    ("signature".to_owned(), "sig-pending".to_owned()),
                    ("attempt_id".to_owned(), "attempt-1".to_owned()),
                ]),
                poll_after_ms: Some(250),
            },
            AdapterOutcome::Succeeded {
                provider_reference: Some("sig-final".to_owned()),
                details: BTreeMap::from([("tx_hash".to_owned(), "sig-final".to_owned())]),
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
        Arc::new(TestClock::new(20_000)),
    );

    let submitted = core.submit_intent(intent(20_000)).await.unwrap();
    let first = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job,
            leased_at_ms: 20_001,
            lease_expires_at_ms: 21_000,
        })
        .await
        .unwrap();

    let first_receipt = store.latest_receipt();
    let expected_subject_id = format!("reconsub_{}", first_receipt.job_id);
    assert_eq!(first_receipt.receipt_version, 3);
    assert_eq!(
        first_receipt.recon_subject_id.as_deref(),
        Some(expected_subject_id.as_str())
    );
    assert!(first_receipt.reconciliation_eligible);
    assert_eq!(
        first_receipt.adapter_execution_reference.as_deref(),
        Some("sig-pending")
    );
    assert_eq!(
        first_receipt.external_observation_key.as_deref(),
        Some("sig-pending")
    );
    assert!(first_receipt.expected_fact_snapshot.is_some());
    assert_eq!(
        store.signal_kinds(),
        vec![ReconIntakeSignalKind::SubmittedWithReference]
    );

    let retry_job = store
        .latest_job(&first.job.tenant_id, &first.job.intent_id)
        .await
        .unwrap();
    core.dispatch_job(LeasedJob {
        lease_id: LeaseId::new(),
        job: retry_job,
        leased_at_ms: 20_010,
        lease_expires_at_ms: 21_010,
    })
    .await
    .unwrap();

    let final_receipt = store.latest_receipt();
    assert_eq!(final_receipt.receipt_version, 3);
    assert!(final_receipt.reconciliation_eligible);
    assert_eq!(
        final_receipt.adapter_execution_reference.as_deref(),
        Some("sig-final")
    );
    assert_eq!(
        final_receipt.external_observation_key.as_deref(),
        Some("sig-final")
    );
    let signal_kinds = store.signal_kinds();
    assert_eq!(signal_kinds.len(), 4);
    assert_eq!(signal_kinds[1], ReconIntakeSignalKind::AdapterCompleted);
    assert_eq!(signal_kinds[2], ReconIntakeSignalKind::Finalized);
    assert_eq!(
        signal_kinds[3],
        ReconIntakeSignalKind::SubmittedWithReference
    );
}

#[tokio::test]
async fn terminal_failure_receipts_emit_terminal_recon_signal() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![
            AdapterOutcome::TerminalFailure {
                code: "boom".to_owned(),
                message: "adapter failed terminally".to_owned(),
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
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(TestClock::new(30_000)),
    );

    let submitted = core.submit_intent(intent(30_000)).await.unwrap();
    let dispatched = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job,
            leased_at_ms: 30_001,
            lease_expires_at_ms: 31_000,
        })
        .await
        .unwrap();
    assert_eq!(dispatched.job.state, CanonicalState::FailedTerminal);

    let receipt = store.latest_receipt();
    assert_eq!(receipt.receipt_version, 3);
    assert!(receipt.reconciliation_eligible);
    assert!(receipt.adapter_execution_reference.is_none());
    assert!(receipt.expected_fact_snapshot.is_some());
    assert_eq!(
        store.signal_kinds(),
        vec![
            ReconIntakeSignalKind::AdapterCompleted,
            ReconIntakeSignalKind::TerminalFailure,
        ]
    );
}

#[test]
fn legacy_receipt_json_deserializes_with_recon_defaults() {
    let receipt: ReceiptEntry = serde_json::from_value(serde_json::json!({
        "receipt_id": "receipt_legacy",
        "tenant_id": "tenant_a",
        "intent_id": "intent_legacy",
        "job_id": "job_legacy",
        "attempt_no": 0,
        "state": "received",
        "classification": "Success",
        "summary": "legacy receipt",
        "details": { "reason_code": "legacy" },
        "occurred_at_ms": 1
    }))
    .unwrap();

    assert_eq!(receipt.receipt_version, 1);
    assert_eq!(receipt.recon_subject_id, None);
    assert!(!receipt.reconciliation_eligible);
    assert_eq!(receipt.execution_correlation_id, None);
    assert_eq!(receipt.adapter_execution_reference, None);
    assert_eq!(receipt.external_observation_key, None);
    assert_eq!(receipt.expected_fact_snapshot, None);
    assert!(receipt.agent_action.is_none());
    assert!(receipt.agent_identity.is_none());
    assert!(receipt.runtime_identity.is_none());
    assert!(receipt.policy_decision.is_none());
    assert!(receipt.approval_result.is_none());
    assert!(receipt.grant_reference.is_none());
    assert!(receipt.execution_mode.is_none());
    assert!(receipt.connector_outcome.is_none());
    assert!(receipt.recon_linkage.is_none());
}

#[tokio::test]
async fn agent_receipts_capture_requested_approved_executed_and_verified_context() {
    let store = Arc::new(InMemoryStore::default());
    let router = Arc::new(TestRouter {
        adapter_id: AdapterId::from("adapter_exec"),
        adapter: Arc::new(SequencedAdapter::new(vec![AdapterOutcome::Succeeded {
            provider_reference: Some("sig-final".to_owned()),
            details: BTreeMap::from([("signature".to_owned(), "sig-final".to_owned())]),
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
        Arc::new(TestClock::new(40_000)),
    );

    let mut value = intent(40_000);
    value.request_id = Some("req_agent_1".into());
    value.correlation_id = Some("corr-agent-1".to_owned());
    value.idempotency_key = Some("idem-agent-1".to_owned());
    value.auth_context = Some(AuthContext {
        principal_id: Some("runtime_demo".to_owned()),
        submitter_kind: Some("agent_runtime".to_owned()),
        auth_scheme: Some("runtime_client_credentials".to_owned()),
        channel: Some("agent_gateway".to_owned()),
        agent_id: Some("agent_123".to_owned()),
        environment_id: Some("env_prod".to_owned()),
        runtime_type: Some("slack".to_owned()),
        runtime_identity: Some("runtime://slack/bot".to_owned()),
        trust_tier: Some("reviewed".to_owned()),
        risk_tier: Some("high".to_owned()),
    });
    value.metadata = BTreeMap::from([
        ("agent.id".to_owned(), "agent_123".to_owned()),
        ("agent.environment_id".to_owned(), "env_prod".to_owned()),
        ("agent.environment_kind".to_owned(), "production".to_owned()),
        ("agent.status".to_owned(), "active".to_owned()),
        ("agent.trust_tier".to_owned(), "reviewed".to_owned()),
        ("agent.risk_tier".to_owned(), "high".to_owned()),
        ("agent.owner_team".to_owned(), "ops".to_owned()),
        ("agent.action_request_id".to_owned(), "act_123".to_owned()),
        ("agent.intent_type".to_owned(), "transfer".to_owned()),
        ("agent.adapter_type".to_owned(), "solana_adapter".to_owned()),
        (
            "agent.requested_scope".to_owned(),
            "payments,treasury".to_owned(),
        ),
        ("agent.effective_scope".to_owned(), "payments".to_owned()),
        ("agent.reason".to_owned(), "vendor payout".to_owned()),
        ("agent.submitted_by".to_owned(), "ops-bot".to_owned()),
        ("policy.decision".to_owned(), "require_approval".to_owned()),
        (
            "policy.explanation".to_owned(),
            "prod transfers require approval".to_owned(),
        ),
        ("policy.bundle_id".to_owned(), "bundle_prod".to_owned()),
        ("policy.bundle_version".to_owned(), "7".to_owned()),
        ("approval.request_id".to_owned(), "apr_123".to_owned()),
        ("approval.state".to_owned(), "approved".to_owned()),
        ("approval.required_approvals".to_owned(), "1".to_owned()),
        ("approval.approvals_received".to_owned(), "1".to_owned()),
        ("approval.approved_by".to_owned(), "alice".to_owned()),
        ("grant.id".to_owned(), "grant_123".to_owned()),
        (
            "grant.source_action_request_id".to_owned(),
            "act_123".to_owned(),
        ),
        (
            "grant.source_approval_request_id".to_owned(),
            "apr_123".to_owned(),
        ),
        (
            "grant.source_policy_bundle_id".to_owned(),
            "bundle_prod".to_owned(),
        ),
        (
            "grant.source_policy_bundle_version".to_owned(),
            "7".to_owned(),
        ),
        ("grant.expires_at_ms".to_owned(), "45000".to_owned()),
        (
            "execution.mode".to_owned(),
            "mode_c_protected_execution".to_owned(),
        ),
        (
            "execution.owner".to_owned(),
            "azums_protected_execution".to_owned(),
        ),
        ("execution.policy".to_owned(), "sponsored".to_owned()),
        (
            "execution.policy.base".to_owned(),
            "customer_signed".to_owned(),
        ),
        ("execution.signing_mode".to_owned(), "sponsored".to_owned()),
        ("execution.payer_source".to_owned(), "azums".to_owned()),
        ("execution.fee_payer".to_owned(), "wallet_1".to_owned()),
        ("connector.outcome".to_owned(), "not_used".to_owned()),
    ]);

    let submitted = core.submit_intent(value).await.unwrap();
    let dispatched = core
        .dispatch_job(LeasedJob {
            lease_id: LeaseId::new(),
            job: submitted.job,
            leased_at_ms: 40_001,
            lease_expires_at_ms: 41_000,
        })
        .await
        .unwrap();
    assert_eq!(dispatched.job.state, CanonicalState::Succeeded);

    let receipt = store.latest_receipt();
    assert_eq!(receipt.receipt_version, 3);
    assert_eq!(
        receipt
            .agent_action
            .as_ref()
            .and_then(|value| value.action_request_id.as_deref()),
        Some("act_123")
    );
    assert_eq!(
        receipt
            .agent_identity
            .as_ref()
            .and_then(|value| value.agent_id.as_deref()),
        Some("agent_123")
    );
    assert_eq!(
        receipt
            .runtime_identity
            .as_ref()
            .and_then(|value| value.runtime_identity.as_deref()),
        Some("runtime://slack/bot")
    );
    assert_eq!(
        receipt
            .policy_decision
            .as_ref()
            .and_then(|value| value.decision.as_deref()),
        Some("require_approval")
    );
    assert_eq!(
        receipt
            .approval_result
            .as_ref()
            .map(|value| value.result.as_str()),
        Some("approved")
    );
    assert_eq!(
        receipt
            .grant_reference
            .as_ref()
            .map(|value| value.grant_id.as_str()),
        Some("grant_123")
    );
    assert_eq!(
        receipt
            .execution_mode
            .as_ref()
            .and_then(|value| value.mode.as_deref()),
        Some("mode_c_protected_execution")
    );
    assert_eq!(
        receipt
            .execution_mode
            .as_ref()
            .and_then(|value| value.owner.as_deref()),
        Some("azums_protected_execution")
    );
    assert_eq!(
        receipt
            .execution_mode
            .as_ref()
            .and_then(|value| value.effective_policy.as_deref()),
        Some("sponsored")
    );
    assert_eq!(
        receipt
            .connector_outcome
            .as_ref()
            .map(|value| value.status.as_str()),
        Some("not_used")
    );
    assert_eq!(
        receipt
            .recon_linkage
            .as_ref()
            .and_then(|value| value.recon_subject_id.as_deref()),
        receipt.recon_subject_id.as_deref()
    );
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
