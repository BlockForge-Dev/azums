use serde::Deserialize;
pub use shared_types::reconciliation::{
    ExceptionActionResponse, ExceptionCaseRecord, ExceptionDetailResponse, ExceptionEventRecord,
    ExceptionEvidenceRecord, ExceptionIndexResponse, ExceptionResolutionRecord,
    ExceptionStateTransitionRequest, ExceptionStateTransitionResponse, OperatorActionRequest,
    ReconActionResponse, ReconciliationFactRecord, ReconciliationReceiptRecord,
    ReconciliationRolloutExceptionMetrics, ReconciliationRolloutIntakeMetrics,
    ReconciliationRolloutLatencyMetrics, ReconciliationRolloutOutcomeMetrics,
    ReconciliationRolloutQueryMetrics, ReconciliationRolloutSummaryResponse,
    ReconciliationRolloutWindow, ReconciliationRunRecord, ReconciliationSubjectRecord,
    ReplayReviewResponse, RequestExceptionsResponse, RequestReconciliationResponse,
    UnifiedEvidenceReferenceRecord, UnifiedExceptionSummary, UnifiedRequestStatusResponse,
};
pub use shared_types::status_api::{
    CallbackDeliveryAttemptRecord, CallbackDeliveryRecord, CallbackDestinationRecord,
    CallbackDestinationResponse, CallbackDetailResponse, CallbackHistoryResponse,
    DeleteCallbackDestinationResponse, HistoryResponse, IntakeAuditRecord, IntakeAuditsResponse,
    JobListItem, JobListResponse, ReceiptLookupResponse, ReceiptResponse, ReplayRequest,
    ReplayResponse, RequestStatusResponse, UpsertCallbackDestinationRequest,
    UpsertCallbackDestinationResponse,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ExceptionIndexQuery {
    pub state: Option<String>,
    pub severity: Option<String>,
    pub category: Option<String>,
    pub adapter_id: Option<String>,
    pub subject_id: Option<String>,
    pub intent_id: Option<String>,
    pub cluster_key: Option<String>,
    pub search: Option<String>,
    pub include_terminal: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl ExceptionIndexQuery {
    fn normalize(value: &Option<String>) -> Option<String> {
        value
            .as_ref()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    }

    pub fn normalized_state(&self) -> Option<String> {
        Self::normalize(&self.state)
    }

    pub fn normalized_severity(&self) -> Option<String> {
        Self::normalize(&self.severity)
    }

    pub fn normalized_category(&self) -> Option<String> {
        Self::normalize(&self.category)
    }

    pub fn normalized_adapter_id(&self) -> Option<String> {
        Self::normalize(&self.adapter_id)
    }

    pub fn normalized_subject_id(&self) -> Option<String> {
        Self::normalize(&self.subject_id)
    }

    pub fn normalized_intent_id(&self) -> Option<String> {
        Self::normalize(&self.intent_id)
    }

    pub fn normalized_cluster_key(&self) -> Option<String> {
        Self::normalize(&self.cluster_key)
    }

    pub fn normalized_search(&self) -> Option<String> {
        self.search
            .as_ref()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    pub fn include_terminal(&self) -> bool {
        self.include_terminal.unwrap_or(false)
    }

    pub fn normalized_limit(&self) -> u32 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }

    pub fn normalized_offset(&self) -> u32 {
        self.offset.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RolloutSummaryQuery {
    pub lookback_hours: Option<u32>,
}

impl RolloutSummaryQuery {
    pub fn normalized_lookback_hours(&self) -> u32 {
        self.lookback_hours.unwrap_or(168).clamp(1, 24 * 30)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExceptionIndexQuery, RolloutSummaryQuery};

    #[test]
    fn exception_index_query_normalizes_filters_and_bounds() {
        let query = ExceptionIndexQuery {
            state: Some("  Open ".to_owned()),
            severity: Some(" HIGH ".to_owned()),
            category: Some(" Manual_Review ".to_owned()),
            adapter_id: Some(" Solana ".to_owned()),
            subject_id: Some(" Subject_A ".to_owned()),
            intent_id: Some(" Intent_A ".to_owned()),
            cluster_key: Some(" Cluster_A ".to_owned()),
            search: Some("  pending too long  ".to_owned()),
            include_terminal: Some(true),
            limit: Some(999),
            offset: Some(7),
        };

        assert_eq!(query.normalized_state().as_deref(), Some("open"));
        assert_eq!(query.normalized_severity().as_deref(), Some("high"));
        assert_eq!(
            query.normalized_category().as_deref(),
            Some("manual_review")
        );
        assert_eq!(query.normalized_adapter_id().as_deref(), Some("solana"));
        assert_eq!(query.normalized_subject_id().as_deref(), Some("subject_a"));
        assert_eq!(query.normalized_intent_id().as_deref(), Some("intent_a"));
        assert_eq!(query.normalized_cluster_key().as_deref(), Some("cluster_a"));
        assert_eq!(
            query.normalized_search().as_deref(),
            Some("pending too long")
        );
        assert!(query.include_terminal());
        assert_eq!(query.normalized_limit(), 200);
        assert_eq!(query.normalized_offset(), 7);
    }

    #[test]
    fn rollout_summary_query_clamps_lookback_window() {
        let default_query = RolloutSummaryQuery {
            lookback_hours: None,
        };
        let tiny_query = RolloutSummaryQuery {
            lookback_hours: Some(0),
        };
        let large_query = RolloutSummaryQuery {
            lookback_hours: Some(24 * 90),
        };

        assert_eq!(default_query.normalized_lookback_hours(), 168);
        assert_eq!(tiny_query.normalized_lookback_hours(), 1);
        assert_eq!(large_query.normalized_lookback_hours(), 24 * 30);
    }
}
