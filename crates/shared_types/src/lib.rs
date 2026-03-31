pub mod status_api {
    use execution_core::{
        AuthContext, CanonicalState, FailureInfo, PlatformClassification, ReceiptEntry,
        StateTransition,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RequestStatusResponse {
        pub tenant_id: String,
        pub request_id: String,
        pub intent_id: String,
        pub kind: Option<String>,
        pub correlation_id: Option<String>,
        pub idempotency_key: Option<String>,
        pub auth_context: Option<AuthContext>,
        pub state: CanonicalState,
        pub classification: PlatformClassification,
        pub adapter_id: String,
        pub attempt: u32,
        pub max_attempts: u32,
        pub replay_count: u32,
        pub replay_of_job_id: Option<String>,
        pub last_failure: Option<FailureInfo>,
        pub created_at_ms: u64,
        pub updated_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReceiptResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub entries: Vec<ReceiptEntry>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReceiptLookupResponse {
        pub tenant_id: String,
        pub receipt_id: String,
        pub intent_id: String,
        pub entry: ReceiptEntry,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HistoryResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub transitions: Vec<StateTransition>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackHistoryResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub callbacks: Vec<CallbackDeliveryRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackDetailResponse {
        pub ok: bool,
        pub callback_id: String,
        pub intent_id: String,
        pub callback: CallbackDeliveryRecord,
        pub request: RequestStatusResponse,
        pub receipt: ReceiptResponse,
        pub history: HistoryResponse,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackDestinationResponse {
        pub tenant_id: String,
        pub configured: bool,
        pub destination: Option<CallbackDestinationRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackDestinationRecord {
        pub delivery_url: String,
        pub timeout_ms: u64,
        pub allow_private_destinations: bool,
        pub allowed_hosts: Vec<String>,
        pub enabled: bool,
        pub has_bearer_token: bool,
        pub has_signature_secret: bool,
        pub signature_key_id: Option<String>,
        pub updated_by_principal_id: String,
        pub created_at_ms: u64,
        pub updated_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UpsertCallbackDestinationRequest {
        pub delivery_url: String,
        pub bearer_token: Option<String>,
        pub signature_secret: Option<String>,
        pub signature_key_id: Option<String>,
        pub timeout_ms: Option<u64>,
        pub allow_private_destinations: Option<bool>,
        pub allowed_hosts: Option<Vec<String>>,
        pub enabled: Option<bool>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UpsertCallbackDestinationResponse {
        pub tenant_id: String,
        pub updated: bool,
        pub destination: CallbackDestinationRecord,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DeleteCallbackDestinationResponse {
        pub tenant_id: String,
        pub deleted: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackDeliveryRecord {
        pub callback_id: String,
        pub state: String,
        pub attempts: u32,
        pub last_http_status: Option<u16>,
        pub last_error_class: Option<String>,
        pub last_error_message: Option<String>,
        pub next_attempt_at_ms: Option<u64>,
        pub delivered_at_ms: Option<u64>,
        pub updated_at_ms: u64,
        pub attempt_history: Vec<CallbackDeliveryAttemptRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CallbackDeliveryAttemptRecord {
        pub attempt_no: u32,
        pub outcome: String,
        pub failure_class: Option<String>,
        pub error_message: Option<String>,
        pub http_status: Option<u16>,
        pub response_excerpt: Option<String>,
        pub occurred_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct JobListResponse {
        pub tenant_id: String,
        pub jobs: Vec<JobListItem>,
        pub limit: u32,
        pub offset: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct JobListItem {
        pub job_id: String,
        pub intent_id: String,
        pub adapter_id: String,
        pub state: CanonicalState,
        pub classification: PlatformClassification,
        pub attempt: u32,
        pub max_attempts: u32,
        pub replay_count: u32,
        pub replay_of_job_id: Option<String>,
        pub next_retry_at_ms: Option<u64>,
        pub updated_at_ms: u64,
        pub created_at_ms: u64,
        pub failure_code: Option<String>,
        pub failure_message: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IntakeAuditsResponse {
        pub tenant_id: String,
        pub audits: Vec<IntakeAuditRecord>,
        pub limit: u32,
        pub offset: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct IntakeAuditRecord {
        pub audit_id: String,
        pub request_id: String,
        pub channel: String,
        pub endpoint: String,
        pub method: String,
        pub principal_id: Option<String>,
        pub submitter_kind: Option<String>,
        pub auth_scheme: Option<String>,
        pub intent_kind: Option<String>,
        pub correlation_id: Option<String>,
        pub idempotency_key: Option<String>,
        pub idempotency_decision: Option<String>,
        pub validation_result: String,
        pub rejection_reason: Option<String>,
        pub error_status: Option<u16>,
        pub error_message: Option<String>,
        pub accepted_intent_id: Option<String>,
        pub accepted_job_id: Option<String>,
        pub details_json: Value,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReplayRequest {
        pub reason: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReplayResponse {
        pub source_job_id: String,
        pub replay_job_id: String,
        pub replay_count: u32,
        pub state: CanonicalState,
        pub route_adapter_id: String,
        pub details: BTreeMap<String, String>,
    }
}

pub mod reconciliation {
    use super::status_api::{
        CallbackHistoryResponse, HistoryResponse, ReceiptResponse, RequestStatusResponse,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationSubjectRecord {
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
        pub last_run_state: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRunRecord {
        pub run_id: String,
        pub subject_id: String,
        pub tenant_id: String,
        pub intent_id: String,
        pub job_id: String,
        pub adapter_id: String,
        pub rule_pack: String,
        pub lifecycle_state: String,
        pub normalized_result: Option<String>,
        pub outcome: String,
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
    pub struct ReconciliationReceiptRecord {
        pub recon_receipt_id: String,
        pub run_id: String,
        pub subject_id: String,
        pub normalized_result: Option<String>,
        pub outcome: String,
        pub summary: String,
        pub details: BTreeMap<String, String>,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationFactRecord {
        pub fact_id: String,
        pub run_id: String,
        pub subject_id: String,
        pub fact_type: String,
        pub fact_key: String,
        pub fact_value: Value,
        pub source_kind: Option<String>,
        pub source_table: Option<String>,
        pub source_id: Option<String>,
        pub metadata: Value,
        pub observed_at_ms: Option<u64>,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RequestReconciliationResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub subject: Option<ReconciliationSubjectRecord>,
        pub runs: Vec<ReconciliationRunRecord>,
        pub latest_receipt: Option<ReconciliationReceiptRecord>,
        pub expected_facts: Vec<ReconciliationFactRecord>,
        pub observed_facts: Vec<ReconciliationFactRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionEvidenceRecord {
        pub evidence_id: String,
        pub case_id: String,
        pub evidence_type: String,
        pub source_table: Option<String>,
        pub source_id: Option<String>,
        pub observed_at_ms: Option<u64>,
        pub payload: Value,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionCaseRecord {
        pub case_id: String,
        pub tenant_id: String,
        pub subject_id: String,
        pub intent_id: String,
        pub job_id: String,
        pub adapter_id: String,
        pub category: String,
        pub severity: String,
        pub state: String,
        pub summary: String,
        pub machine_reason: String,
        pub dedupe_key: String,
        pub cluster_key: String,
        pub first_seen_at_ms: u64,
        pub last_seen_at_ms: u64,
        pub occurrence_count: u64,
        pub created_at_ms: u64,
        pub updated_at_ms: u64,
        pub resolved_at_ms: Option<u64>,
        pub latest_run_id: Option<String>,
        pub latest_outcome_id: Option<String>,
        pub latest_recon_receipt_id: Option<String>,
        pub latest_execution_receipt_id: Option<String>,
        pub latest_evidence_snapshot_id: Option<String>,
        pub last_actor: Option<String>,
        pub evidence: Vec<ExceptionEvidenceRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionEventRecord {
        pub event_id: String,
        pub case_id: String,
        pub event_type: String,
        pub from_state: Option<String>,
        pub to_state: Option<String>,
        pub actor: String,
        pub reason: String,
        pub payload: Value,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionResolutionRecord {
        pub resolution_id: String,
        pub case_id: String,
        pub resolution_state: String,
        pub actor: String,
        pub reason: String,
        pub payload: Value,
        pub created_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionIndexResponse {
        pub tenant_id: String,
        pub cases: Vec<ExceptionCaseRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionDetailResponse {
        pub tenant_id: String,
        pub case: ExceptionCaseRecord,
        pub events: Vec<ExceptionEventRecord>,
        pub resolution_history: Vec<ExceptionResolutionRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionStateTransitionRequest {
        pub state: String,
        pub reason: String,
        pub payload: Option<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionStateTransitionResponse {
        pub ok: bool,
        pub case: ExceptionCaseRecord,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct OperatorActionRequest {
        pub reason: String,
        pub payload: Option<Value>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconActionResponse {
        pub ok: bool,
        pub action: String,
        pub action_id: String,
        pub subject: ReconciliationSubjectRecord,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ExceptionActionResponse {
        pub ok: bool,
        pub action: String,
        pub case: ExceptionCaseRecord,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReplayReviewResponse {
        pub ok: bool,
        pub handoff: String,
        pub replay: super::status_api::ReplayResponse,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutWindow {
        pub lookback_hours: u32,
        pub started_at_ms: u64,
        pub generated_at_ms: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutIntakeMetrics {
        pub eligible_execution_receipts: u64,
        pub intake_signals: u64,
        pub subjects_total: u64,
        pub dirty_subjects: u64,
        pub retry_scheduled_subjects: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutOutcomeMetrics {
        pub matched: u64,
        pub partially_matched: u64,
        pub unmatched: u64,
        pub pending_observation: u64,
        pub stale: u64,
        pub manual_review_required: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutExceptionMetrics {
        pub total_cases: u64,
        pub unresolved_cases: u64,
        pub high_or_critical_cases: u64,
        pub false_positive_cases: u64,
        pub exception_rate: f64,
        pub false_positive_rate: f64,
        pub stale_rate: f64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutLatencyMetrics {
        pub avg_recon_latency_ms: Option<u64>,
        pub p95_recon_latency_ms: Option<u64>,
        pub max_recon_latency_ms: Option<u64>,
        pub avg_operator_handling_ms: Option<u64>,
        pub p95_operator_handling_ms: Option<u64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutQueryMetrics {
        pub sampled_intent_id: Option<String>,
        pub exception_index_query_ms: Option<u64>,
        pub unified_request_query_ms: Option<u64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReconciliationRolloutSummaryResponse {
        pub tenant_id: String,
        pub window: ReconciliationRolloutWindow,
        pub intake: ReconciliationRolloutIntakeMetrics,
        pub outcomes: ReconciliationRolloutOutcomeMetrics,
        pub exceptions: ReconciliationRolloutExceptionMetrics,
        pub latency: ReconciliationRolloutLatencyMetrics,
        pub queries: ReconciliationRolloutQueryMetrics,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RequestExceptionsResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub cases: Vec<ExceptionCaseRecord>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UnifiedExceptionSummary {
        pub total_cases: u32,
        pub unresolved_cases: u32,
        pub highest_severity: Option<String>,
        pub categories: Vec<String>,
        pub open_case_ids: Vec<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UnifiedEvidenceReferenceRecord {
        pub kind: String,
        pub label: String,
        pub source_table: Option<String>,
        pub source_id: Option<String>,
        pub observed_at_ms: Option<u64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct UnifiedRequestStatusResponse {
        pub tenant_id: String,
        pub intent_id: String,
        pub request: RequestStatusResponse,
        pub receipt: ReceiptResponse,
        pub history: HistoryResponse,
        pub callbacks: CallbackHistoryResponse,
        pub reconciliation: RequestReconciliationResponse,
        pub exceptions: RequestExceptionsResponse,
        pub dashboard_status: String,
        pub recon_status: Option<String>,
        pub reconciliation_eligible: bool,
        pub latest_execution_receipt_id: Option<String>,
        pub latest_recon_receipt_id: Option<String>,
        pub latest_evidence_snapshot_id: Option<String>,
        pub exception_summary: UnifiedExceptionSummary,
        pub evidence_references: Vec<UnifiedEvidenceReferenceRecord>,
    }
}
