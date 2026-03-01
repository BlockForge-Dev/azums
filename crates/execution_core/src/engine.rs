use crate::error::{CoreError, CoreResult};
use crate::model::{
    is_terminal_state, AdapterExecutionRequest, AdapterOutcome, CallbackId, CallbackJob,
    CanonicalState, ExecutionJob, FailureInfo, LeasedJob, NormalizedIntent, PlatformClassification,
    ReceiptEntry, ReplayCommand, ReplayDecisionId, ReplayDecisionRecord, StateTransition,
    StatusSummary, TransitionActor, TransitionId,
};
use crate::policy::{
    classify_adapter_outcome, transition_allowed, ReplayPolicy, RetryDecision, RetryPolicy,
};
use crate::ports::{AdapterExecutionError, AdapterRouter, Authorizer, Clock, DurableStore};
use std::collections::BTreeMap;
use std::sync::Arc;

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

        if let Some(idempotency_key) = idempotency_key.as_ref() {
            if let Some(existing_intent_id) = self
                .store
                .lookup_intent_by_idempotency(&intent.tenant_id, idempotency_key)
                .await?
            {
                return self
                    .existing_submit_result(&intent.tenant_id, &existing_intent_id, idempotency_key)
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
            let bound_intent_id = self
                .store
                .bind_intent_idempotency(&intent.tenant_id, idempotency_key, &intent.intent_id)
                .await?;
            if bound_intent_id != intent.intent_id {
                return self
                    .existing_submit_result(&intent.tenant_id, &bound_intent_id, idempotency_key)
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

        self.store.persist_intent(&intent).await?;
        self.store.persist_job(&job).await?;
        self.bootstrap_received_transition(&job).await?;
        self.transition_job(
            &mut job,
            CanonicalState::Validated,
            PlatformClassification::Success,
            "intent_validated",
            "intent passed core validation".to_owned(),
            TransitionActor::System,
            0,
        )
        .await?;
        self.transition_job(
            &mut job,
            CanonicalState::Queued,
            PlatformClassification::Success,
            "adapter_routed",
            format!("queued via route `{}`", route.rule),
            TransitionActor::System,
            0,
        )
        .await?;
        self.store.enqueue_dispatch(&job.job_id, None).await?;

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

        let adapter = self.router.adapter_executor(&job.adapter_id)?;
        let outcome = match adapter.execute(&request).await {
            Ok(outcome) => outcome,
            Err(err) => self.map_adapter_error_to_outcome(err),
        };

        self.handle_adapter_result(job, outcome, current_attempt)
            .await
    }

    pub async fn handle_adapter_result(
        &self,
        mut job: ExecutionJob,
        outcome: AdapterOutcome,
        current_attempt: u32,
    ) -> CoreResult<DispatchResult> {
        if let AdapterOutcome::InProgress { poll_after_ms, .. } = outcome {
            return self
                .handle_in_progress(&mut job, poll_after_ms, current_attempt)
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
                self.schedule_retry(&mut job, failure, current_attempt)
                    .await
            }
            PlatformClassification::TerminalFailure => {
                let failure =
                    failure.expect("terminal classification always includes failure info");
                self.mark_terminal_failure(&mut job, failure, current_attempt)
                    .await
            }
            PlatformClassification::Blocked => {
                let failure = failure.expect("blocked classification always includes failure info");
                self.mark_terminal_failure(&mut job, failure, current_attempt)
                    .await
            }
            PlatformClassification::ManualReview => {
                let failure =
                    failure.expect("manual review classification always includes failure info");
                self.mark_terminal_failure(&mut job, failure, current_attempt)
                    .await
            }
        }
    }

    async fn handle_in_progress(
        &self,
        job: &mut ExecutionJob,
        poll_after_ms: Option<u64>,
        current_attempt: u32,
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

    pub async fn schedule_retry(
        &self,
        job: &mut ExecutionJob,
        failure: FailureInfo,
        current_attempt: u32,
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
                self.mark_dead_lettered(job, exhausted_failure, current_attempt)
                    .await
            }
        }
    }

    pub async fn mark_terminal_failure(
        &self,
        job: &mut ExecutionJob,
        failure: FailureInfo,
        current_attempt: u32,
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
        )
        .await?;
        self.enqueue_terminal_callback_if_needed(job).await?;

        Ok(DispatchResult {
            job: job.clone(),
            classification: PlatformClassification::TerminalFailure,
        })
    }

    pub async fn emit_receipt(&self, receipt: ReceiptEntry) -> CoreResult<()> {
        self.store.append_receipt(&receipt).await?;
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
        self.bootstrap_received_transition(&replay_job).await?;
        self.transition_job(
            &mut replay_job,
            CanonicalState::Validated,
            PlatformClassification::Success,
            "replay_validated",
            "replay request validated".to_owned(),
            TransitionActor::System,
            0,
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

    async fn bootstrap_received_transition(&self, job: &ExecutionJob) -> CoreResult<()> {
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
        self.store.record_transition(&transition).await?;

        let receipt = ReceiptEntry {
            receipt_id: crate::model::ReceiptId::new(),
            tenant_id: transition.tenant_id,
            intent_id: transition.intent_id,
            job_id: transition.job_id,
            attempt_no: 0,
            state: transition.to_state,
            classification: transition.classification,
            summary: transition.reason,
            details: {
                let mut details = BTreeMap::from([
                    ("reason_code".to_owned(), transition.reason_code),
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
    ) -> CoreResult<()> {
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
        self.store.record_transition(&transition).await?;

        job.state = to_state;
        job.updated_at_ms = now_ms;
        self.store.update_job(job).await?;

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

        let receipt = ReceiptEntry {
            receipt_id: crate::model::ReceiptId::new(),
            tenant_id: job.tenant_id.clone(),
            intent_id: job.intent_id.clone(),
            job_id: job.job_id.clone(),
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
        self.emit_receipt(receipt).await?;
        Ok(())
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
        let existing_job = self
            .store
            .get_latest_job_for_intent(tenant_id, intent_id)
            .await?
            .ok_or_else(|| CoreError::IdempotencyConflict {
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

fn normalize_idempotency_key(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}
