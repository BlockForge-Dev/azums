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
