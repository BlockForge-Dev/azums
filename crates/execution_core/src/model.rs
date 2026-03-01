use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use uuid::Uuid;

pub type TimestampMs = u64;

macro_rules! id_wrapper {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);

        impl $name {
            pub fn new() -> Self {
                Self(format!("{}_{}", $prefix, Uuid::new_v4().simple()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_wrapper!(TenantId, "tenant");
id_wrapper!(RequestId, "req");
id_wrapper!(IntentId, "intent");
id_wrapper!(JobId, "job");
id_wrapper!(LeaseId, "lease");
id_wrapper!(AdapterId, "adapter");
id_wrapper!(TransitionId, "transition");
id_wrapper!(ReceiptId, "receipt");
id_wrapper!(CallbackId, "callback");
id_wrapper!(ReplayDecisionId, "replay");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IntentKind(pub String);

impl IntentKind {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IntentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalState {
    #[serde(alias = "Submitted", alias = "submitted")]
    Received,
    #[serde(alias = "Validated", alias = "validated")]
    Validated,
    #[serde(alias = "Rejected", alias = "rejected")]
    Rejected,
    #[serde(alias = "Routed", alias = "routed")]
    Queued,
    #[serde(alias = "Leased", alias = "leased")]
    Leased,
    #[serde(alias = "Dispatching", alias = "dispatching")]
    Executing,
    #[serde(alias = "RetryScheduled", alias = "retryscheduled")]
    RetryScheduled,
    #[serde(alias = "Succeeded", alias = "succeeded")]
    Succeeded,
    #[serde(
        alias = "TerminalFailure",
        alias = "terminalfailure",
        alias = "Blocked",
        alias = "blocked",
        alias = "ManualReview",
        alias = "manualreview"
    )]
    FailedTerminal,
    #[serde(alias = "DeadLettered", alias = "deadlettered")]
    DeadLettered,
    #[serde(alias = "Replayed", alias = "replayed")]
    Replayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformClassification {
    Success,
    RetryableFailure,
    TerminalFailure,
    Blocked,
    ManualReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    #[serde(default)]
    pub principal_id: Option<String>,
    #[serde(default)]
    pub submitter_kind: Option<String>,
    #[serde(default)]
    pub auth_scheme: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedIntent {
    #[serde(default)]
    pub request_id: Option<RequestId>,
    pub intent_id: IntentId,
    pub tenant_id: TenantId,
    pub kind: IntentKind,
    pub payload: Value,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub auth_context: Option<AuthContext>,
    pub metadata: BTreeMap<String, String>,
    pub received_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRoute {
    pub adapter_id: AdapterId,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureInfo {
    pub code: String,
    pub message: String,
    pub classification: PlatformClassification,
    #[serde(default)]
    pub caller_can_fix: bool,
    #[serde(default)]
    pub operator_can_fix: bool,
    pub retry_after_ms: Option<u64>,
    pub provider_details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionJob {
    pub job_id: JobId,
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub adapter_id: AdapterId,
    pub state: CanonicalState,
    pub attempt: u32,
    pub max_attempts: u32,
    pub replay_count: u32,
    #[serde(default)]
    pub replay_of_job_id: Option<JobId>,
    pub next_retry_at_ms: Option<TimestampMs>,
    pub last_failure: Option<FailureInfo>,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
}

impl ExecutionJob {
    pub fn new(
        tenant_id: TenantId,
        intent_id: IntentId,
        adapter_id: AdapterId,
        max_attempts: u32,
        now_ms: TimestampMs,
    ) -> Self {
        Self {
            job_id: JobId::new(),
            tenant_id,
            intent_id,
            adapter_id,
            state: CanonicalState::Received,
            attempt: 0,
            max_attempts,
            replay_count: 0,
            replay_of_job_id: None,
            next_retry_at_ms: None,
            last_failure: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeasedJob {
    pub lease_id: LeaseId,
    pub job: ExecutionJob,
    pub leased_at_ms: TimestampMs,
    pub lease_expires_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterExecutionRequest {
    #[serde(default)]
    pub request_id: Option<RequestId>,
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub job_id: JobId,
    pub adapter_id: AdapterId,
    pub attempt: u32,
    pub intent_kind: IntentKind,
    pub payload: Value,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub auth_context: Option<AuthContext>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterOutcome {
    InProgress {
        provider_reference: Option<String>,
        details: BTreeMap<String, String>,
        poll_after_ms: Option<u64>,
    },
    Succeeded {
        provider_reference: Option<String>,
        details: BTreeMap<String, String>,
    },
    RetryableFailure {
        code: String,
        message: String,
        retry_after_ms: Option<u64>,
        provider_details: Option<Value>,
    },
    TerminalFailure {
        code: String,
        message: String,
        provider_details: Option<Value>,
    },
    Blocked {
        code: String,
        message: String,
    },
    ManualReview {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransitionActor {
    System,
    Adapter(AdapterId),
    Operator(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    pub transition_id: TransitionId,
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub job_id: JobId,
    pub from_state: Option<CanonicalState>,
    pub to_state: CanonicalState,
    pub classification: PlatformClassification,
    pub reason_code: String,
    pub reason: String,
    pub adapter_id: Option<AdapterId>,
    pub actor: TransitionActor,
    pub occurred_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptEntry {
    pub receipt_id: ReceiptId,
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub job_id: JobId,
    #[serde(default)]
    pub attempt_no: u32,
    pub state: CanonicalState,
    pub classification: PlatformClassification,
    pub summary: String,
    pub details: BTreeMap<String, String>,
    pub occurred_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatorRole {
    Viewer,
    Operator,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorPrincipal {
    pub principal_id: String,
    pub role: OperatorRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCommand {
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub requested_by: OperatorPrincipal,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayDecisionRecord {
    pub replay_decision_id: ReplayDecisionId,
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub source_job_id: JobId,
    pub allowed: bool,
    pub reason: String,
    pub requested_by: String,
    pub occurred_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub tenant_id: TenantId,
    pub intent_id: IntentId,
    pub job_id: JobId,
    pub adapter_id: AdapterId,
    pub state: CanonicalState,
    pub classification: PlatformClassification,
    pub updated_at_ms: TimestampMs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackJob {
    pub callback_id: CallbackId,
    pub summary: StatusSummary,
    pub enqueued_at_ms: TimestampMs,
}

pub fn is_terminal_state(state: CanonicalState) -> bool {
    matches!(
        state,
        CanonicalState::Succeeded
            | CanonicalState::Rejected
            | CanonicalState::FailedTerminal
            | CanonicalState::DeadLettered
    )
}
