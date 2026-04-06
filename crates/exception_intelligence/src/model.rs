use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExceptionCategory {
    ObservationMissing,
    StateMismatch,
    AmountMismatch,
    DestinationMismatch,
    DelayedVerification,
    DuplicateSignal,
    RepeatedRequestPattern,
    ExternalStateUnknown,
    PolicyViolation,
    ManualReviewRequired,
}

impl ExceptionCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObservationMissing => "observation_missing",
            Self::StateMismatch => "state_mismatch",
            Self::AmountMismatch => "amount_mismatch",
            Self::DestinationMismatch => "destination_mismatch",
            Self::DelayedVerification => "delayed_verification",
            Self::DuplicateSignal => "duplicate_signal",
            Self::RepeatedRequestPattern => "repeated_request_pattern",
            Self::ExternalStateUnknown => "external_state_unknown",
            Self::PolicyViolation => "policy_violation",
            Self::ManualReviewRequired => "manual_review_required",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "observation_missing" => Some(Self::ObservationMissing),
            "state_mismatch" => Some(Self::StateMismatch),
            "amount_mismatch" => Some(Self::AmountMismatch),
            "destination_mismatch" => Some(Self::DestinationMismatch),
            "delayed_verification" | "delayed_finality" => Some(Self::DelayedVerification),
            "duplicate_signal" => Some(Self::DuplicateSignal),
            "repeated_request_pattern" => Some(Self::RepeatedRequestPattern),
            "external_state_unknown" => Some(Self::ExternalStateUnknown),
            "policy_violation" => Some(Self::PolicyViolation),
            "manual_review_required" | "manual_review" => Some(Self::ManualReviewRequired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExceptionSeverity {
    Info,
    Warning,
    High,
    Critical,
}

impl ExceptionSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    pub fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExceptionState {
    Open,
    Acknowledged,
    Investigating,
    Resolved,
    Dismissed,
    FalsePositive,
}

impl ExceptionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Acknowledged => "acknowledged",
            Self::Investigating => "investigating",
            Self::Resolved => "resolved",
            Self::Dismissed => "dismissed",
            Self::FalsePositive => "false_positive",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "open" | "manual_review_required" => Some(Self::Open),
            "acknowledged" => Some(Self::Acknowledged),
            "investigating" => Some(Self::Investigating),
            "resolved" => Some(Self::Resolved),
            "dismissed" | "suppressed" => Some(Self::Dismissed),
            "false_positive" => Some(Self::FalsePositive),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Resolved | Self::Dismissed | Self::FalsePositive)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionEvidence {
    pub evidence_id: String,
    pub case_id: String,
    pub evidence_type: String,
    pub source_table: Option<String>,
    pub source_id: Option<String>,
    pub observed_at_ms: Option<u64>,
    pub payload: Value,
    pub created_at_ms: u64,
}

impl ExceptionEvidence {
    pub fn new(
        case_id: impl Into<String>,
        evidence_type: impl Into<String>,
        source_table: Option<String>,
        source_id: Option<String>,
        observed_at_ms: Option<u64>,
        payload: Value,
        created_at_ms: u64,
    ) -> Self {
        Self {
            evidence_id: format!("exev_{}", Uuid::new_v4().simple()),
            case_id: case_id.into(),
            evidence_type: evidence_type.into(),
            source_table,
            source_id,
            observed_at_ms,
            payload,
            created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionEvent {
    pub event_id: String,
    pub case_id: String,
    pub event_type: String,
    pub from_state: Option<ExceptionState>,
    pub to_state: Option<ExceptionState>,
    pub actor: String,
    pub reason: String,
    pub payload: Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionResolutionRecord {
    pub resolution_id: String,
    pub case_id: String,
    pub resolution_state: ExceptionState,
    pub actor: String,
    pub reason: String,
    pub payload: Value,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionCase {
    pub case_id: String,
    pub tenant_id: String,
    pub subject_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub category: ExceptionCategory,
    pub severity: ExceptionSeverity,
    pub state: ExceptionState,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionCaseDetail {
    pub case: ExceptionCase,
    pub evidence: Vec<ExceptionEvidence>,
    pub events: Vec<ExceptionEvent>,
    pub resolution_history: Vec<ExceptionResolutionRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExceptionSearchQuery {
    pub state: Option<String>,
    pub severity: Option<String>,
    pub category: Option<String>,
    pub adapter_id: Option<String>,
    pub subject_id: Option<String>,
    pub intent_id: Option<String>,
    pub cluster_key: Option<String>,
    pub search: Option<String>,
    pub include_terminal: bool,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionDraft {
    pub category: ExceptionCategory,
    pub severity: ExceptionSeverity,
    pub state: ExceptionState,
    pub summary: String,
    pub machine_reason: String,
    pub evidence: Vec<ExceptionEvidenceDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionEvidenceDraft {
    pub evidence_type: String,
    pub source_table: Option<String>,
    pub source_id: Option<String>,
    pub observed_at_ms: Option<u64>,
    pub payload: Value,
}

#[cfg(test)]
mod tests {
    use super::ExceptionCategory;

    #[test]
    fn exception_category_parse_accepts_new_and_legacy_tokens() {
        assert_eq!(
            ExceptionCategory::parse("delayed_verification"),
            Some(ExceptionCategory::DelayedVerification)
        );
        assert_eq!(
            ExceptionCategory::parse("delayed_finality"),
            Some(ExceptionCategory::DelayedVerification)
        );
        assert_eq!(
            ExceptionCategory::parse("repeated_request_pattern"),
            Some(ExceptionCategory::RepeatedRequestPattern)
        );
    }
}
