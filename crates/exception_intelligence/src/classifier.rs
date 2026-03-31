use crate::model::{
    ExceptionCategory, ExceptionDraft, ExceptionEvidenceDraft, ExceptionSeverity, ExceptionState,
};

#[derive(Debug, Clone, Default)]
pub struct ExceptionContext {
    pub tenant_id: String,
    pub subject_id: String,
    pub intent_id: String,
    pub job_id: String,
    pub adapter_id: String,
    pub latest_run_id: Option<String>,
    pub latest_outcome_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClassifiedExceptionDraft {
    pub category: ExceptionCategory,
    pub severity: ExceptionSeverity,
    pub state: ExceptionState,
    pub summary: String,
    pub machine_reason: String,
    pub dedupe_key: String,
    pub cluster_key: String,
    pub evidence: Vec<ExceptionEvidenceDraft>,
}

#[derive(Debug, Clone, Default)]
pub struct ExceptionClassifier;

impl ExceptionClassifier {
    pub fn classify(
        &self,
        context: &ExceptionContext,
        draft: &ExceptionDraft,
    ) -> ClassifiedExceptionDraft {
        let category = draft.category;
        let severity = severity_floor(category).max(draft.severity);
        let state = normalize_active_state(draft.state);
        let machine_reason = normalize_token(&draft.machine_reason);
        let dedupe_key = [
            context.subject_id.as_str(),
            context.adapter_id.as_str(),
            category.as_str(),
            machine_reason.as_str(),
        ]
        .into_iter()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join("|");
        let cluster_key = [
            context.adapter_id.as_str(),
            category.as_str(),
            machine_reason.as_str(),
        ]
        .into_iter()
        .map(normalize_token)
        .collect::<Vec<_>>()
        .join("|");

        ClassifiedExceptionDraft {
            category,
            severity,
            state,
            summary: draft.summary.trim().to_owned(),
            machine_reason,
            dedupe_key,
            cluster_key,
            evidence: draft.evidence.clone(),
        }
    }
}

fn severity_floor(category: ExceptionCategory) -> ExceptionSeverity {
    match category {
        ExceptionCategory::ObservationMissing | ExceptionCategory::DelayedFinality => {
            ExceptionSeverity::Warning
        }
        ExceptionCategory::PolicyViolation => ExceptionSeverity::Critical,
        ExceptionCategory::StateMismatch
        | ExceptionCategory::AmountMismatch
        | ExceptionCategory::DestinationMismatch
        | ExceptionCategory::DuplicateSignal
        | ExceptionCategory::ExternalStateUnknown
        | ExceptionCategory::ManualReviewRequired => ExceptionSeverity::High,
    }
}

fn normalize_active_state(state: ExceptionState) -> ExceptionState {
    match state {
        ExceptionState::Acknowledged => ExceptionState::Acknowledged,
        ExceptionState::Investigating => ExceptionState::Investigating,
        ExceptionState::Open
        | ExceptionState::Resolved
        | ExceptionState::Dismissed
        | ExceptionState::FalsePositive => ExceptionState::Open,
    }
}

fn normalize_token(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .trim()
        .to_ascii_lowercase()
        .replace(' ', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExceptionDraft, ExceptionSeverity};

    #[test]
    fn classifier_applies_severity_floor_and_normalized_keys() {
        let classifier = ExceptionClassifier;
        let draft = ExceptionDraft {
            category: ExceptionCategory::PolicyViolation,
            severity: ExceptionSeverity::Warning,
            state: ExceptionState::Resolved,
            summary: "  policy blocked  ".to_owned(),
            machine_reason: "Policy Blocked".to_owned(),
            evidence: Vec::new(),
        };
        let classified = classifier.classify(
            &ExceptionContext {
                tenant_id: "tenant_a".to_owned(),
                subject_id: "subject_a".to_owned(),
                intent_id: "intent_a".to_owned(),
                job_id: "job_a".to_owned(),
                adapter_id: "solana".to_owned(),
                latest_run_id: Some("run_1".to_owned()),
                latest_outcome_id: Some("outcome_1".to_owned()),
            },
            &draft,
        );

        assert_eq!(classified.severity, ExceptionSeverity::Critical);
        assert_eq!(classified.state, ExceptionState::Open);
        assert_eq!(
            classified.dedupe_key,
            "subject_a|solana|policy_violation|policy_blocked"
        );
        assert_eq!(
            classified.cluster_key,
            "solana|policy_violation|policy_blocked"
        );
    }
}
