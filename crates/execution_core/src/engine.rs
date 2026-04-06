use crate::error::{CoreError, CoreResult};
use crate::model::{
    is_terminal_state, latest_receipt_version, recon_subject_id_for_job, AdapterExecutionRequest,
    AdapterOutcome, CallbackId, CallbackJob, CanonicalState, ExecutionJob, FailureInfo,
    IdempotencyBinding, LeasedJob, NormalizedIntent, PlatformClassification, ReceiptAgentAction,
    ReceiptAgentIdentity, ReceiptApprovalResult, ReceiptConnectorOutcome, ReceiptEntry,
    ReceiptExecutionMode, ReceiptGrantReference, ReceiptPolicyDecision, ReceiptReconLinkage,
    ReceiptRuntimeIdentity, ReconIntakeSignal, ReconIntakeSignalId, ReconIntakeSignalKind,
    ReplayCommand, ReplayDecisionId, ReplayDecisionRecord, StateTransition, StatusSummary,
    TransitionActor, TransitionId,
};
use crate::policy::{
    classify_adapter_outcome, transition_allowed, ReplayPolicy, RetryDecision, RetryPolicy,
};
use crate::ports::{AdapterExecutionError, AdapterRouter, Authorizer, Clock, DurableStore};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const EXISTING_SUBMIT_RESULT_POLL_ATTEMPTS: usize = 50;
const EXISTING_SUBMIT_RESULT_POLL_DELAY_MS: u64 = 10;

#[derive(Debug, Clone, Default)]
struct ReceiptContext {
    request_id: Option<String>,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    intent_kind: Option<String>,
    payload_hash: Option<String>,
    adapter_execution_reference: Option<String>,
    external_observation_key: Option<String>,
    reconciliation_eligible: bool,
    agent_action: Option<ReceiptAgentAction>,
    agent_identity: Option<ReceiptAgentIdentity>,
    runtime_identity: Option<ReceiptRuntimeIdentity>,
    policy_decision: Option<ReceiptPolicyDecision>,
    approval_result: Option<ReceiptApprovalResult>,
    grant_reference: Option<ReceiptGrantReference>,
    execution_mode: Option<ReceiptExecutionMode>,
    connector_outcome: Option<ReceiptConnectorOutcome>,
}

impl ReceiptContext {
    fn from_intent(intent: &NormalizedIntent) -> Self {
        Self::from_sources(
            intent.request_id.as_ref().map(ToString::to_string),
            intent.correlation_id.clone(),
            intent.idempotency_key.clone(),
            Some(intent.kind.to_string()),
            &intent.payload,
            intent.auth_context.as_ref(),
            &intent.metadata,
        )
    }

    fn from_request(request: &AdapterExecutionRequest) -> Self {
        Self::from_sources(
            request.request_id.as_ref().map(ToString::to_string),
            request.correlation_id.clone(),
            request.idempotency_key.clone(),
            Some(request.intent_kind.to_string()),
            &request.payload,
            request.auth_context.as_ref(),
            &request.metadata,
        )
    }

