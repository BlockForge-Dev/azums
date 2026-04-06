use crate::engine::{ReconEngine, ReconEngineConfig};
use crate::error::ReconError;
use crate::intake::ReconIntakeService;
use crate::model::ReconSubject;
use crate::paystack_rules::PaystackReconRulePack;
use crate::postgres::PostgresReconStore;
use crate::rules::{ReconRuleRegistry, SolanaReconRulePack};
use exception_intelligence::PostgresExceptionStore;
use execution_core::{
    AdapterId, CallbackId, CanonicalState, IntentId, JobId, PlatformClassification,
    ReconIntakeSignal, ReconIntakeSignalId, ReconIntakeSignalKind, TenantId, TransitionId,
};
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ReconWorkerConfig {
    pub poll_interval_ms: u64,
    pub intake_batch_size: u32,
    pub reconcile_batch_size: u32,
    pub max_retry_attempts: u32,
    pub retry_backoff_ms: u64,
}

impl Default for ReconWorkerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 500,
            intake_batch_size: 100,
            reconcile_batch_size: 32,
            max_retry_attempts: 3,
            retry_backoff_ms: 5_000,
        }
    }
}

pub struct ReconWorker {
    recon_store: Arc<PostgresReconStore>,
    exception_store: Arc<PostgresExceptionStore>,
    engine: ReconEngine<PostgresReconStore, PostgresExceptionStore>,
    cfg: ReconWorkerConfig,
}

