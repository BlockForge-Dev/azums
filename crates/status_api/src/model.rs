use serde::Deserialize;
pub use shared_types::status_api::{
    CallbackDeliveryAttemptRecord, CallbackDeliveryRecord, CallbackDestinationRecord,
    CallbackDestinationResponse, CallbackHistoryResponse, DeleteCallbackDestinationResponse,
    HistoryResponse, IntakeAuditRecord, IntakeAuditsResponse, JobListItem, JobListResponse,
    ReceiptResponse, ReplayRequest, ReplayResponse, RequestStatusResponse,
    UpsertCallbackDestinationRequest, UpsertCallbackDestinationResponse,
};

#[derive(Debug, Clone, Deserialize)]
pub struct JobsQuery {
    pub state: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl JobsQuery {
    pub fn normalized_limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }

    pub fn normalized_offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallbackHistoryQuery {
    pub include_attempts: Option<bool>,
    pub attempt_limit: Option<u32>,
}

impl CallbackHistoryQuery {
    pub fn include_attempts(&self) -> bool {
        self.include_attempts.unwrap_or(true)
    }

    pub fn normalized_attempt_limit(&self) -> u32 {
        self.attempt_limit.unwrap_or(25).clamp(1, 200)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntakeAuditsQuery {
    pub validation_result: Option<String>,
    pub channel: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl IntakeAuditsQuery {
    pub fn normalized_validation_result(&self) -> Option<String> {
        self.validation_result
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    }

    pub fn normalized_channel(&self) -> Option<String> {
        self.channel
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    }

    pub fn normalized_limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }

    pub fn normalized_offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}
