use exception_intelligence::{
    ExceptionCategory, ExceptionDraft, ExceptionEvidenceDraft, ExceptionSeverity, ExceptionState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconOutcome {
    Queued,
    CollectingObservations,
    Matching,
    Matched,
    PartiallyMatched,
    Unmatched,
    Stale,
    ManualReviewRequired,
    Resolved,
}

impl ReconOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CollectingObservations => "collecting_observations",
            Self::Matching => "matching",
            Self::Matched => "matched",
            Self::PartiallyMatched => "partially_matched",
            Self::Unmatched => "unmatched",
            Self::Stale => "stale",
            Self::ManualReviewRequired => "manual_review_required",
            Self::Resolved => "resolved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "collecting_observations" => Some(Self::CollectingObservations),
            "matching" => Some(Self::Matching),
            "matched" => Some(Self::Matched),
            "partially_matched" => Some(Self::PartiallyMatched),
            "unmatched" => Some(Self::Unmatched),
            "stale" => Some(Self::Stale),
            "manual_review_required" => Some(Self::ManualReviewRequired),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconResult {
    Matched,
    PartiallyMatched,
    Unmatched,
    PendingObservation,
    Stale,
    ManualReviewRequired,
}

impl ReconResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::PartiallyMatched => "partially_matched",
            Self::Unmatched => "unmatched",
            Self::PendingObservation => "pending_observation",
            Self::Stale => "stale",
            Self::ManualReviewRequired => "manual_review_required",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "matched" => Some(Self::Matched),
            "partially_matched" => Some(Self::PartiallyMatched),
            "unmatched" => Some(Self::Unmatched),
            "pending_observation" => Some(Self::PendingObservation),
            "stale" => Some(Self::Stale),
            "manual_review_required" | "manual_review" => Some(Self::ManualReviewRequired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconRunState {
    Queued,
    CollectingObservations,
    Matching,
    WritingReceipt,
    Completed,
    RetryScheduled,
    Failed,
}

impl ReconRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CollectingObservations => "collecting_observations",
            Self::Matching => "matching",
            Self::WritingReceipt => "writing_receipt",
            Self::Completed => "completed",
            Self::RetryScheduled => "retry_scheduled",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "collecting_observations" => Some(Self::CollectingObservations),
            "matching" => Some(Self::Matching),
            "writing_receipt" => Some(Self::WritingReceipt),
            "completed" => Some(Self::Completed),
            "retry_scheduled" => Some(Self::RetryScheduled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconOperatorActionType {
    Rerun,
    RefreshObservation,
}

impl ReconOperatorActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rerun => "rerun_reconciliation",
            Self::RefreshObservation => "refresh_observation",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rerun_reconciliation" | "rerun" => Some(Self::Rerun),
            "refresh_observation" | "refresh" => Some(Self::RefreshObservation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconSubject {
    pub subject_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub canonical_state: String,
    pub platform_classification: String,
    pub latest_receipt_id: Option<String>,
    pub latest_transition_id: Option<String>,
    pub latest_callback_id: Option<String>,
    pub latest_signal_id: Option<String>,
    pub latest_signal_kind: Option<String>,
    pub execution_correlation_id: Option<String>,
    pub adapter_execution_reference: Option<String>,
    pub external_observation_key: Option<String>,
    pub expected_fact_snapshot: Option<Value>,
    pub dirty: bool,
    pub recon_attempt_count: u32,
    pub recon_retry_count: u32,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub scheduled_at_ms: Option<u64>,
    pub next_reconcile_after_ms: Option<u64>,
    pub last_reconciled_at_ms: Option<u64>,
    pub last_recon_error: Option<String>,
    pub last_run_state: Option<ReconRunState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFact {
    pub expected_fact_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub fact_type: String,
    pub fact_key: String,
    pub fact_value: Value,
    pub derived_from: Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedFact {
    pub observed_fact_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub fact_type: String,
    pub fact_key: String,
    pub fact_value: Value,
    pub source_kind: String,
    pub source_table: Option<String>,
    pub source_id: Option<String>,
    pub metadata: Value,
    pub observed_at_ms: Option<u64>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconRun {
    pub run_id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub rule_pack: String,
    pub lifecycle_state: ReconRunState,
    pub normalized_result: Option<ReconResult>,
    pub outcome: ReconOutcome,
    pub summary: String,
    pub machine_reason: String,
    pub expected_fact_count: u32,
    pub observed_fact_count: u32,
    pub matched_fact_count: u32,
    pub unmatched_fact_count: u32,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub attempt_number: u32,
    pub retry_scheduled_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub exception_case_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconOutcomeRecord {
    pub outcome_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub lifecycle_state: ReconRunState,
    pub normalized_result: Option<ReconResult>,
    pub outcome: ReconOutcome,
    pub summary: String,
    pub machine_reason: String,
    pub details: BTreeMap<String, String>,
    pub exception_case_ids: Vec<String>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconReceipt {
    pub recon_receipt_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub normalized_result: Option<ReconResult>,
    pub outcome: ReconOutcome,
    pub summary: String,
    pub details: BTreeMap<String, String>,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconRunStateTransition {
    pub state_transition_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub from_state: Option<ReconRunState>,
    pub to_state: ReconRunState,
    pub reason: String,
    pub payload: Value,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconEvidenceSnapshot {
    pub evidence_snapshot_id: String,
    pub run_id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub lifecycle_state: ReconRunState,
    pub normalized_result: Option<ReconResult>,
    pub context: Value,
    pub adapter_rows: Value,
    pub expected_facts: Value,
    pub observed_facts: Value,
    pub match_result: Value,
    pub details: Value,
    pub exceptions: Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconOperatorActionRecord {
    pub action_id: String,
    pub subject_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub action_type: ReconOperatorActionType,
    pub actor: String,
    pub reason: String,
    pub payload: Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactMismatch {
    pub fact_key: String,
    pub mismatch_type: String,
    pub expected: Option<Value>,
    pub observed: Option<Value>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconMatchResult {
    pub matched_fact_keys: Vec<String>,
    pub missing_expected: Vec<String>,
    pub unexpected_observed: Vec<String>,
    pub mismatches: Vec<FactMismatch>,
}

#[derive(Debug, Clone, Default)]
pub struct ReconClassification {
    pub outcome: Option<ReconOutcome>,
    pub summary: Option<String>,
    pub machine_reason: Option<String>,
    pub details: BTreeMap<String, String>,
    pub exceptions: Vec<ExceptionDraft>,
}

#[derive(Debug, Clone)]
pub struct ReconEmission {
    pub outcome: ReconOutcome,
    pub summary: String,
    pub machine_reason: String,
    pub details: BTreeMap<String, String>,
    pub exceptions: Vec<ExceptionDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFactDraft {
    pub fact_type: String,
    pub fact_key: String,
    pub fact_value: Value,
    pub derived_from: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservedFactDraft {
    pub fact_type: String,
    pub fact_key: String,
    pub fact_value: Value,
    pub source_kind: String,
    pub source_table: Option<String>,
    pub source_id: Option<String>,
    pub metadata: Value,
    pub observed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconContext {
    pub latest_receipt: Option<Value>,
    pub latest_transition: Option<Value>,
    pub callback_delivery: Option<Value>,
    pub intent: Option<Value>,
    pub job: Option<Value>,
}

pub fn make_fact_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

pub fn normalize_result(outcome: ReconOutcome) -> ReconResult {
    match outcome {
        ReconOutcome::Matched | ReconOutcome::Resolved => ReconResult::Matched,
        ReconOutcome::PartiallyMatched => ReconResult::PartiallyMatched,
        ReconOutcome::Unmatched => ReconResult::Unmatched,
        ReconOutcome::Stale => ReconResult::Stale,
        ReconOutcome::ManualReviewRequired => ReconResult::ManualReviewRequired,
        ReconOutcome::Queued | ReconOutcome::CollectingObservations | ReconOutcome::Matching => {
            ReconResult::PendingObservation
        }
    }
}

pub fn make_exception(
    category: ExceptionCategory,
    severity: ExceptionSeverity,
    state: ExceptionState,
    summary: impl Into<String>,
    machine_reason: impl Into<String>,
    evidence: Vec<ExceptionEvidenceDraft>,
) -> ExceptionDraft {
    ExceptionDraft {
        category,
        severity,
        state,
        summary: summary.into(),
        machine_reason: machine_reason.into(),
        evidence,
    }
}

pub fn evidence(
    evidence_type: impl Into<String>,
    source_table: Option<String>,
    source_id: Option<String>,
    observed_at_ms: Option<u64>,
    payload: Value,
) -> ExceptionEvidenceDraft {
    ExceptionEvidenceDraft {
        evidence_type: evidence_type.into(),
        source_table,
        source_id,
        observed_at_ms,
        payload,
    }
}
