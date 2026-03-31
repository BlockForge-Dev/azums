use crate::error::ReconError;
use crate::model::ReconSubject;
use async_trait::async_trait;
use execution_core::ReconIntakeSignal;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ReconIntakeResult {
    pub subject: ReconSubject,
    pub duplicate_signal: bool,
}

#[async_trait]
pub trait ReconIntakeRepository: Send + Sync {
    async fn claim_intake_signal(&self, signal: &ReconIntakeSignal) -> Result<bool, ReconError>;
    async fn materialize_subject_from_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<ReconSubject, ReconError>;
    async fn load_subject_for_execution(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
    ) -> Result<Option<ReconSubject>, ReconError>;
}

pub struct ReconIntakeService<R> {
    repo: Arc<R>,
}

impl<R> ReconIntakeService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }
}

impl<R> ReconIntakeService<R>
where
    R: ReconIntakeRepository,
{
    pub async fn ingest_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<ReconIntakeResult, ReconError> {
        let accepted = self.repo.claim_intake_signal(signal).await?;
        if !accepted {
            let subject = self
                .repo
                .load_subject_for_execution(
                    signal.tenant_id.as_str(),
                    signal.intent_id.as_str(),
                    signal.job_id.as_str(),
                )
                .await?
                .ok_or_else(|| {
                    ReconError::Backend(format!(
                        "duplicate recon intake signal `{}` has no materialized subject",
                        signal.signal_id
                    ))
                })?;
            return Ok(ReconIntakeResult {
                subject,
                duplicate_signal: true,
            });
        }

        let subject = self.repo.materialize_subject_from_signal(signal).await?;
        Ok(ReconIntakeResult {
            subject,
            duplicate_signal: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ReconSubject;
    use execution_core::{
        CanonicalState, CallbackId, PlatformClassification, ReconIntakeSignal,
        ReconIntakeSignalId, ReconIntakeSignalKind, TenantId,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRepo {
        seen_signals: Mutex<HashSet<String>>,
        subjects: Mutex<HashMap<(String, String, String), ReconSubject>>,
    }

    #[async_trait]
    impl ReconIntakeRepository for FakeRepo {
        async fn claim_intake_signal(&self, signal: &ReconIntakeSignal) -> Result<bool, ReconError> {
            let mut seen = self.seen_signals.lock().unwrap();
            Ok(seen.insert(signal.signal_id.to_string()))
        }

        async fn materialize_subject_from_signal(
            &self,
            signal: &ReconIntakeSignal,
        ) -> Result<ReconSubject, ReconError> {
            let key = (
                signal.tenant_id.to_string(),
                signal.intent_id.to_string(),
                signal.job_id.to_string(),
            );
            let mut subjects = self.subjects.lock().unwrap();
            let subject = subjects
                .entry(key)
                .or_insert_with(|| ReconSubject {
                    subject_id: signal.recon_subject_id.clone(),
                    tenant_id: signal.tenant_id.to_string(),
                    intent_id: signal.intent_id.to_string(),
                    job_id: signal.job_id.to_string(),
                    adapter_id: signal
                        .adapter_id
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "adapter_solana".to_owned()),
                    canonical_state: signal
                        .canonical_state
                        .map(|state| format!("{state:?}"))
                        .unwrap_or_else(|| "Succeeded".to_owned()),
                    platform_classification: signal
                        .classification
                        .map(|value| format!("{value:?}"))
                        .unwrap_or_else(|| "Success".to_owned()),
                    latest_receipt_id: signal.receipt_id.as_ref().map(ToString::to_string),
                    latest_transition_id: signal.transition_id.as_ref().map(ToString::to_string),
                    latest_callback_id: signal.callback_id.as_ref().map(ToString::to_string),
                    latest_signal_id: Some(signal.signal_id.to_string()),
                    latest_signal_kind: Some(signal.signal_kind.as_str().to_owned()),
                    execution_correlation_id: signal.execution_correlation_id.clone(),
                    adapter_execution_reference: signal.adapter_execution_reference.clone(),
                    external_observation_key: signal.external_observation_key.clone(),
                    expected_fact_snapshot: signal.expected_fact_snapshot.clone(),
                    dirty: true,
                    recon_attempt_count: 0,
                    recon_retry_count: 0,
                    created_at_ms: signal.occurred_at_ms,
                    updated_at_ms: signal.occurred_at_ms,
                    scheduled_at_ms: Some(signal.occurred_at_ms),
                    next_reconcile_after_ms: Some(signal.occurred_at_ms),
                    last_reconciled_at_ms: None,
                    last_recon_error: None,
                    last_run_state: None,
                })
                .clone();
            Ok(subject)
        }

        async fn load_subject_for_execution(
            &self,
            tenant_id: &str,
            intent_id: &str,
            job_id: &str,
        ) -> Result<Option<ReconSubject>, ReconError> {
            Ok(self
                .subjects
                .lock()
                .unwrap()
                .get(&(tenant_id.to_owned(), intent_id.to_owned(), job_id.to_owned()))
                .cloned())
        }
    }

    fn signal(signal_id: &str) -> ReconIntakeSignal {
        ReconIntakeSignal {
            signal_id: ReconIntakeSignalId::from(signal_id.to_owned()),
            source_system: "execution_core".to_owned(),
            signal_kind: ReconIntakeSignalKind::Finalized,
            tenant_id: TenantId::from("tenant_a"),
            intent_id: "intent_1".into(),
            job_id: "job_1".into(),
            adapter_id: Some("adapter_solana".into()),
            receipt_id: Some("receipt_1".into()),
            transition_id: Some("transition_1".into()),
            callback_id: Some(CallbackId::from("callback_1".to_owned())),
            recon_subject_id: "reconsub_job_1".to_owned(),
            canonical_state: Some(CanonicalState::Succeeded),
            classification: Some(PlatformClassification::Success),
            execution_correlation_id: Some("corr-1".to_owned()),
            adapter_execution_reference: Some("sig-final".to_owned()),
            external_observation_key: Some("sig-final".to_owned()),
            expected_fact_snapshot: Some(serde_json::json!({ "version": 1 })),
            payload: serde_json::json!({}),
            occurred_at_ms: 1,
        }
    }

    #[tokio::test]
    async fn intake_signal_materializes_subject_once() {
        let repo = Arc::new(FakeRepo::default());
        let service = ReconIntakeService::new(repo.clone());

        let first = service.ingest_signal(&signal("sig_1")).await.unwrap();
        assert!(!first.duplicate_signal);
        assert_eq!(first.subject.subject_id, "reconsub_job_1");
        assert_eq!(first.subject.latest_signal_kind.as_deref(), Some("finalized"));
        assert_eq!(
            first.subject.adapter_execution_reference.as_deref(),
            Some("sig-final")
        );
        assert!(first.subject.expected_fact_snapshot.is_some());
        assert_eq!(first.subject.scheduled_at_ms, Some(1));

        let second = service.ingest_signal(&signal("sig_1")).await.unwrap();
        assert!(second.duplicate_signal);
        assert_eq!(second.subject.subject_id, "reconsub_job_1");
        assert_eq!(repo.subjects.lock().unwrap().len(), 1);
    }
}