    fn from_sources(
        request_id: Option<String>,
        correlation_id: Option<String>,
        idempotency_key: Option<String>,
        intent_kind: Option<String>,
        payload: &Value,
        auth_context: Option<&crate::model::AuthContext>,
        metadata: &BTreeMap<String, String>,
    ) -> Self {
        let agent_origin = auth_context.and_then(|ctx| ctx.agent_id.as_ref()).is_some()
            || metadata
                .keys()
                .any(|key| key.starts_with("agent.") || key.starts_with("policy."));
        let grant_reference = build_receipt_grant_reference(metadata);
        Self {
            request_id,
            correlation_id,
            idempotency_key,
            intent_kind,
            payload_hash: Some(payload_hash(payload)),
            adapter_execution_reference: None,
            external_observation_key: None,
            reconciliation_eligible: false,
            agent_action: build_receipt_agent_action(metadata),
            agent_identity: build_receipt_agent_identity(auth_context, metadata),
            runtime_identity: build_receipt_runtime_identity(auth_context),
            policy_decision: build_receipt_policy_decision(metadata),
            approval_result: build_receipt_approval_result(
                metadata,
                grant_reference.as_ref(),
                agent_origin,
            ),
            grant_reference,
            execution_mode: build_receipt_execution_mode(metadata),
            connector_outcome: build_receipt_connector_outcome(metadata, agent_origin),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubmitResult {
    pub job: ExecutionJob,
    pub route_rule: String,
}

#[derive(Debug, Clone)]
pub struct DispatchResult {
    pub job: ExecutionJob,
    pub classification: PlatformClassification,
}

#[derive(Debug, Clone)]
pub struct ReplayResult {
    pub source_job_id: crate::model::JobId,
    pub replay_job: ExecutionJob,
}

pub struct ExecutionCore {
    store: Arc<dyn DurableStore>,
    router: Arc<dyn AdapterRouter>,
    authorizer: Arc<dyn Authorizer>,
    retry_policy: RetryPolicy,
    replay_policy: ReplayPolicy,
    clock: Arc<dyn Clock>,
}

impl ExecutionCore {
    pub fn new(
        store: Arc<dyn DurableStore>,
        router: Arc<dyn AdapterRouter>,
        authorizer: Arc<dyn Authorizer>,
        retry_policy: RetryPolicy,
        replay_policy: ReplayPolicy,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            store,
            router,
            authorizer,
            retry_policy,
            replay_policy,
            clock,
        }
    }

    pub async fn submit_intent(&self, intent: NormalizedIntent) -> CoreResult<SubmitResult> {
        let mut intent = intent;
        let idempotency_key = normalize_idempotency_key(intent.idempotency_key.take());
        intent.idempotency_key = idempotency_key.clone();
        let request_fingerprint = idempotency_fingerprint(&intent);

        if let Some(idempotency_key) = idempotency_key.as_ref() {
            if let Some(existing_binding) = self
                .store
                .lookup_intent_by_idempotency(&intent.tenant_id, idempotency_key)
                .await?
            {
                ensure_idempotency_binding_matches(
                    &existing_binding,
                    &request_fingerprint,
                    idempotency_key,
                )?;
                return self
                    .existing_submit_result(
                        &intent.tenant_id,
                        &existing_binding.intent_id,
                        idempotency_key,
                    )
                    .await;
            }
        }

        if !self.router.supported_intent(&intent.kind) {
            return Err(CoreError::UnsupportedIntent(intent.kind.clone()));
        }

        let route = self.router.resolve_adapter(&intent)?;
        if !self
            .authorizer
            .can_route_adapter(&intent.tenant_id, &route.adapter_id)
        {
            return Err(CoreError::AdapterRoutingDenied {
                tenant_id: intent.tenant_id,
                adapter_id: route.adapter_id,
            });
        }

        if let Some(idempotency_key) = idempotency_key.as_ref() {
            let bound_binding = self
                .store
                .bind_intent_idempotency(
                    &intent.tenant_id,
                    idempotency_key,
                    &intent.intent_id,
                    &request_fingerprint,
                )
                .await?;
            ensure_idempotency_binding_matches(
                &bound_binding,
                &request_fingerprint,
                idempotency_key,
            )?;
            if bound_binding.intent_id != intent.intent_id {
                return self
                    .existing_submit_result(
                        &intent.tenant_id,
                        &bound_binding.intent_id,
                        idempotency_key,
                    )
                    .await;
            }
        }

        let now_ms = self.clock.now_ms();
        let mut job = ExecutionJob::new(
            intent.tenant_id.clone(),
            intent.intent_id.clone(),
            route.adapter_id.clone(),
            self.retry_policy.max_attempts,
            now_ms,
        );
        let receipt_context = ReceiptContext::from_intent(&intent);

        let (received_transition, received_receipt) =
            self.build_received_transition_and_receipt(&job, &receipt_context)?;
        let (validated_transition, validated_receipt) = self.apply_transition(
            &mut job,
            CanonicalState::Validated,
            PlatformClassification::Success,
            "intent_validated",
            "intent passed core validation".to_owned(),
            TransitionActor::System,
            0,
            &receipt_context,
        )?;
        let (queued_transition, queued_receipt) = self.apply_transition(
            &mut job,
            CanonicalState::Queued,
            PlatformClassification::Success,
            "adapter_routed",
            format!("queued via route `{}`", route.rule),
            TransitionActor::System,
            0,
            &receipt_context,
        )?;

        self.store
            .persist_submission(
                &intent,
                &job,
                &[received_transition, validated_transition, queued_transition],
                &[received_receipt, validated_receipt, queued_receipt],
                None,
            )
            .await?;

        Ok(SubmitResult {
            job,
            route_rule: route.rule,
        })
    }

    pub async fn dispatch_job(&self, lease: LeasedJob) -> CoreResult<DispatchResult> {
        let mut job = lease.job;
        let current_attempt = job.attempt.saturating_add(1);
        if matches!(job.state, CanonicalState::RetryScheduled) {
            self.transition_job(
                &mut job,
                CanonicalState::Queued,
                PlatformClassification::RetryableFailure,
                "retry_due",
                "retry delay elapsed; job returned to queue".to_owned(),
                TransitionActor::System,
                current_attempt,
                &ReceiptContext::default(),
            )
            .await?;
        }
        self.transition_job(
            &mut job,
            CanonicalState::Leased,
            PlatformClassification::Success,
            "job_leased",
            "job lease acquired by execution worker".to_owned(),
            TransitionActor::System,
            current_attempt,
            &ReceiptContext::default(),
        )
        .await?;
        self.transition_job(
            &mut job,
            CanonicalState::Executing,
            PlatformClassification::Success,
            "dispatch_started",
            "dispatching to adapter".to_owned(),
            TransitionActor::System,
            current_attempt,
            &ReceiptContext::default(),
        )
        .await?;

        let intent = self
            .store
            .get_intent(&job.tenant_id, &job.intent_id)
            .await?
            .ok_or_else(|| CoreError::IntentNotFound(job.intent_id.to_string()))?;

        let request = AdapterExecutionRequest {
            request_id: intent.request_id,
            tenant_id: job.tenant_id.clone(),
            intent_id: job.intent_id.clone(),
            job_id: job.job_id.clone(),
            adapter_id: job.adapter_id.clone(),
            attempt: current_attempt,
            intent_kind: intent.kind,
            payload: intent.payload,
            correlation_id: intent.correlation_id,
            idempotency_key: intent.idempotency_key,
            auth_context: intent.auth_context,
            metadata: intent.metadata,
        };
        let base_receipt_context = ReceiptContext::from_request(&request);

        let adapter = self.router.adapter_executor(&job.adapter_id)?;
        let outcome = match adapter.execute(&request).await {
            Ok(outcome) => outcome,
            Err(err) => self.map_adapter_error_to_outcome(err),
        };

        self.handle_adapter_result(job, outcome, current_attempt, &base_receipt_context)
            .await
    }

    async fn handle_adapter_result(
        &self,
        mut job: ExecutionJob,
        outcome: AdapterOutcome,
        current_attempt: u32,
        base_receipt_context: &ReceiptContext,
    ) -> CoreResult<DispatchResult> {
        let outcome_receipt_context = receipt_context_for_outcome(base_receipt_context, &outcome);
        if let AdapterOutcome::InProgress { poll_after_ms, .. } = outcome {
            return self
                .handle_in_progress(
                    &mut job,
                    poll_after_ms,
                    current_attempt,
                    &outcome_receipt_context,
                )
                .await;
        }

        job.attempt = job.attempt.saturating_add(1);
        job.updated_at_ms = self.clock.now_ms();
        self.store.update_job(&job).await?;

        let (classification, failure) = classify_adapter_outcome(&outcome);
        match classification {
            PlatformClassification::Success => {
                job.last_failure = None;
                job.next_retry_at_ms = None;
                let actor = TransitionActor::Adapter(job.adapter_id.clone());
                self.transition_job(
                    &mut job,
                    CanonicalState::Succeeded,
                    PlatformClassification::Success,
                    "adapter_succeeded",
                    "adapter execution succeeded".to_owned(),
                    actor,
                    current_attempt,
                    &outcome_receipt_context,
                )
                .await?;
                self.enqueue_terminal_callback_if_needed(&job).await?;
                Ok(DispatchResult {
                    job,
                    classification,
                })
            }
            PlatformClassification::RetryableFailure => {
                let failure =
                    failure.expect("retryable classification always includes failure info");
                self.schedule_retry(&mut job, failure, current_attempt, &outcome_receipt_context)
                    .await
            }
            PlatformClassification::TerminalFailure => {
                let failure =
                    failure.expect("terminal classification always includes failure info");
                self.mark_terminal_failure(
                    &mut job,
                    failure,
                    current_attempt,
                    &outcome_receipt_context,
                )
                .await
            }
            PlatformClassification::Blocked => {
                let failure = failure.expect("blocked classification always includes failure info");
                self.mark_terminal_failure(
                    &mut job,
                    failure,
                    current_attempt,
                    &outcome_receipt_context,
                )
                .await
            }
            PlatformClassification::ManualReview => {
                let failure =
                    failure.expect("manual review classification always includes failure info");
                self.mark_terminal_failure(
                    &mut job,
                    failure,
                    current_attempt,
                    &outcome_receipt_context,
                )
                .await
            }
        }
    }

    async fn handle_in_progress(
        &self,
        job: &mut ExecutionJob,
        poll_after_ms: Option<u64>,
        current_attempt: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<DispatchResult> {
        let now_ms = self.clock.now_ms();
        let delay_ms = poll_after_ms.unwrap_or(1_000).max(1);
        let run_at_ms = now_ms.saturating_add(delay_ms);
        job.last_failure = None;
        job.next_retry_at_ms = Some(run_at_ms);

        self.transition_job(
            job,
            CanonicalState::RetryScheduled,
            PlatformClassification::Success,
            "adapter_in_progress",
            format!(
                "adapter reported in-progress; polling again in {} ms",
                delay_ms
            ),
            TransitionActor::Adapter(job.adapter_id.clone()),
            current_attempt,
            receipt_context,
        )
        .await?;
        self.store
            .enqueue_dispatch(&job.job_id, Some(run_at_ms))
            .await?;

        Ok(DispatchResult {
            job: job.clone(),
            classification: PlatformClassification::Success,
        })
    }

    async fn schedule_retry(
        &self,
        job: &mut ExecutionJob,
        failure: FailureInfo,
        current_attempt: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<DispatchResult> {
        let now_ms = self.clock.now_ms();
        let decision = self
            .retry_policy
            .decide(now_ms, job.attempt, failure.retry_after_ms);

        match decision {
            RetryDecision::RetryAt {
                run_at_ms,
                delay_ms,
                ..
            } => {
                job.last_failure = Some(failure);
                job.next_retry_at_ms = Some(run_at_ms);
                self.transition_job(
                    job,
                    CanonicalState::RetryScheduled,
                    PlatformClassification::RetryableFailure,
                    "retry_scheduled",
                    format!("retry scheduled in {} ms", delay_ms),
                    TransitionActor::System,
                    current_attempt,
                    receipt_context,
                )
                .await?;
                self.store
                    .enqueue_dispatch(&job.job_id, Some(run_at_ms))
                    .await?;

                Ok(DispatchResult {
                    job: job.clone(),
                    classification: PlatformClassification::RetryableFailure,
                })
            }
            RetryDecision::Exhausted => {
                let exhausted_failure = FailureInfo {
                    code: "retry_exhausted".to_owned(),
                    message: "retry budget exhausted".to_owned(),
                    classification: PlatformClassification::TerminalFailure,
                    caller_can_fix: true,
                    operator_can_fix: true,
                    retry_after_ms: None,
                    provider_details: failure.provider_details,
                };
                self.mark_dead_lettered(job, exhausted_failure, current_attempt, receipt_context)
                    .await
            }
        }
    }

    async fn mark_terminal_failure(
        &self,
        job: &mut ExecutionJob,
        failure: FailureInfo,
        current_attempt: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<DispatchResult> {
        let classification = match failure.classification {
            PlatformClassification::RetryableFailure => PlatformClassification::TerminalFailure,
            value => value,
        };
        job.last_failure = Some(failure.clone());
        job.next_retry_at_ms = None;
        let actor = TransitionActor::Adapter(job.adapter_id.clone());
        self.transition_job(
            job,
            CanonicalState::FailedTerminal,
            classification,
            &failure.code,
            failure.message,
            actor,
            current_attempt,
            receipt_context,
        )
        .await?;
        self.enqueue_terminal_callback_if_needed(job).await?;

        Ok(DispatchResult {
            job: job.clone(),
            classification,
        })
    }

    async fn mark_dead_lettered(
        &self,
        job: &mut ExecutionJob,
        failure: FailureInfo,
        current_attempt: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<DispatchResult> {
        job.last_failure = Some(failure.clone());
        job.next_retry_at_ms = None;
        self.transition_job(
            job,
            CanonicalState::DeadLettered,
            PlatformClassification::TerminalFailure,
            "dead_lettered",
            failure.message,
            TransitionActor::System,
            current_attempt,
            receipt_context,
        )
        .await?;
        self.enqueue_terminal_callback_if_needed(job).await?;

        Ok(DispatchResult {
            job: job.clone(),
            classification: PlatformClassification::TerminalFailure,
        })
    }

    pub async fn emit_receipt(&self, receipt: ReceiptEntry) -> CoreResult<()> {
        self.emit_receipt_bundle(receipt, Vec::new()).await?;
        Ok(())
    }

    async fn emit_receipt_bundle(
        &self,
        receipt: ReceiptEntry,
        signals: Vec<ReconIntakeSignal>,
    ) -> CoreResult<()> {
        self.store.append_receipt_bundle(&receipt, &signals).await?;
        Ok(())
    }

    pub async fn request_replay(&self, command: ReplayCommand) -> CoreResult<ReplayResult> {
        if !self
            .authorizer
            .can_replay(&command.requested_by, &command.tenant_id)
        {
            return Err(CoreError::UnauthorizedReplay {
                principal_id: command.requested_by.principal_id,
            });
        }

        let source_job = self
            .store
            .get_latest_job_for_intent(&command.tenant_id, &command.intent_id)
            .await?
            .ok_or_else(|| CoreError::IntentNotFound(command.intent_id.to_string()))?;

        if source_job.tenant_id != command.tenant_id {
            return Err(CoreError::TenantMismatch {
                job_id: source_job.job_id,
                expected: source_job.tenant_id,
                actual: command.tenant_id,
            });
        }

        if !self
            .replay_policy
            .can_replay(source_job.state, source_job.replay_count)
        {
            self.record_replay_decision(&source_job, &command, false, "state not replayable")
                .await?;
            return Err(CoreError::ReplayDenied {
                reason: "state not replayable or replay budget exhausted".to_owned(),
            });
        }

        if !self
            .authorizer
            .can_trigger_manual_action(&command.requested_by, &command.tenant_id)
        {
            self.record_replay_decision(&source_job, &command, false, "operator not allowed")
                .await?;
            return Err(CoreError::UnauthorizedManualAction {
                principal_id: command.requested_by.principal_id,
            });
        }

        self.record_replay_decision(&source_job, &command, true, "operator replay approved")
            .await?;

        let now_ms = self.clock.now_ms();
        let intent = self
            .store
            .get_intent(&source_job.tenant_id, &source_job.intent_id)
            .await?
            .ok_or_else(|| CoreError::IntentNotFound(source_job.intent_id.to_string()))?;
        let receipt_context = ReceiptContext::from_intent(&intent);
        let mut replay_job = ExecutionJob::new(
            source_job.tenant_id.clone(),
            source_job.intent_id.clone(),
            source_job.adapter_id.clone(),
            source_job.max_attempts,
            now_ms,
        );
        replay_job.replay_count = source_job.replay_count.saturating_add(1);
        replay_job.replay_of_job_id = Some(source_job.job_id.clone());

        self.store.persist_job(&replay_job).await?;
        self.bootstrap_received_transition(&replay_job, &receipt_context)
            .await?;
        self.transition_job(
            &mut replay_job,
            CanonicalState::Validated,
            PlatformClassification::Success,
            "replay_validated",
            "replay request validated".to_owned(),
            TransitionActor::System,
            0,
            &receipt_context,
        )
        .await?;
        self.transition_job(
            &mut replay_job,
            CanonicalState::Replayed,
            PlatformClassification::Success,
            "replay_started",
            format!("replay requested: {}", command.reason),
            TransitionActor::Operator(command.requested_by.principal_id),
            0,
            &receipt_context,
        )
        .await?;
        self.transition_job(
            &mut replay_job,
            CanonicalState::Queued,
            PlatformClassification::Success,
            "replay_queued",
            "replay queued for execution".to_owned(),
            TransitionActor::System,
            0,
            &receipt_context,
        )
        .await?;
        self.store
            .enqueue_dispatch(&replay_job.job_id, None)
            .await?;

        Ok(ReplayResult {
            source_job_id: source_job.job_id,
            replay_job,
        })
    }

    async fn bootstrap_received_transition(
        &self,
        job: &ExecutionJob,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<()> {
        let (transition, receipt) =
            self.build_received_transition_and_receipt(job, receipt_context)?;
        self.store.record_transition(&transition).await?;
        self.emit_receipt(receipt).await
    }

    async fn transition_job(
        &self,
        job: &mut ExecutionJob,
        to_state: CanonicalState,
        classification: PlatformClassification,
        reason_code: &str,
        reason: String,
        actor: TransitionActor,
        attempt_no: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<()> {
        let (transition, receipt) = self.apply_transition(
            job,
            to_state,
            classification,
            reason_code,
            reason,
            actor,
            attempt_no,
            receipt_context,
        )?;
        self.store.record_transition(&transition).await?;
        self.store.update_job(job).await?;
        let signals = self.recon_signals_for_receipt(&receipt, &transition);
        self.emit_receipt_bundle(receipt, signals).await?;
        Ok(())
    }

    fn recon_signals_for_receipt(
        &self,
        receipt: &ReceiptEntry,
        transition: &StateTransition,
    ) -> Vec<ReconIntakeSignal> {
        let mut kinds = Vec::new();
        let has_reference = receipt.adapter_execution_reference.is_some();
        match (receipt.state, receipt.classification) {
            (CanonicalState::RetryScheduled, PlatformClassification::Success) if has_reference => {
                kinds.push(ReconIntakeSignalKind::SubmittedWithReference);
            }
            (CanonicalState::RetryScheduled, PlatformClassification::RetryableFailure) => {
                kinds.push(ReconIntakeSignalKind::AdapterCompleted);
                if has_reference {
                    kinds.push(ReconIntakeSignalKind::SubmittedWithReference);
                }
            }
            (CanonicalState::Succeeded, _) => {
                kinds.push(ReconIntakeSignalKind::AdapterCompleted);
                kinds.push(ReconIntakeSignalKind::Finalized);
                if has_reference {
                    kinds.push(ReconIntakeSignalKind::SubmittedWithReference);
                }
            }
            (CanonicalState::FailedTerminal, _)
            | (CanonicalState::DeadLettered, _)
            | (CanonicalState::Rejected, _) => {
                kinds.push(ReconIntakeSignalKind::AdapterCompleted);
                kinds.push(ReconIntakeSignalKind::TerminalFailure);
                if has_reference {
                    kinds.push(ReconIntakeSignalKind::SubmittedWithReference);
                }
            }
            _ => {}
        }

        kinds
            .into_iter()
            .map(|kind| ReconIntakeSignal {
                signal_id: ReconIntakeSignalId::new(),
                source_system: "execution_core".to_owned(),
                signal_kind: kind,
                tenant_id: receipt.tenant_id.clone(),
                intent_id: receipt.intent_id.clone(),
                job_id: receipt.job_id.clone(),
                adapter_id: transition.adapter_id.clone(),
                receipt_id: Some(receipt.receipt_id.clone()),
                transition_id: Some(transition.transition_id.clone()),
                callback_id: None,
                recon_subject_id: receipt
                    .recon_subject_id
                    .clone()
                    .unwrap_or_else(|| recon_subject_id_for_job(&receipt.job_id)),
                canonical_state: Some(receipt.state),
                classification: Some(receipt.classification),
                execution_correlation_id: receipt.execution_correlation_id.clone(),
                adapter_execution_reference: receipt.adapter_execution_reference.clone(),
                external_observation_key: receipt.external_observation_key.clone(),
                expected_fact_snapshot: receipt.expected_fact_snapshot.clone(),
                payload: serde_json::json!({
                    "summary": receipt.summary,
                    "details": receipt.details,
                    "attempt_no": receipt.attempt_no,
                    "reason_code": transition.reason_code,
                }),
                occurred_at_ms: receipt.occurred_at_ms,
            })
            .collect()
    }

    fn build_received_transition_and_receipt(
        &self,
        job: &ExecutionJob,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<(StateTransition, ReceiptEntry)> {
        if !transition_allowed(None, CanonicalState::Received) {
            return Err(CoreError::IllegalTransition {
                from: None,
                to: CanonicalState::Received,
            });
        }

        let now_ms = self.clock.now_ms();
        let transition = StateTransition {
            transition_id: TransitionId::new(),
            tenant_id: job.tenant_id.clone(),
            intent_id: job.intent_id.clone(),
            job_id: job.job_id.clone(),
            from_state: None,
            to_state: CanonicalState::Received,
            classification: PlatformClassification::Success,
            reason_code: "request_received".to_owned(),
            reason: "request received by execution core".to_owned(),
            adapter_id: Some(job.adapter_id.clone()),
            actor: TransitionActor::System,
            occurred_at_ms: now_ms,
        };

        let receipt = ReceiptEntry {
            receipt_id: crate::model::ReceiptId::new(),
            tenant_id: transition.tenant_id.clone(),
            intent_id: transition.intent_id.clone(),
            job_id: transition.job_id.clone(),
            receipt_version: latest_receipt_version(),
            recon_subject_id: Some(recon_subject_id_for_job(&job.job_id)),
            reconciliation_eligible: false,
            execution_correlation_id: receipt_context.correlation_id.clone(),
            adapter_execution_reference: None,
            external_observation_key: None,
            expected_fact_snapshot: build_expected_fact_snapshot(
                job,
                transition.to_state,
                transition.classification,
                receipt_context,
            ),
            agent_action: receipt_context.agent_action.clone(),
            agent_identity: receipt_context.agent_identity.clone(),
            runtime_identity: receipt_context.runtime_identity.clone(),
            policy_decision: receipt_context.policy_decision.clone(),
            approval_result: receipt_context.approval_result.clone(),
            grant_reference: receipt_context.grant_reference.clone(),
            execution_mode: receipt_context.execution_mode.clone(),
            connector_outcome: receipt_context.connector_outcome.clone(),
            recon_linkage: Some(build_receipt_recon_linkage(job, receipt_context, false)),
            attempt_no: 0,
            state: transition.to_state,
            classification: transition.classification,
            summary: transition.reason.clone(),
            details: {
                let mut details = BTreeMap::from([
                    ("reason_code".to_owned(), transition.reason_code.clone()),
                    ("actor".to_owned(), "system".to_owned()),
                    ("attempt_no".to_owned(), "0".to_owned()),
                ]);
                if let Some(source_job_id) = job.replay_of_job_id.as_ref() {
                    details.insert("replay_of_job_id".to_owned(), source_job_id.to_string());
                }
                details
            },
            occurred_at_ms: now_ms,
        };

        Ok((transition, receipt))
    }

    fn apply_transition(
        &self,
        job: &mut ExecutionJob,
        to_state: CanonicalState,
        classification: PlatformClassification,
        reason_code: &str,
        reason: String,
        actor: TransitionActor,
        attempt_no: u32,
        receipt_context: &ReceiptContext,
    ) -> CoreResult<(StateTransition, ReceiptEntry)> {
        let from_state = Some(job.state);
        if !transition_allowed(from_state, to_state) {
            return Err(CoreError::IllegalTransition {
                from: from_state,
                to: to_state,
            });
        }

        let now_ms = self.clock.now_ms();
        let transition = StateTransition {
            transition_id: TransitionId::new(),
            tenant_id: job.tenant_id.clone(),
            intent_id: job.intent_id.clone(),
            job_id: job.job_id.clone(),
            from_state,
            to_state,
            classification,
            reason_code: reason_code.to_owned(),
            reason: reason.clone(),
            adapter_id: Some(job.adapter_id.clone()),
            actor: actor.clone(),
            occurred_at_ms: now_ms,
        };

        job.state = to_state;
        job.updated_at_ms = now_ms;

        let actor_detail = match actor {
            TransitionActor::System => "system".to_owned(),
            TransitionActor::Adapter(adapter_id) => format!("adapter:{adapter_id}"),
            TransitionActor::Operator(principal_id) => format!("operator:{principal_id}"),
        };
        let mut details = BTreeMap::new();
        details.insert("reason_code".to_owned(), reason_code.to_owned());
        details.insert("actor".to_owned(), actor_detail);
        if let Some(source_job_id) = job.replay_of_job_id.as_ref() {
            details.insert("replay_of_job_id".to_owned(), source_job_id.to_string());
        }
        if classification != PlatformClassification::Success {
            if let Some(failure) = job.last_failure.as_ref() {
                details.insert("failure_code".to_owned(), failure.code.clone());
                details.insert(
                    "failure_classification".to_owned(),
                    format!("{:?}", failure.classification),
                );
                details.insert(
                    "caller_can_fix".to_owned(),
                    failure.caller_can_fix.to_string(),
                );
                details.insert(
                    "operator_can_fix".to_owned(),
                    failure.operator_can_fix.to_string(),
                );
            }
        }

        let reconciliation_eligible = receipt_context.reconciliation_eligible
            || matches!(
                to_state,
                CanonicalState::Succeeded
                    | CanonicalState::FailedTerminal
                    | CanonicalState::DeadLettered
                    | CanonicalState::Rejected
            );
        let receipt = ReceiptEntry {
            receipt_id: crate::model::ReceiptId::new(),
            tenant_id: job.tenant_id.clone(),
            intent_id: job.intent_id.clone(),
            job_id: job.job_id.clone(),
            receipt_version: latest_receipt_version(),
            recon_subject_id: Some(recon_subject_id_for_job(&job.job_id)),
            reconciliation_eligible,
            execution_correlation_id: receipt_context.correlation_id.clone(),
            adapter_execution_reference: receipt_context.adapter_execution_reference.clone(),
            external_observation_key: receipt_context.external_observation_key.clone(),
            expected_fact_snapshot: build_expected_fact_snapshot(
                job,
                to_state,
                classification,
                receipt_context,
            ),
            agent_action: receipt_context.agent_action.clone(),
            agent_identity: receipt_context.agent_identity.clone(),
            runtime_identity: receipt_context.runtime_identity.clone(),
            policy_decision: receipt_context.policy_decision.clone(),
            approval_result: receipt_context.approval_result.clone(),
            grant_reference: receipt_context.grant_reference.clone(),
            execution_mode: receipt_context.execution_mode.clone(),
            connector_outcome: receipt_context.connector_outcome.clone(),
            recon_linkage: Some(build_receipt_recon_linkage(
                job,
                receipt_context,
                reconciliation_eligible,
            )),
            attempt_no,
            state: to_state,
            classification,
            summary: reason,
            details: {
                let mut details = details;
                details.insert("attempt_no".to_owned(), attempt_no.to_string());
                details
            },
            occurred_at_ms: now_ms,
        };

        Ok((transition, receipt))
    }

    async fn enqueue_terminal_callback_if_needed(&self, job: &ExecutionJob) -> CoreResult<()> {
        if !is_terminal_state(job.state) {
            return Ok(());
        }

        let classification = match job.state {
            CanonicalState::Succeeded => PlatformClassification::Success,
            CanonicalState::FailedTerminal => job
                .last_failure
                .as_ref()
                .map(|failure| failure.classification)
                .unwrap_or(PlatformClassification::TerminalFailure),
            CanonicalState::DeadLettered => PlatformClassification::TerminalFailure,
            _ => return Ok(()),
        };

        let callback = CallbackJob {
            callback_id: CallbackId::new(),
            summary: StatusSummary {
                tenant_id: job.tenant_id.clone(),
                intent_id: job.intent_id.clone(),
                job_id: job.job_id.clone(),
                adapter_id: job.adapter_id.clone(),
                state: job.state,
                classification,
                updated_at_ms: self.clock.now_ms(),
            },
            enqueued_at_ms: self.clock.now_ms(),
        };
        self.store.enqueue_callback_job(&callback).await?;
        Ok(())
    }

    async fn record_replay_decision(
        &self,
        source_job: &ExecutionJob,
        command: &ReplayCommand,
        allowed: bool,
        reason: &str,
    ) -> CoreResult<()> {
        let decision = ReplayDecisionRecord {
            replay_decision_id: ReplayDecisionId::new(),
            tenant_id: source_job.tenant_id.clone(),
            intent_id: source_job.intent_id.clone(),
            source_job_id: source_job.job_id.clone(),
            allowed,
            reason: reason.to_owned(),
            requested_by: command.requested_by.principal_id.clone(),
            occurred_at_ms: self.clock.now_ms(),
        };
        self.store.record_replay_decision(&decision).await?;
        Ok(())
    }

    fn map_adapter_error_to_outcome(&self, err: AdapterExecutionError) -> AdapterOutcome {
        match err {
            AdapterExecutionError::Unavailable(message)
            | AdapterExecutionError::Timeout(message)
            | AdapterExecutionError::Transport(message) => AdapterOutcome::RetryableFailure {
                code: "adapter_unavailable".to_owned(),
                message,
                retry_after_ms: None,
                provider_details: None,
            },
            AdapterExecutionError::ContractViolation(message) => AdapterOutcome::ManualReview {
                code: "adapter_contract_violation".to_owned(),
                message,
            },
            AdapterExecutionError::UnsupportedIntent(message) => AdapterOutcome::Blocked {
                code: "adapter_unsupported_intent".to_owned(),
                message,
            },
            AdapterExecutionError::Unauthorized(message) => AdapterOutcome::Blocked {
                code: "adapter_unauthorized".to_owned(),
                message,
            },
        }
    }

    async fn existing_submit_result(
        &self,
        tenant_id: &crate::model::TenantId,
        intent_id: &crate::model::IntentId,
        idempotency_key: &str,
    ) -> CoreResult<SubmitResult> {
        let mut existing_job = self
            .store
            .get_latest_job_for_intent(tenant_id, intent_id)
            .await?;
        if existing_job.is_none() {
            for _ in 0..EXISTING_SUBMIT_RESULT_POLL_ATTEMPTS {
                std::thread::sleep(Duration::from_millis(EXISTING_SUBMIT_RESULT_POLL_DELAY_MS));
                existing_job = self
                    .store
                    .get_latest_job_for_intent(tenant_id, intent_id)
                    .await?;
                if existing_job.is_some() {
                    break;
                }
            }
        }

        let existing_job = existing_job.ok_or_else(|| CoreError::IdempotencyConflict {
            key: idempotency_key.to_owned(),
            reason: format!(
                "idempotency key is bound to intent `{intent_id}` but no execution job was found"
            ),
        })?;

        Ok(SubmitResult {
            job: existing_job,
            route_rule: format!("idempotency_reuse:{idempotency_key}"),
        })
    }
}

fn build_receipt_agent_action(metadata: &BTreeMap<String, String>) -> Option<ReceiptAgentAction> {
    let action_request_id = metadata.get("agent.action_request_id").cloned();
    let intent_type = metadata.get("agent.intent_type").cloned();
    let adapter_type = metadata.get("agent.adapter_type").cloned();
    let requested_scope = csv_metadata(metadata, "agent.requested_scope");
    let effective_scope = csv_metadata(metadata, "agent.effective_scope");
    let reason = metadata.get("agent.reason").cloned();
    let submitted_by = metadata.get("agent.submitted_by").cloned();

    if action_request_id.is_none()
        && intent_type.is_none()
        && adapter_type.is_none()
        && requested_scope.is_empty()
        && effective_scope.is_empty()
        && reason.is_none()
        && submitted_by.is_none()
    {
        return None;
    }

    Some(ReceiptAgentAction {
        action_request_id,
        intent_type,
        adapter_type,
        requested_scope,
        effective_scope,
        reason,
        submitted_by,
    })
}

fn build_receipt_agent_identity(
    auth_context: Option<&crate::model::AuthContext>,
    metadata: &BTreeMap<String, String>,
) -> Option<ReceiptAgentIdentity> {
    let agent_id = auth_context
        .and_then(|ctx| ctx.agent_id.clone())
        .or_else(|| metadata.get("agent.id").cloned());
    let environment_id = auth_context
        .and_then(|ctx| ctx.environment_id.clone())
        .or_else(|| metadata.get("agent.environment_id").cloned());
    let environment_kind = metadata.get("agent.environment_kind").cloned();
    let status = metadata.get("agent.status").cloned();
    let trust_tier = auth_context
        .and_then(|ctx| ctx.trust_tier.clone())
        .or_else(|| metadata.get("agent.trust_tier").cloned());
    let risk_tier = auth_context
        .and_then(|ctx| ctx.risk_tier.clone())
        .or_else(|| metadata.get("agent.risk_tier").cloned());
    let owner_team = metadata.get("agent.owner_team").cloned();

    if agent_id.is_none()
        && environment_id.is_none()
        && environment_kind.is_none()
        && status.is_none()
        && trust_tier.is_none()
        && risk_tier.is_none()
        && owner_team.is_none()
    {
        return None;
    }

    Some(ReceiptAgentIdentity {
        agent_id,
        environment_id,
        environment_kind,
        status,
        trust_tier,
        risk_tier,
        owner_team,
    })
}

fn build_receipt_runtime_identity(
    auth_context: Option<&crate::model::AuthContext>,
) -> Option<ReceiptRuntimeIdentity> {
    let Some(auth_context) = auth_context else {
        return None;
    };

    if auth_context.runtime_type.is_none()
        && auth_context.runtime_identity.is_none()
        && auth_context.submitter_kind.is_none()
        && auth_context.channel.is_none()
    {
        return None;
    }

    Some(ReceiptRuntimeIdentity {
        runtime_type: auth_context.runtime_type.clone(),
        runtime_identity: auth_context.runtime_identity.clone(),
        submitter_kind: auth_context.submitter_kind.clone(),
        channel: auth_context.channel.clone(),
    })
}

fn build_receipt_policy_decision(
    metadata: &BTreeMap<String, String>,
) -> Option<ReceiptPolicyDecision> {
    let decision = metadata.get("policy.decision").cloned();
    let explanation = metadata.get("policy.explanation").cloned();
    let bundle_id = metadata.get("policy.bundle_id").cloned();
    let bundle_version = parse_u64_metadata(metadata, "policy.bundle_version");

    if decision.is_none()
        && explanation.is_none()
        && bundle_id.is_none()
        && bundle_version.is_none()
    {
        return None;
    }

    Some(ReceiptPolicyDecision {
        decision,
        explanation,
        bundle_id,
        bundle_version,
    })
}

fn build_receipt_approval_result(
    metadata: &BTreeMap<String, String>,
    grant_reference: Option<&ReceiptGrantReference>,
    agent_origin: bool,
) -> Option<ReceiptApprovalResult> {
    let approval_request_id = metadata.get("approval.request_id").cloned();
    let state = metadata.get("approval.state").cloned();
    let required_approvals = parse_u32_metadata(metadata, "approval.required_approvals");
    let approvals_received = parse_u32_metadata(metadata, "approval.approvals_received");
    let approved_by = csv_metadata(metadata, "approval.approved_by");

    if !agent_origin
        && approval_request_id.is_none()
        && state.is_none()
        && grant_reference.is_none()
    {
        return None;
    }

    let result = if let Some(state) = state.as_ref() {
        state.clone()
    } else if grant_reference.is_some() {
        "satisfied_by_grant".to_owned()
    } else {
        "not_required".to_owned()
    };

    Some(ReceiptApprovalResult {
        result,
        approval_request_id,
        state,
        required_approvals,
        approvals_received,
        approved_by,
    })
}

fn build_receipt_grant_reference(
    metadata: &BTreeMap<String, String>,
) -> Option<ReceiptGrantReference> {
    let grant_id = metadata.get("grant.id").cloned()?;
    Some(ReceiptGrantReference {
        grant_id,
        source_action_request_id: metadata.get("grant.source_action_request_id").cloned(),
        source_approval_request_id: metadata.get("grant.source_approval_request_id").cloned(),
        source_policy_bundle_id: metadata.get("grant.source_policy_bundle_id").cloned(),
        source_policy_bundle_version: parse_u64_metadata(
            metadata,
            "grant.source_policy_bundle_version",
        ),
        expires_at_ms: parse_u64_metadata(metadata, "grant.expires_at_ms"),
    })
}

fn build_receipt_execution_mode(
    metadata: &BTreeMap<String, String>,
) -> Option<ReceiptExecutionMode> {
    let mode = metadata.get("execution.mode").cloned();
    let owner = metadata.get("execution.owner").cloned();
    let effective_policy = metadata.get("execution.policy").cloned();
    let base_policy = metadata.get("execution.policy.base").cloned();
    let signing_mode = metadata.get("execution.signing_mode").cloned();
    let payer_source = metadata.get("execution.payer_source").cloned();
    let fee_payer = metadata.get("execution.fee_payer").cloned();

    if mode.is_none()
        && owner.is_none()
        && effective_policy.is_none()
        && base_policy.is_none()
        && signing_mode.is_none()
        && payer_source.is_none()
        && fee_payer.is_none()
    {
        return None;
    }

    Some(ReceiptExecutionMode {
        mode,
        owner,
        effective_policy,
        base_policy,
        signing_mode,
        payer_source,
        fee_payer,
    })
}

fn build_receipt_connector_outcome(
    metadata: &BTreeMap<String, String>,
    agent_origin: bool,
) -> Option<ReceiptConnectorOutcome> {
    let status = metadata.get("connector.outcome").cloned();
    let connector_type = metadata.get("connector.type").cloned();
    let binding_id = metadata.get("connector.binding_id").cloned();
    let reference = metadata.get("connector.reference").cloned();

    if status.is_none() && connector_type.is_none() && binding_id.is_none() && reference.is_none() {
        if !agent_origin {
            return None;
        }
        return Some(ReceiptConnectorOutcome {
            status: "not_used".to_owned(),
            connector_type: None,
            binding_id: None,
            reference: None,
        });
    }

    Some(ReceiptConnectorOutcome {
        status: status.unwrap_or_else(|| "unknown".to_owned()),
        connector_type,
        binding_id,
        reference,
    })
}

fn build_receipt_recon_linkage(
    job: &ExecutionJob,
    receipt_context: &ReceiptContext,
    reconciliation_eligible: bool,
) -> ReceiptReconLinkage {
    let connector = receipt_context.connector_outcome.as_ref();
    let connector_reference = recon_downstream_reference(receipt_context);
    ReceiptReconLinkage {
        recon_subject_id: Some(recon_subject_id_for_job(&job.job_id)),
        reconciliation_eligible,
        execution_correlation_id: receipt_context.correlation_id.clone(),
        adapter_execution_reference: receipt_context.adapter_execution_reference.clone(),
        external_observation_key: receipt_context.external_observation_key.clone(),
        connector_type: connector.and_then(|value| value.connector_type.clone()),
        connector_binding_id: connector.and_then(|value| value.binding_id.clone()),
        connector_reference,
    }
}

fn csv_metadata(metadata: &BTreeMap<String, String>, key: &str) -> Vec<String> {
    metadata
        .get(key)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_u32_metadata(metadata: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    metadata.get(key)?.trim().parse().ok()
}

fn parse_u64_metadata(metadata: &BTreeMap<String, String>, key: &str) -> Option<u64> {
    metadata.get(key)?.trim().parse().ok()
}

fn payload_hash(payload: &Value) -> String {
    let canonical = serde_json::to_vec(&canonicalize_json_value(payload)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("{:x}", hasher.finalize())
}

fn receipt_context_for_outcome(base: &ReceiptContext, outcome: &AdapterOutcome) -> ReceiptContext {
    let mut context = base.clone();
    let adapter_execution_reference = adapter_execution_reference(outcome);
    let external_observation_key =
        external_observation_key(outcome, adapter_execution_reference.clone());
    context.adapter_execution_reference = adapter_execution_reference;
    context.external_observation_key = external_observation_key;
    context.reconciliation_eligible = context.adapter_execution_reference.is_some()
        || context.external_observation_key.is_some()
        || matches!(
            outcome,
            AdapterOutcome::Succeeded { .. }
                | AdapterOutcome::TerminalFailure { .. }
                | AdapterOutcome::Blocked { .. }
                | AdapterOutcome::ManualReview { .. }
        );
    context
}

fn adapter_execution_reference(outcome: &AdapterOutcome) -> Option<String> {
    match outcome {
        AdapterOutcome::InProgress {
            provider_reference, ..
        }
        | AdapterOutcome::Succeeded {
            provider_reference, ..
        } => provider_reference.clone(),
        _ => None,
    }
}

fn external_observation_key(
    outcome: &AdapterOutcome,
    adapter_execution_reference: Option<String>,
) -> Option<String> {
    let details = match outcome {
        AdapterOutcome::InProgress { details, .. } | AdapterOutcome::Succeeded { details, .. } => {
            Some(details)
        }
        _ => None,
    };
    details
        .and_then(|details| {
            details
                .get("signature")
                .cloned()
                .or_else(|| details.get("tx_hash").cloned())
                .or_else(|| details.get("attempt_id").cloned())
        })
        .or(adapter_execution_reference)
}

fn build_expected_fact_snapshot(
    job: &ExecutionJob,
    state: CanonicalState,
    classification: PlatformClassification,
    receipt_context: &ReceiptContext,
) -> Option<Value> {
    let reconciliation_eligible = receipt_context.reconciliation_eligible
        || matches!(
            state,
            CanonicalState::Succeeded
                | CanonicalState::FailedTerminal
                | CanonicalState::DeadLettered
                | CanonicalState::Rejected
        );
    let connector_reference = recon_downstream_reference(receipt_context);
    let connector_snapshot = receipt_context.connector_outcome.as_ref().map(|connector| {
        serde_json::json!({
            "status": connector.status.clone(),
            "connector_type": connector.connector_type.clone(),
            "binding_id": connector.binding_id.clone(),
            "reference": connector_reference,
        })
    }).or_else(|| {
        connector_reference.map(|reference| {
            serde_json::json!({
                "status": "external_reference_only",
                "connector_type": Value::Null,
                "binding_id": Value::Null,
                "reference": reference,
            })
        })
    });
    Some(serde_json::json!({
        "version": 2,
        "recon_subject_id": recon_subject_id_for_job(&job.job_id),
        "job_id": job.job_id,
        "tenant_id": job.tenant_id,
        "intent_id": job.intent_id,
        "adapter_id": job.adapter_id,
        "canonical_state": state,
        "classification": classification,
        "request_id": receipt_context.request_id,
        "correlation_id": receipt_context.correlation_id,
        "idempotency_key": receipt_context.idempotency_key,
        "intent_kind": receipt_context.intent_kind,
        "payload_hash": receipt_context.payload_hash,
        "adapter_execution_reference": receipt_context.adapter_execution_reference,
        "external_observation_key": receipt_context.external_observation_key,
        "connector": connector_snapshot,
        "reconciliation_eligible": reconciliation_eligible,
    }))
}

fn recon_downstream_reference(receipt_context: &ReceiptContext) -> Option<String> {
    receipt_context
        .connector_outcome
        .as_ref()
        .and_then(|value| value.reference.clone())
        .or_else(|| receipt_context.adapter_execution_reference.clone())
        .or_else(|| receipt_context.external_observation_key.clone())
}

fn normalize_idempotency_key(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

fn ensure_idempotency_binding_matches(
    binding: &IdempotencyBinding,
    request_fingerprint: &str,
    idempotency_key: &str,
) -> CoreResult<()> {
    if let Some(existing_fingerprint) = binding.request_fingerprint.as_deref() {
        if existing_fingerprint != request_fingerprint {
            return Err(CoreError::IdempotencyConflict {
                key: idempotency_key.to_owned(),
                reason: format!(
                    "key is already bound to intent `{}` with a different normalized request",
                    binding.intent_id
                ),
            });
        }
    }

    Ok(())
}

fn idempotency_fingerprint(intent: &NormalizedIntent) -> String {
    let metadata = intent
        .metadata
        .iter()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "request_id" | "correlation_id" | "idempotency_key"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();

    let envelope = Value::Object(Map::from_iter([
        (
            "tenant_id".to_owned(),
            Value::String(intent.tenant_id.to_string()),
        ),
        ("kind".to_owned(), Value::String(intent.kind.to_string())),
        (
            "payload".to_owned(),
            canonicalize_json_value(&intent.payload),
        ),
        (
            "metadata".to_owned(),
            serde_json::to_value(metadata).unwrap_or(Value::Null),
        ),
    ]));

    let canonical = serde_json::to_vec(&canonicalize_json_value(&envelope)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("{:x}", hasher.finalize())
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json_value).collect()),
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = Map::with_capacity(keys.len());
            for key in keys {
                if let Some(item) = map.get(&key) {
                    canonical.insert(key, canonicalize_json_value(item));
                }
            }
            Value::Object(canonical)
        }
        _ => value.clone(),
    }
}
