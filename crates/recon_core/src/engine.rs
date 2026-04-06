use crate::error::ReconError;
use crate::model::{
    evidence, make_exception, make_fact_id, normalize_result, ExpectedFactDraft, ObservedFactDraft,
    ReconContext, ReconEvidenceSnapshot, ReconReceipt, ReconResult, ReconRun, ReconRunState,
    ReconRunStateTransition, ReconSubject,
};
use crate::rules::ReconRuleRegistry;
use async_trait::async_trait;
use exception_intelligence::{
    ExceptionCase, ExceptionCategory, ExceptionDraft, ExceptionSeverity, ExceptionState,
    PostgresExceptionStore,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ReconEngineConfig {
    pub max_retry_attempts: u32,
    pub retry_backoff_ms: u64,
}

impl Default for ReconEngineConfig {
    fn default() -> Self {
        Self {
            max_retry_attempts: 3,
            retry_backoff_ms: 5_000,
        }
    }
}

#[async_trait]
pub trait ReconEngineStore: Send + Sync {
    async fn load_recon_context(&self, subject: &ReconSubject) -> Result<ReconContext, ReconError>;
    async fn load_adapter_observations(
        &self,
        subject: &ReconSubject,
    ) -> Result<Vec<Value>, ReconError>;
    async fn create_run(&self, run: &ReconRun) -> Result<(), ReconError>;
    async fn append_run_state_transition(
        &self,
        transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError>;
    async fn finalize_run(
        &self,
        subject: &ReconSubject,
        run: &ReconRun,
        receipt: &ReconReceipt,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        evidence: &ReconEvidenceSnapshot,
        final_transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError>;
}

#[async_trait]
pub trait ReconExceptionSink: Send + Sync {
    async fn sync_subject_cases(
        &self,
        tenant_id: &str,
        subject_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        latest_run_id: Option<&str>,
        drafts: &[ExceptionDraft],
        now_ms: u64,
    ) -> Result<Vec<ExceptionCase>, ReconError>;
}

#[async_trait]
impl ReconExceptionSink for PostgresExceptionStore {
    async fn sync_subject_cases(
        &self,
        tenant_id: &str,
        subject_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        latest_run_id: Option<&str>,
        drafts: &[ExceptionDraft],
        now_ms: u64,
    ) -> Result<Vec<ExceptionCase>, ReconError> {
        PostgresExceptionStore::sync_subject_cases(
            self,
            tenant_id,
            subject_id,
            intent_id,
            job_id,
            adapter_id,
            latest_run_id,
            drafts,
            now_ms,
        )
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))
    }
}

pub struct ReconEngine<S, E> {
    store: Arc<S>,
    exception_sink: Arc<E>,
    rules: ReconRuleRegistry,
    cfg: ReconEngineConfig,
}

impl<S, E> ReconEngine<S, E>
where
    S: ReconEngineStore + 'static,
    E: ReconExceptionSink + 'static,
{
    pub fn new(
        store: Arc<S>,
        exception_sink: Arc<E>,
        rules: ReconRuleRegistry,
        cfg: ReconEngineConfig,
    ) -> Self {
        Self {
            store,
            exception_sink,
            rules,
            cfg,
        }
    }

    pub async fn process_subject(&self, subject: &ReconSubject) -> Result<(), ReconError> {
        let now_ms = current_ms();
        let rule_pack = self.rules.resolve(&subject.adapter_id);
        let mut run = base_run(subject, rule_pack.map(|pack| pack.rule_pack_id()), now_ms);

        self.store.create_run(&run).await?;
        self.record_transition(
            &run,
            None,
            ReconRunState::Queued,
            "recon_subject_claimed",
            json!({
                "subject_id": subject.subject_id,
                "attempt_number": run.attempt_number,
            }),
        )
        .await?;

        let Some(rule_pack) = rule_pack else {
            return self
                .finalize_terminal_failure(
                    subject,
                    &mut run,
                    ReconError::RulePackUnavailable(subject.adapter_id.clone()),
                    Vec::new(),
                    Vec::new(),
                    ReconContext::default(),
                    Vec::new(),
                )
                .await;
        };

        self.record_transition(
            &run,
            Some(run.lifecycle_state),
            ReconRunState::CollectingObservations,
            "collecting_observations",
            json!({
                "adapter_id": subject.adapter_id,
                "rule_pack": rule_pack.rule_pack_id(),
            }),
        )
        .await?;
        run.lifecycle_state = ReconRunState::CollectingObservations;
        run.updated_at_ms = current_ms();

        let context = match self.store.load_recon_context(subject).await {
            Ok(context) => context,
            Err(err) => {
                return self
                    .handle_processing_error(
                        subject,
                        &mut run,
                        err,
                        Vec::new(),
                        Vec::new(),
                        ReconContext::default(),
                        Vec::new(),
                    )
                    .await;
            }
        };

        let adapter_rows = match self.store.load_adapter_observations(subject).await {
            Ok(rows) => rows,
            Err(err) => {
                return self
                    .handle_processing_error(
                        subject,
                        &mut run,
                        err,
                        Vec::new(),
                        Vec::new(),
                        context,
                        Vec::new(),
                    )
                    .await;
            }
        };

        let expected = match rule_pack.build_expected_facts(subject, &context).await {
            Ok(expected) => expected,
            Err(err) => {
                return self
                    .handle_processing_error(
                        subject,
                        &mut run,
                        err,
                        Vec::new(),
                        Vec::new(),
                        context,
                        adapter_rows,
                    )
                    .await;
            }
        };

        let observed = match rule_pack
            .collect_observed_facts(subject, &context, &adapter_rows)
            .await
        {
            Ok(observed) => observed,
            Err(err) => {
                return self
                    .handle_processing_error(
                        subject,
                        &mut run,
                        err,
                        expected,
                        Vec::new(),
                        context,
                        adapter_rows,
                    )
                    .await;
            }
        };

        self.record_transition(
            &run,
            Some(run.lifecycle_state),
            ReconRunState::Matching,
            "matching",
            json!({
                "expected_fact_count": expected.len(),
                "observed_fact_count": observed.len(),
            }),
        )
        .await?;
        run.lifecycle_state = ReconRunState::Matching;
        run.updated_at_ms = current_ms();

        let matched = rule_pack.match_facts(subject, &expected, &observed);
        let classification = rule_pack.classify(subject, &context, &expected, &observed, &matched);
        let mut emission = rule_pack.emit_recon_result(subject, &matched, classification);

        let cases = self
            .exception_sink
            .sync_subject_cases(
                &subject.tenant_id,
                &subject.subject_id,
                &subject.intent_id,
                &subject.job_id,
                &subject.adapter_id,
                Some(&run.run_id),
                &emission.exceptions,
                current_ms(),
            )
            .await?;

        run.lifecycle_state = ReconRunState::WritingReceipt;
        run.updated_at_ms = current_ms();
        self.record_transition(
            &run,
            Some(ReconRunState::Matching),
            ReconRunState::WritingReceipt,
            "writing_receipt",
            json!({
                "exception_case_count": cases.len(),
            }),
        )
        .await?;

        let normalized_result = normalize_result(emission.outcome);
        let final_now_ms = current_ms();
        run.lifecycle_state = ReconRunState::Completed;
        run.normalized_result = Some(normalized_result);
        run.outcome = emission.outcome;
        run.summary = emission.summary.clone();
        run.machine_reason = emission.machine_reason.clone();
        run.expected_fact_count = expected.len() as u32;
        run.observed_fact_count = observed.len() as u32;
        run.matched_fact_count = matched.matched_fact_keys.len() as u32;
        run.unmatched_fact_count = (matched.missing_expected.len()
            + matched.unexpected_observed.len()
            + matched.mismatches.len()) as u32;
        run.updated_at_ms = final_now_ms;
        run.completed_at_ms = Some(final_now_ms);
        run.exception_case_ids = cases.into_iter().map(|case| case.case_id).collect();
        run.last_error = None;
        run.retry_scheduled_at_ms = None;

        emission
            .details
            .entry("normalized_result".to_owned())
            .or_insert_with(|| normalized_result.as_str().to_owned());
        emission
            .details
            .entry("run_state".to_owned())
            .or_insert_with(|| run.lifecycle_state.as_str().to_owned());

        let receipt = ReconReceipt {
            recon_receipt_id: make_fact_id("reconrcpt"),
            run_id: run.run_id.clone(),
            subject_id: subject.subject_id.clone(),
            normalized_result: Some(normalized_result),
            outcome: run.outcome,
            summary: run.summary.clone(),
            details: emission.details.clone(),
            created_at_ms: final_now_ms,
        };
        let evidence = build_evidence_snapshot(
            subject,
            &run,
            &context,
            &adapter_rows,
            &expected,
            &observed,
            &matched,
            &emission.details,
            &emission.exceptions,
            final_now_ms,
        );
        let final_transition = transition(
            &run,
            Some(ReconRunState::WritingReceipt),
            ReconRunState::Completed,
            "recon_completed",
            json!({
                "outcome": run.outcome.as_str(),
                "normalized_result": normalized_result.as_str(),
            }),
            final_now_ms,
        );

        self.store
            .finalize_run(
                subject,
                &run,
                &receipt,
                &expected,
                &observed,
                &evidence,
                &final_transition,
            )
            .await
    }

    async fn handle_processing_error(
        &self,
        subject: &ReconSubject,
        run: &mut ReconRun,
        err: ReconError,
        expected: Vec<ExpectedFactDraft>,
        observed: Vec<ObservedFactDraft>,
        context: ReconContext,
        adapter_rows: Vec<Value>,
    ) -> Result<(), ReconError> {
        if err.is_retryable() && subject.recon_retry_count < self.cfg.max_retry_attempts {
            self.schedule_retry(subject, run, err, expected, observed, context, adapter_rows)
                .await
        } else {
            self.finalize_terminal_failure(
                subject,
                run,
                err,
                expected,
                observed,
                context,
                adapter_rows,
            )
            .await
        }
    }

    async fn schedule_retry(
        &self,
        subject: &ReconSubject,
        run: &mut ReconRun,
        err: ReconError,
        expected: Vec<ExpectedFactDraft>,
        observed: Vec<ObservedFactDraft>,
        context: ReconContext,
        adapter_rows: Vec<Value>,
    ) -> Result<(), ReconError> {
        let now_ms = current_ms();
        let next_retry_at_ms = now_ms
            + self
                .cfg
                .retry_backoff_ms
                .saturating_mul((subject.recon_retry_count + 1) as u64);
        let previous_state = run.lifecycle_state;
        run.lifecycle_state = ReconRunState::RetryScheduled;
        run.normalized_result = Some(ReconResult::PendingObservation);
        run.outcome = crate::model::ReconOutcome::CollectingObservations;
        run.summary = format!("reconciliation retry scheduled: {err}");
        run.machine_reason = "recon_retry_scheduled".to_owned();
        run.updated_at_ms = now_ms;
        run.completed_at_ms = Some(now_ms);
        run.retry_scheduled_at_ms = Some(next_retry_at_ms);
        run.last_error = Some(err.to_string());
        run.expected_fact_count = expected.len() as u32;
        run.observed_fact_count = observed.len() as u32;

        let receipt = ReconReceipt {
            recon_receipt_id: make_fact_id("reconrcpt"),
            run_id: run.run_id.clone(),
            subject_id: subject.subject_id.clone(),
            normalized_result: run.normalized_result,
            outcome: run.outcome,
            summary: run.summary.clone(),
            details: BTreeMap::from([
                (
                    "run_state".to_owned(),
                    run.lifecycle_state.as_str().to_owned(),
                ),
                (
                    "normalized_result".to_owned(),
                    ReconResult::PendingObservation.as_str().to_owned(),
                ),
                (
                    "retry_scheduled_at_ms".to_owned(),
                    next_retry_at_ms.to_string(),
                ),
                ("error".to_owned(), err.to_string()),
            ]),
            created_at_ms: now_ms,
        };
        let evidence = build_evidence_snapshot(
            subject,
            run,
            &context,
            &adapter_rows,
            &expected,
            &observed,
            &crate::model::ReconMatchResult::default(),
            &receipt.details,
            &[],
            now_ms,
        );
        let final_transition = transition(
            run,
            Some(previous_state),
            ReconRunState::RetryScheduled,
            "recon_retry_scheduled",
            json!({
                "error": err.to_string(),
                "next_retry_at_ms": next_retry_at_ms,
            }),
            now_ms,
        );

        self.store
            .finalize_run(
                subject,
                run,
                &receipt,
                &expected,
                &observed,
                &evidence,
                &final_transition,
            )
            .await
    }

    async fn finalize_terminal_failure(
        &self,
        subject: &ReconSubject,
        run: &mut ReconRun,
        err: ReconError,
        expected: Vec<ExpectedFactDraft>,
        observed: Vec<ObservedFactDraft>,
        context: ReconContext,
        adapter_rows: Vec<Value>,
    ) -> Result<(), ReconError> {
        let now_ms = current_ms();
        let previous_state = run.lifecycle_state;
        run.lifecycle_state = ReconRunState::Failed;
        run.normalized_result = Some(ReconResult::ManualReviewRequired);
        run.outcome = crate::model::ReconOutcome::ManualReviewRequired;
        run.summary = format!("reconciliation requires manual review: {err}");
        run.machine_reason = "recon_engine_failed".to_owned();
        run.updated_at_ms = now_ms;
        run.completed_at_ms = Some(now_ms);
        run.retry_scheduled_at_ms = None;
        run.last_error = Some(err.to_string());
        run.expected_fact_count = expected.len() as u32;
        run.observed_fact_count = observed.len() as u32;

        let exceptions = vec![make_exception(
            ExceptionCategory::ManualReviewRequired,
            ExceptionSeverity::High,
            ExceptionState::Open,
            "reconciliation engine could not complete automatically",
            "recon_engine_failed",
            vec![evidence(
                "recon_engine_error",
                Some("recon_core_runs".to_owned()),
                Some(run.run_id.clone()),
                Some(now_ms),
                json!({
                    "error": err.to_string(),
                    "adapter_id": subject.adapter_id,
                    "subject_id": subject.subject_id,
                }),
            )],
        )];
        let cases = self
            .exception_sink
            .sync_subject_cases(
                &subject.tenant_id,
                &subject.subject_id,
                &subject.intent_id,
                &subject.job_id,
                &subject.adapter_id,
                Some(&run.run_id),
                &exceptions,
                now_ms,
            )
            .await?;
        run.exception_case_ids = cases.into_iter().map(|case| case.case_id).collect();

        let receipt = ReconReceipt {
            recon_receipt_id: make_fact_id("reconrcpt"),
            run_id: run.run_id.clone(),
            subject_id: subject.subject_id.clone(),
            normalized_result: run.normalized_result,
            outcome: run.outcome,
            summary: run.summary.clone(),
            details: BTreeMap::from([
                (
                    "run_state".to_owned(),
                    run.lifecycle_state.as_str().to_owned(),
                ),
                (
                    "normalized_result".to_owned(),
                    ReconResult::ManualReviewRequired.as_str().to_owned(),
                ),
                ("error".to_owned(), err.to_string()),
            ]),
            created_at_ms: now_ms,
        };
        let evidence = build_evidence_snapshot(
            subject,
            run,
            &context,
            &adapter_rows,
            &expected,
            &observed,
            &crate::model::ReconMatchResult::default(),
            &receipt.details,
            &exceptions,
            now_ms,
        );
        let final_transition = transition(
            run,
            Some(previous_state),
            ReconRunState::Failed,
            "recon_failed",
            json!({
                "error": err.to_string(),
            }),
            now_ms,
        );

        self.store
            .finalize_run(
                subject,
                run,
                &receipt,
                &expected,
                &observed,
                &evidence,
                &final_transition,
            )
            .await
    }

    async fn record_transition(
        &self,
        run: &ReconRun,
        from_state: Option<ReconRunState>,
        to_state: ReconRunState,
        reason: &str,
        payload: Value,
    ) -> Result<(), ReconError> {
        self.store
            .append_run_state_transition(&transition(
                run,
                from_state,
                to_state,
                reason,
                payload,
                current_ms(),
            ))
            .await
    }
}

fn transition(
    run: &ReconRun,
    from_state: Option<ReconRunState>,
    to_state: ReconRunState,
    reason: &str,
    payload: Value,
    occurred_at_ms: u64,
) -> ReconRunStateTransition {
    ReconRunStateTransition {
        state_transition_id: make_fact_id("reconstate"),
        run_id: run.run_id.clone(),
        subject_id: run.subject_id.clone(),
        from_state,
        to_state,
        reason: reason.to_owned(),
        payload,
        occurred_at_ms,
    }
}

fn base_run(subject: &ReconSubject, rule_pack_id: Option<&str>, now_ms: u64) -> ReconRun {
    ReconRun {
        run_id: make_fact_id("reconrun"),
        subject_id: subject.subject_id.clone(),
        tenant_id: subject.tenant_id.clone(),
        intent_id: subject.intent_id.clone(),
        job_id: subject.job_id.clone(),
        adapter_id: subject.adapter_id.clone(),
        rule_pack: rule_pack_id.unwrap_or("unresolved").to_owned(),
        lifecycle_state: ReconRunState::Queued,
        normalized_result: Some(ReconResult::PendingObservation),
        outcome: crate::model::ReconOutcome::Queued,
        summary: "reconciliation run queued".to_owned(),
        machine_reason: "recon_run_queued".to_owned(),
        expected_fact_count: 0,
        observed_fact_count: 0,
        matched_fact_count: 0,
        unmatched_fact_count: 0,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        completed_at_ms: None,
        attempt_number: subject.recon_attempt_count + 1,
        retry_scheduled_at_ms: None,
        last_error: None,
        exception_case_ids: Vec::new(),
    }
}

fn build_evidence_snapshot(
    subject: &ReconSubject,
    run: &ReconRun,
    context: &ReconContext,
    adapter_rows: &[Value],
    expected: &[ExpectedFactDraft],
    observed: &[ObservedFactDraft],
    matched: &crate::model::ReconMatchResult,
    details: &BTreeMap<String, String>,
    exceptions: &[ExceptionDraft],
    now_ms: u64,
) -> ReconEvidenceSnapshot {
    ReconEvidenceSnapshot {
        evidence_snapshot_id: make_fact_id("reconevid"),
        run_id: run.run_id.clone(),
        subject_id: subject.subject_id.clone(),
        tenant_id: subject.tenant_id.clone(),
        intent_id: subject.intent_id.clone(),
        job_id: subject.job_id.clone(),
        adapter_id: subject.adapter_id.clone(),
        lifecycle_state: run.lifecycle_state,
        normalized_result: run.normalized_result,
        context: serde_json::to_value(context).unwrap_or_else(|_| json!({})),
        adapter_rows: Value::Array(adapter_rows.to_vec()),
        expected_facts: serde_json::to_value(expected).unwrap_or_else(|_| Value::Array(Vec::new())),
        observed_facts: serde_json::to_value(observed).unwrap_or_else(|_| Value::Array(Vec::new())),
        match_result: serde_json::to_value(matched).unwrap_or_else(|_| json!({})),
        details: serde_json::to_value(details).unwrap_or_else(|_| json!({})),
        exceptions: serde_json::to_value(exceptions).unwrap_or_else(|_| Value::Array(Vec::new())),
        created_at_ms: now_ms,
    }
}

fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ExpectedFactDraft, ObservedFactDraft, ReconClassification, ReconMatchResult,
    };
    use crate::rules::ReconRulePack;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeStore {
        context: Mutex<ReconContext>,
        adapter_rows: Mutex<Vec<Value>>,
        created_runs: Mutex<Vec<ReconRun>>,
        transitions: Mutex<Vec<ReconRunStateTransition>>,
        finalized_runs: Mutex<Vec<ReconRun>>,
        finalized_receipts: Mutex<Vec<ReconReceipt>>,
        evidence: Mutex<Vec<ReconEvidenceSnapshot>>,
        fail_context: Mutex<Option<String>>,
    }

    #[async_trait]
    impl ReconEngineStore for FakeStore {
        async fn load_recon_context(
            &self,
            _subject: &ReconSubject,
        ) -> Result<ReconContext, ReconError> {
            if let Some(err) = self.fail_context.lock().unwrap().clone() {
                return Err(ReconError::Backend(err));
            }
            Ok(self.context.lock().unwrap().clone())
        }

        async fn load_adapter_observations(
            &self,
            _subject: &ReconSubject,
        ) -> Result<Vec<Value>, ReconError> {
            Ok(self.adapter_rows.lock().unwrap().clone())
        }

        async fn create_run(&self, run: &ReconRun) -> Result<(), ReconError> {
            self.created_runs.lock().unwrap().push(run.clone());
            Ok(())
        }

        async fn append_run_state_transition(
            &self,
            transition: &ReconRunStateTransition,
        ) -> Result<(), ReconError> {
            self.transitions.lock().unwrap().push(transition.clone());
            Ok(())
        }

        async fn finalize_run(
            &self,
            _subject: &ReconSubject,
            run: &ReconRun,
            receipt: &ReconReceipt,
            _expected: &[ExpectedFactDraft],
            _observed: &[ObservedFactDraft],
            evidence: &ReconEvidenceSnapshot,
            final_transition: &ReconRunStateTransition,
        ) -> Result<(), ReconError> {
            self.finalized_runs.lock().unwrap().push(run.clone());
            self.finalized_receipts
                .lock()
                .unwrap()
                .push(receipt.clone());
            self.evidence.lock().unwrap().push(evidence.clone());
            self.transitions
                .lock()
                .unwrap()
                .push(final_transition.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeExceptionSink {
        cases: Mutex<Vec<ExceptionCase>>,
    }

    #[async_trait]
    impl ReconExceptionSink for FakeExceptionSink {
        async fn sync_subject_cases(
            &self,
            tenant_id: &str,
            subject_id: &str,
            intent_id: &str,
            job_id: &str,
            adapter_id: &str,
            latest_run_id: Option<&str>,
            drafts: &[ExceptionDraft],
            now_ms: u64,
        ) -> Result<Vec<ExceptionCase>, ReconError> {
            let cases: Vec<ExceptionCase> = drafts
                .iter()
                .enumerate()
                .map(|(idx, draft)| ExceptionCase {
                    case_id: format!("case_{idx}"),
                    tenant_id: tenant_id.to_owned(),
                    subject_id: subject_id.to_owned(),
                    intent_id: intent_id.to_owned(),
                    job_id: job_id.to_owned(),
                    adapter_id: adapter_id.to_owned(),
                    category: draft.category,
                    severity: draft.severity,
                    state: draft.state,
                    summary: draft.summary.clone(),
                    machine_reason: draft.machine_reason.clone(),
                    dedupe_key: format!("dedupe_{idx}"),
                    cluster_key: format!("cluster_{idx}"),
                    first_seen_at_ms: now_ms,
                    last_seen_at_ms: now_ms,
                    occurrence_count: 1,
                    created_at_ms: now_ms,
                    updated_at_ms: now_ms,
                    resolved_at_ms: None,
                    latest_run_id: latest_run_id.map(ToOwned::to_owned),
                    latest_outcome_id: None,
                    latest_recon_receipt_id: None,
                    latest_execution_receipt_id: None,
                    latest_evidence_snapshot_id: None,
                    last_actor: Some("fake_exception_sink".to_owned()),
                })
                .collect();
            *self.cases.lock().unwrap() = cases.clone();
            Ok(cases)
        }
    }

    struct FakeRulePack;

    #[async_trait]
    impl ReconRulePack for FakeRulePack {
        fn adapter_id(&self) -> &'static str {
            "adapter_test"
        }

        fn rule_pack_id(&self) -> &'static str {
            "test.v1"
        }

        async fn build_expected_facts(
            &self,
            _subject: &ReconSubject,
            _context: &ReconContext,
        ) -> Result<Vec<ExpectedFactDraft>, ReconError> {
            Ok(vec![ExpectedFactDraft {
                fact_type: "test".to_owned(),
                fact_key: "test.state".to_owned(),
                fact_value: json!("ok"),
                derived_from: json!({"source": "test"}),
            }])
        }

        async fn collect_observed_facts(
            &self,
            _subject: &ReconSubject,
            _context: &ReconContext,
            _adapter_rows: &[Value],
        ) -> Result<Vec<ObservedFactDraft>, ReconError> {
            Ok(vec![ObservedFactDraft {
                fact_type: "test".to_owned(),
                fact_key: "test.state".to_owned(),
                fact_value: json!("ok"),
                source_kind: "adapter".to_owned(),
                source_table: Some("test.table".to_owned()),
                source_id: Some("row_1".to_owned()),
                metadata: json!({"row": 1}),
                observed_at_ms: Some(1),
            }])
        }

        fn match_facts(
            &self,
            _subject: &ReconSubject,
            expected: &[ExpectedFactDraft],
            observed: &[ObservedFactDraft],
        ) -> ReconMatchResult {
            let mut result = ReconMatchResult::default();
            if expected.first().map(|fact| &fact.fact_value)
                == observed.first().map(|fact| &fact.fact_value)
            {
                result.matched_fact_keys.push("test.state".to_owned());
            }
            result
        }

        fn classify(
            &self,
            _subject: &ReconSubject,
            _context: &ReconContext,
            _expected: &[ExpectedFactDraft],
            _observed: &[ObservedFactDraft],
            _matched: &ReconMatchResult,
        ) -> ReconClassification {
            ReconClassification {
                outcome: Some(crate::model::ReconOutcome::Matched),
                summary: Some("matched".to_owned()),
                machine_reason: Some("matched".to_owned()),
                details: BTreeMap::new(),
                exceptions: Vec::new(),
            }
        }
    }

    fn subject() -> ReconSubject {
        ReconSubject {
            subject_id: "subject_1".to_owned(),
            tenant_id: "tenant_1".to_owned(),
            intent_id: "intent_1".to_owned(),
            job_id: "job_1".to_owned(),
            adapter_id: "adapter_test".to_owned(),
            canonical_state: "Succeeded".to_owned(),
            platform_classification: "Success".to_owned(),
            latest_receipt_id: Some("receipt_1".to_owned()),
            latest_transition_id: Some("transition_1".to_owned()),
            latest_callback_id: None,
            latest_signal_id: Some("signal_1".to_owned()),
            latest_signal_kind: Some("finalized".to_owned()),
            execution_correlation_id: Some("corr_1".to_owned()),
            adapter_execution_reference: Some("exec_1".to_owned()),
            external_observation_key: Some("obs_1".to_owned()),
            expected_fact_snapshot: Some(json!({"state": "ok"})),
            dirty: true,
            recon_attempt_count: 0,
            recon_retry_count: 0,
            created_at_ms: 1,
            updated_at_ms: 2,
            scheduled_at_ms: Some(2),
            next_reconcile_after_ms: Some(2),
            last_reconciled_at_ms: None,
            last_recon_error: None,
            last_run_state: None,
        }
    }

    #[tokio::test]
    async fn engine_processes_subject_without_knowing_adapter_internals() {
        let store = Arc::new(FakeStore::default());
        let exception_sink = Arc::new(FakeExceptionSink::default());
        let mut rules = ReconRuleRegistry::new();
        rules.register(Box::new(FakeRulePack));
        let engine = ReconEngine::new(
            store.clone(),
            exception_sink,
            rules,
            ReconEngineConfig::default(),
        );

        engine.process_subject(&subject()).await.unwrap();

        let runs = store.finalized_runs.lock().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].lifecycle_state, ReconRunState::Completed);
        assert_eq!(runs[0].normalized_result, Some(ReconResult::Matched));
        drop(runs);

        let transitions = store.transitions.lock().unwrap();
        assert!(transitions
            .iter()
            .any(|transition| transition.to_state == ReconRunState::CollectingObservations));
        assert!(transitions
            .iter()
            .any(|transition| transition.to_state == ReconRunState::Matching));
        assert!(transitions
            .iter()
            .any(|transition| transition.to_state == ReconRunState::Completed));
        drop(transitions);

        let evidence = store.evidence.lock().unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].normalized_result, Some(ReconResult::Matched));
    }

    #[tokio::test]
    async fn backend_failures_schedule_recon_retry_without_corrupting_lineage() {
        let store = Arc::new(FakeStore::default());
        *store.fail_context.lock().unwrap() = Some("temporary outage".to_owned());
        let exception_sink = Arc::new(FakeExceptionSink::default());
        let mut rules = ReconRuleRegistry::new();
        rules.register(Box::new(FakeRulePack));
        let engine = ReconEngine::new(
            store.clone(),
            exception_sink,
            rules,
            ReconEngineConfig::default(),
        );

        engine.process_subject(&subject()).await.unwrap();

        let runs = store.finalized_runs.lock().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].lifecycle_state, ReconRunState::RetryScheduled);
        assert_eq!(
            runs[0].normalized_result,
            Some(ReconResult::PendingObservation)
        );
        assert!(runs[0].retry_scheduled_at_ms.is_some());
        assert!(runs[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("temporary outage"));
    }
}
