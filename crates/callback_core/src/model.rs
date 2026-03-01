use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryState {
    Queued,
    Delivering,
    RetryScheduled,
    Delivered,
    TerminalFailure,
}

impl DeliveryState {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryState::Queued => "queued",
            DeliveryState::Delivering => "delivering",
            DeliveryState::RetryScheduled => "retry_scheduled",
            DeliveryState::Delivered => "delivered",
            DeliveryState::TerminalFailure => "terminal_failure",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(DeliveryState::Queued),
            "delivering" => Some(DeliveryState::Delivering),
            "retry_scheduled" => Some(DeliveryState::RetryScheduled),
            "delivered" => Some(DeliveryState::Delivered),
            "terminal_failure" => Some(DeliveryState::TerminalFailure),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryAttemptOutcome {
    Succeeded,
    FailedRetryable,
    FailedTerminal,
    SkippedDuplicate,
}

impl DeliveryAttemptOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryAttemptOutcome::Succeeded => "succeeded",
            DeliveryAttemptOutcome::FailedRetryable => "failed_retryable",
            DeliveryAttemptOutcome::FailedTerminal => "failed_terminal",
            DeliveryAttemptOutcome::SkippedDuplicate => "skipped_duplicate",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "succeeded" => Some(DeliveryAttemptOutcome::Succeeded),
            "failed_retryable" => Some(DeliveryAttemptOutcome::FailedRetryable),
            "failed_terminal" => Some(DeliveryAttemptOutcome::FailedTerminal),
            "skipped_duplicate" => Some(DeliveryAttemptOutcome::SkippedDuplicate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryFailureClass {
    Transport,
    Timeout,
    Http4xx,
    Http5xx,
    InvalidDestination,
    DestinationBlocked,
    Serialization,
    Internal,
}

impl DeliveryFailureClass {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryFailureClass::Transport => "transport",
            DeliveryFailureClass::Timeout => "timeout",
            DeliveryFailureClass::Http4xx => "http_4xx",
            DeliveryFailureClass::Http5xx => "http_5xx",
            DeliveryFailureClass::InvalidDestination => "invalid_destination",
            DeliveryFailureClass::DestinationBlocked => "destination_blocked",
            DeliveryFailureClass::Serialization => "serialization",
            DeliveryFailureClass::Internal => "internal",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "transport" => Some(DeliveryFailureClass::Transport),
            "timeout" => Some(DeliveryFailureClass::Timeout),
            "http_4xx" => Some(DeliveryFailureClass::Http4xx),
            "http_5xx" => Some(DeliveryFailureClass::Http5xx),
            "invalid_destination" => Some(DeliveryFailureClass::InvalidDestination),
            "destination_blocked" => Some(DeliveryFailureClass::DestinationBlocked),
            "serialization" => Some(DeliveryFailureClass::Serialization),
            "internal" => Some(DeliveryFailureClass::Internal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOutcome {
    pub http_status: u16,
    pub response_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchFailure {
    pub class: DeliveryFailureClass,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub http_status: Option<u16>,
    pub retry_after_secs: Option<i64>,
    pub response_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStatus {
    pub callback_id: String,
    pub tenant_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub state: DeliveryState,
    pub attempts: u32,
    pub last_http_status: Option<u16>,
    pub last_error_class: Option<DeliveryFailureClass>,
    pub last_error_message: Option<String>,
    pub next_attempt_at_ms: Option<u64>,
    pub delivered_at_ms: Option<u64>,
    pub first_seen_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub attempt_id: Uuid,
    pub callback_id: String,
    pub attempt_no: u32,
    pub outcome: DeliveryAttemptOutcome,
    pub failure_class: Option<DeliveryFailureClass>,
    pub error_message: Option<String>,
    pub http_status: Option<u16>,
    pub response_excerpt: Option<String>,
    pub occurred_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantCallbackDestination {
    pub tenant_id: String,
    pub delivery_url: String,
    pub bearer_token: Option<String>,
    pub signature_secret: Option<String>,
    pub signature_key_id: Option<String>,
    pub timeout_ms: u64,
    pub allow_private_destinations: bool,
    pub allowed_hosts: Option<String>,
    pub enabled: bool,
    pub updated_by_principal_id: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