impl ReconWorker {
    pub fn new(
        recon_store: Arc<PostgresReconStore>,
        exception_store: Arc<PostgresExceptionStore>,
        cfg: ReconWorkerConfig,
    ) -> Self {
        let mut rules = ReconRuleRegistry::new();
        rules.register(Box::new(SolanaReconRulePack));
        rules.register(Box::new(PaystackReconRulePack));
        let engine = ReconEngine::new(
            recon_store.clone(),
            exception_store.clone(),
            rules,
            ReconEngineConfig {
                max_retry_attempts: cfg.max_retry_attempts,
                retry_backoff_ms: cfg.retry_backoff_ms,
            },
        );
        Self {
            recon_store,
            exception_store,
            engine,
            cfg,
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), ReconError> {
        self.recon_store.ensure_schema().await?;
        self.exception_store
            .ensure_schema()
            .await
            .map_err(|err| ReconError::Backend(err.to_string()))?;
        Ok(())
    }

    pub async fn run_once(&self) -> Result<(), ReconError> {
        self.ingest_recon_signals().await?;
        self.ingest_receipts().await?;
        self.ingest_transitions().await?;
        self.ingest_solana_attempts().await?;
        self.ingest_paystack_executions().await?;
        self.ingest_paystack_webhook_events().await?;
        self.ingest_callbacks().await?;
        self.reconcile_dirty_subjects().await?;
        Ok(())
    }

    pub async fn run_forever(&self) -> Result<(), ReconError> {
        loop {
            self.run_once().await?;
            tokio::time::sleep(Duration::from_millis(self.cfg.poll_interval_ms)).await;
        }
    }

    async fn ingest_recon_signals(&self) -> Result<(), ReconError> {
        let intake_service = ReconIntakeService::new(self.recon_store.clone());
        let watermark = self
            .recon_store
            .get_watermark("platform_recon_intake_signals")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT
                signal_id,
                source_system,
                signal_kind,
                tenant_id,
                intent_id,
                job_id,
                adapter_id,
                receipt_id,
                transition_id,
                callback_id,
                recon_subject_id,
                canonical_state,
                classification,
                execution_correlation_id,
                adapter_execution_reference,
                external_observation_key,
                expected_fact_snapshot_json,
                payload_json,
                occurred_at_ms
            FROM platform_recon_intake_signals
            WHERE occurred_at_ms > $1
               OR (occurred_at_ms = $1 AND signal_id > $2)
            ORDER BY occurred_at_ms ASC, signal_id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let signal_id: String = row.get("signal_id");
            let occurred_at_ms = row.get::<i64, _>("occurred_at_ms").max(0) as u64;
            let signal = map_recon_signal_row(row, occurred_at_ms)?;
            intake_service.ingest_signal(&signal).await?;
            latest_cursor = Some(json!({ "ts": occurred_at_ms, "id": signal_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("platform_recon_intake_signals", cursor, current_ms())
                .await?;
        }

        Ok(())
    }

    async fn ingest_receipts(&self) -> Result<(), ReconError> {
        let watermark = self
            .recon_store
            .get_watermark("execution_core_receipts")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT receipt_id, tenant_id, intent_id, job_id, state, classification, occurred_at_ms
            FROM execution_core_receipts
            WHERE occurred_at_ms > $1
               OR (occurred_at_ms = $1 AND receipt_id > $2)
            ORDER BY occurred_at_ms ASC, receipt_id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let tenant_id: String = row.get("tenant_id");
            let intent_id: String = row.get("intent_id");
            let job_id: String = row.get("job_id");
            let state: String = row.get("state");
            let classification: String = row.get("classification");
            let receipt_id: String = row.get("receipt_id");
            let occurred_at_ms = row.get::<i64, _>("occurred_at_ms").max(0) as u64;
            let adapter_id = load_adapter_id(self.recon_store.pool(), &job_id).await?;
            self.recon_store
                .upsert_subject_from_receipt(
                    &tenant_id,
                    &intent_id,
                    &job_id,
                    &adapter_id,
                    &state,
                    &classification,
                    &receipt_id,
                    occurred_at_ms,
                )
                .await?;
            latest_cursor = Some(json!({ "ts": occurred_at_ms, "id": receipt_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("execution_core_receipts", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn ingest_transitions(&self) -> Result<(), ReconError> {
        let watermark = self
            .recon_store
            .get_watermark("execution_core_state_transitions")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT transition_id, tenant_id, intent_id, job_id, to_state, classification, occurred_at_ms
            FROM execution_core_state_transitions
            WHERE occurred_at_ms > $1
               OR (occurred_at_ms = $1 AND transition_id > $2)
            ORDER BY occurred_at_ms ASC, transition_id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let tenant_id: String = row.get("tenant_id");
            let intent_id: String = row.get("intent_id");
            let job_id: String = row.get("job_id");
            let state: String = row.get("to_state");
            let classification: String = row.get("classification");
            let transition_id: String = row.get("transition_id");
            let occurred_at_ms = row.get::<i64, _>("occurred_at_ms").max(0) as u64;
            let adapter_id = load_adapter_id(self.recon_store.pool(), &job_id).await?;
            self.recon_store
                .upsert_subject_from_transition(
                    &tenant_id,
                    &intent_id,
                    &job_id,
                    &adapter_id,
                    &state,
                    &classification,
                    &transition_id,
                    occurred_at_ms,
                )
                .await?;
            latest_cursor = Some(json!({ "ts": occurred_at_ms, "id": transition_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("execution_core_state_transitions", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn ingest_callbacks(&self) -> Result<(), ReconError> {
        let watermark = self
            .recon_store
            .get_watermark("callback_core_deliveries")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT callback_id, tenant_id, intent_id, job_id, updated_at_ms
            FROM callback_core_deliveries
            WHERE updated_at_ms > $1
               OR (updated_at_ms = $1 AND callback_id > $2)
            ORDER BY updated_at_ms ASC, callback_id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let callback_id: String = row.get("callback_id");
            let tenant_id: String = row.get("tenant_id");
            let intent_id: String = row.get("intent_id");
            let job_id: String = row.get("job_id");
            let updated_at_ms = row.get::<i64, _>("updated_at_ms").max(0) as u64;
            self.recon_store
                .attach_callback_to_subject(
                    &tenant_id,
                    &intent_id,
                    &job_id,
                    &callback_id,
                    updated_at_ms,
                )
                .await?;
            latest_cursor = Some(json!({ "ts": updated_at_ms, "id": callback_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("callback_core_deliveries", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn ingest_solana_attempts(&self) -> Result<(), ReconError> {
        let watermark = self.recon_store.get_watermark("solana.tx_attempts").await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT
                id::text AS attempt_id,
                tenant_id,
                intent_id,
                job_id,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms
            FROM solana.tx_attempts
            WHERE tenant_id IS NOT NULL
              AND job_id IS NOT NULL
              AND (
                    (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT > $1
                 OR (
                        (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT = $1
                    AND id::text > $2
                 )
              )
            ORDER BY updated_at ASC, id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let attempt_id: String = row.get("attempt_id");
            let tenant_id: String = row.get("tenant_id");
            let intent_id: String = row.get("intent_id");
            let job_id: String = row.get("job_id");
            let updated_at_ms = row.get::<i64, _>("updated_at_ms").max(0) as u64;
            self.recon_store
                .mark_subject_dirty(&tenant_id, &intent_id, &job_id, updated_at_ms)
                .await?;
            latest_cursor = Some(json!({ "ts": updated_at_ms, "id": attempt_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("solana.tx_attempts", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn ingest_paystack_executions(&self) -> Result<(), ReconError> {
        let watermark = self
            .recon_store
            .get_watermark("paystack.executions")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT
                intent_id,
                tenant_id,
                job_id,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms
            FROM paystack.executions
            WHERE job_id IS NOT NULL
              AND (
                    (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT > $1
                 OR (
                        (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT = $1
                    AND intent_id > $2
                 )
              )
            ORDER BY updated_at ASC, intent_id ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let intent_id: String = row.get("intent_id");
            let tenant_id: String = row.get("tenant_id");
            let job_id: String = row.get("job_id");
            let updated_at_ms = row.get::<i64, _>("updated_at_ms").max(0) as u64;
            self.recon_store
                .mark_subject_dirty(&tenant_id, &intent_id, &job_id, updated_at_ms)
                .await?;
            latest_cursor = Some(json!({ "ts": updated_at_ms, "id": intent_id }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("paystack.executions", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn ingest_paystack_webhook_events(&self) -> Result<(), ReconError> {
        let watermark = self
            .recon_store
            .get_watermark("paystack.webhook_events")
            .await?;
        let ts = watermark
            .get("ts")
            .and_then(|value| value.as_i64())
            .unwrap_or_default();
        let id = watermark
            .get("id")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT
                event_key,
                tenant_id,
                correlated_intent_id AS intent_id,
                correlated_job_id AS job_id,
                received_at_ms
            FROM paystack.webhook_events
            WHERE correlated_intent_id IS NOT NULL
              AND correlated_job_id IS NOT NULL
              AND (
                    received_at_ms > $1
                 OR (
                        received_at_ms = $1
                    AND event_key > $2
                 )
              )
            ORDER BY received_at_ms ASC, event_key ASC
            LIMIT $3
            "#,
        )
        .bind(ts)
        .bind(id)
        .bind(self.cfg.intake_batch_size as i64)
        .fetch_all(self.recon_store.pool())
        .await
        .map_err(|err| ReconError::Backend(err.to_string()))?;

        let mut latest_cursor = None;
        for row in rows {
            let event_key: String = row.get("event_key");
            let tenant_id: String = row.get("tenant_id");
            let intent_id: String = row.get("intent_id");
            let job_id: String = row.get("job_id");
            let received_at_ms = row.get::<i64, _>("received_at_ms").max(0) as u64;
            self.recon_store
                .mark_subject_dirty(&tenant_id, &intent_id, &job_id, received_at_ms)
                .await?;
            latest_cursor = Some(json!({ "ts": received_at_ms, "id": event_key }));
        }

        if let Some(cursor) = latest_cursor {
            self.recon_store
                .set_watermark("paystack.webhook_events", cursor, current_ms())
                .await?;
        }
        Ok(())
    }

    async fn reconcile_dirty_subjects(&self) -> Result<(), ReconError> {
        let subjects = self
            .recon_store
            .claim_dirty_subjects(self.cfg.reconcile_batch_size)
            .await?;
        for subject in subjects {
            self.reconcile_subject(&subject).await?;
        }
        Ok(())
    }

    async fn reconcile_subject(&self, subject: &ReconSubject) -> Result<(), ReconError> {
        self.engine.process_subject(subject).await
    }
}

fn map_recon_signal_row(
    row: sqlx::postgres::PgRow,
    occurred_at_ms: u64,
) -> Result<ReconIntakeSignal, ReconError> {
    let signal_kind_raw: String = row.get("signal_kind");
    let signal_kind = ReconIntakeSignalKind::parse(signal_kind_raw.as_str()).ok_or_else(|| {
        ReconError::Backend(format!(
            "unknown recon intake signal kind `{signal_kind_raw}`"
        ))
    })?;
    let canonical_state = row
        .try_get::<Option<String>, _>("canonical_state")
        .ok()
        .flatten()
        .map(|value| parse_json_enum::<CanonicalState>(&value))
        .transpose()?;
    let classification = row
        .try_get::<Option<String>, _>("classification")
        .ok()
        .flatten()
        .map(|value| parse_json_enum::<PlatformClassification>(&value))
        .transpose()?;
    Ok(ReconIntakeSignal {
        signal_id: ReconIntakeSignalId::from(row.get::<String, _>("signal_id")),
        source_system: row.get("source_system"),
        signal_kind,
        tenant_id: TenantId::from(row.get::<String, _>("tenant_id")),
        intent_id: IntentId::from(row.get::<String, _>("intent_id")),
        job_id: JobId::from(row.get::<String, _>("job_id")),
        adapter_id: row
            .try_get::<Option<String>, _>("adapter_id")
            .ok()
            .flatten()
            .map(AdapterId::from),
        receipt_id: row
            .try_get::<Option<String>, _>("receipt_id")
            .ok()
            .flatten()
            .map(Into::into),
        transition_id: row
            .try_get::<Option<String>, _>("transition_id")
            .ok()
            .flatten()
            .map(TransitionId::from),
        callback_id: row
            .try_get::<Option<String>, _>("callback_id")
            .ok()
            .flatten()
            .map(CallbackId::from),
        recon_subject_id: row.get("recon_subject_id"),
        canonical_state,
        classification,
        execution_correlation_id: row
            .try_get::<Option<String>, _>("execution_correlation_id")
            .ok()
            .flatten(),
        adapter_execution_reference: row
            .try_get::<Option<String>, _>("adapter_execution_reference")
            .ok()
            .flatten(),
        external_observation_key: row
            .try_get::<Option<String>, _>("external_observation_key")
            .ok()
            .flatten(),
        expected_fact_snapshot: row
            .try_get::<Option<serde_json::Value>, _>("expected_fact_snapshot_json")
            .ok()
            .flatten(),
        payload: row
            .try_get::<serde_json::Value, _>("payload_json")
            .unwrap_or_else(|_| json!({})),
        occurred_at_ms,
    })
}

fn parse_json_enum<T>(value: &str) -> Result<T, ReconError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value::<T>(serde_json::Value::String(value.to_owned()))
        .map_err(|err| ReconError::Backend(format!("failed to parse recon enum `{value}`: {err}")))
}

async fn load_adapter_id(pool: &sqlx::PgPool, job_id: &str) -> Result<String, ReconError> {
    sqlx::query_scalar::<_, String>(
        "SELECT adapter_id FROM execution_core_jobs WHERE job_id = $1 LIMIT 1",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .map_err(|err| ReconError::Backend(err.to_string()))
}

fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
