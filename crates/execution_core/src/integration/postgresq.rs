use crate::engine::ExecutionCore;
use crate::error::{CoreError, CoreResult};
use crate::model::{
    CallbackJob, ExecutionJob, IdempotencyBinding, IntentId, JobId, LeaseId, LeasedJob,
    NormalizedIntent, ReceiptEntry, ReconIntakeSignal, ReplayDecisionRecord, StateTransition,
    TenantId, TimestampMs,
};
use crate::ports::{AdapterExecutionError, DurableStore, RoutingError, StoreError};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresQConfig {
    pub dispatch_queue: String,
    pub callback_queue: String,
    pub dispatch_job_type: String,
    pub callback_job_type: String,
    pub queue_job_max_attempts: i32,
}

impl Default for PostgresQConfig {
    fn default() -> Self {
        Self {
            dispatch_queue: "execution.dispatch".to_owned(),
            callback_queue: "execution.callback".to_owned(),
            dispatch_job_type: "execution.dispatch".to_owned(),
            callback_job_type: "execution.callback".to_owned(),
            queue_job_max_attempts: 25,
        }
    }
}

#[derive(Clone)]
pub struct PostgresQStore {
    pool: PgPool,
    cfg: PostgresQConfig,
}

impl PostgresQStore {
    pub fn new(pool: PgPool, cfg: PostgresQConfig) -> Self {
        Self { pool, cfg }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn config(&self) -> &PostgresQConfig {
        &self.cfg
    }

    pub fn notify_channel_for_queue(queue: &str) -> String {
        format!("azums_{}_notify", Self::sanitize_dataset_queue(queue))
    }

    pub async fn ensure_schema(&self) -> Result<(), StoreError> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_intents (
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                intent_kind TEXT NOT NULL,
                received_at_ms BIGINT NOT NULL,
                intent_json JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, intent_id)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_jobs (
                job_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                job_json JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS execution_core_jobs_tenant_intent_updated_idx
            ON execution_core_jobs(tenant_id, intent_id, updated_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_intent_idempotency (
                tenant_id TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                request_fingerprint TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, idempotency_key)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS execution_core_intent_idempotency_intent_idx
            ON execution_core_intent_idempotency(tenant_id, intent_id, created_at DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_state_transitions (
                transition_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                to_state TEXT NOT NULL,
                classification TEXT NOT NULL,
                occurred_at_ms BIGINT NOT NULL,
                transition_json JSONB NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS execution_core_state_transitions_job_idx
            ON execution_core_state_transitions(job_id, occurred_at_ms ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_receipts (
                receipt_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                state TEXT NOT NULL,
                classification TEXT NOT NULL,
                occurred_at_ms BIGINT NOT NULL,
                receipt_json JSONB NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS execution_core_receipts_job_idx
            ON execution_core_receipts(job_id, occurred_at_ms ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS platform_recon_intake_signals (
                signal_id TEXT PRIMARY KEY,
                source_system TEXT NOT NULL,
                signal_kind TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NULL,
                receipt_id TEXT NULL,
                transition_id TEXT NULL,
                callback_id TEXT NULL,
                recon_subject_id TEXT NOT NULL,
                canonical_state TEXT NULL,
                classification TEXT NULL,
                execution_correlation_id TEXT NULL,
                adapter_execution_reference TEXT NULL,
                external_observation_key TEXT NULL,
                expected_fact_snapshot_json JSONB NULL,
                payload_json JSONB NOT NULL,
                occurred_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS platform_recon_intake_signals_subject_idx
            ON platform_recon_intake_signals(recon_subject_id, occurred_at_ms ASC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS platform_recon_intake_signals_occurred_idx
            ON platform_recon_intake_signals(occurred_at_ms ASC, signal_id ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS execution_core_replay_decisions (
                replay_decision_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                source_job_id TEXT NOT NULL,
                allowed BOOLEAN NOT NULL,
                reason TEXT NOT NULL,
                requested_by TEXT NOT NULL,
                occurred_at_ms BIGINT NOT NULL,
                replay_json JSONB NOT NULL
            )
            "#,
            r#"
            DO $$
            BEGIN
                IF to_regclass('public.jobs') IS NOT NULL THEN
                    EXECUTE '
                        CREATE INDEX IF NOT EXISTS jobs_queue_job_type_runnable_idx
                        ON jobs (queue, job_type, priority DESC, run_at ASC, created_at ASC, id)
                        WHERE status = ''queued''
                    ';
                END IF;
            END
            $$;
            "#,
            r#"
            DO $$
            BEGIN
                IF to_regclass('public.job_attempts') IS NOT NULL THEN
                    EXECUTE '
                        CREATE INDEX IF NOT EXISTS job_attempts_dataset_job_attempt_no_idx
                        ON job_attempts (dataset_id, job_id, attempt_no DESC)
                    ';
                END IF;
            END
            $$;
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(sqlx_err_to_store)?;
        }

        sqlx::query(
            r#"
            ALTER TABLE execution_core_intent_idempotency
            ADD COLUMN IF NOT EXISTS request_fingerprint TEXT NULL
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        Ok(())
    }

    async fn enqueue_pg_job(
        &self,
        queue: &str,
        job_type: &str,
        payload: Value,
        run_at_ms: TimestampMs,
    ) -> Result<(), StoreError> {
        let run_at = u64_to_datetime(run_at_ms);
        let dataset_id = Self::dataset_id_for(queue, run_at);
        self.ensure_dataset_partition(&dataset_id).await?;

        sqlx::query(
            r#"
            INSERT INTO jobs (
                dataset_id,
                queue,
                job_type,
                payload_schema,
                payload_schema_version,
                payload_json,
                run_at,
                status,
                priority,
                max_attempts,
                idempotency_key
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                1,
                $5,
                $6,
                'queued',
                0,
                $7,
                NULL
            )
            "#,
        )
        .bind(dataset_id)
        .bind(queue)
        .bind(job_type)
        .bind(job_type)
        .bind(payload)
        .bind(run_at)
        .bind(self.cfg.queue_job_max_attempts)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(Self::notify_channel_for_queue(queue))
            .bind(job_type)
            .execute(&self.pool)
            .await
            .map_err(sqlx_err_to_store)?;

        Ok(())
    }

    async fn ensure_dataset_partition(&self, dataset_id: &str) -> Result<(), StoreError> {
        match sqlx::query("SELECT public.ensure_jobs_dataset_partition($1)")
            .bind(dataset_id)
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42883") => {
                Ok(())
            }
            Err(err) => Err(sqlx_err_to_store(err)),
        }
    }

    fn sanitize_dataset_queue(queue: &str) -> String {
        let mut out = String::with_capacity(queue.len());
        for ch in queue.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push('_');
            }
        }
        let trimmed = out.trim_matches('_');
        if trimmed.is_empty() {
            "default".to_owned()
        } else {
            trimmed.chars().take(32).collect()
        }
    }

    fn dataset_id_for(queue: &str, run_at: DateTime<Utc>) -> String {
        format!(
            "{}_{}",
            Self::sanitize_dataset_queue(queue),
            run_at.format("%Y%m%d_%H")
        )
    }
}

#[async_trait]
impl DurableStore for PostgresQStore {
    async fn persist_intent(&self, intent: &NormalizedIntent) -> Result<(), StoreError> {
        let intent_json = serde_json::to_value(intent)
            .map_err(|e| StoreError::Backend(format!("serialize intent: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO execution_core_intents (
                tenant_id, intent_id, intent_kind, received_at_ms, intent_json
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, intent_id)
            DO UPDATE SET
                intent_kind = EXCLUDED.intent_kind,
                received_at_ms = EXCLUDED.received_at_ms,
                intent_json = EXCLUDED.intent_json
            "#,
        )
        .bind(intent.tenant_id.as_str())
        .bind(intent.intent_id.as_str())
        .bind(intent.kind.as_str())
        .bind(intent.received_at_ms as i64)
        .bind(intent_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn get_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<NormalizedIntent>, StoreError> {
        let row = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT intent_json
            FROM execution_core_intents
            WHERE tenant_id = $1 AND intent_id = $2
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        match row {
            Some(value) => {
                let parsed = serde_json::from_value(value)
                    .map_err(|e| StoreError::Backend(format!("deserialize intent: {e}")))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    async fn lookup_intent_by_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
    ) -> Result<Option<IdempotencyBinding>, StoreError> {
        let row = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT intent_id, request_fingerprint
            FROM execution_core_intent_idempotency
            WHERE tenant_id = $1 AND idempotency_key = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        Ok(row.map(|(intent_id, request_fingerprint)| IdempotencyBinding {
            intent_id: IntentId::from(intent_id),
            request_fingerprint,
        }))
    }

    async fn bind_intent_idempotency(
        &self,
        tenant_id: &TenantId,
        idempotency_key: &str,
        intent_id: &IntentId,
        request_fingerprint: &str,
    ) -> Result<IdempotencyBinding, StoreError> {
        let inserted = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            INSERT INTO execution_core_intent_idempotency (
                tenant_id,
                idempotency_key,
                intent_id,
                request_fingerprint
            )
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, idempotency_key) DO NOTHING
            RETURNING intent_id, request_fingerprint
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(idempotency_key)
        .bind(intent_id.as_str())
        .bind(request_fingerprint)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        if let Some((intent_id, request_fingerprint)) = inserted {
            return Ok(IdempotencyBinding {
                intent_id: IntentId::from(intent_id),
                request_fingerprint,
            });
        }

        let existing = sqlx::query_as::<_, (String, Option<String>)>(
            r#"
            SELECT intent_id, request_fingerprint
            FROM execution_core_intent_idempotency
            WHERE tenant_id = $1 AND idempotency_key = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?
        .ok_or_else(|| {
            StoreError::Backend(
                "idempotency key bind returned no existing row after conflict".to_owned(),
            )
        })?;

        Ok(IdempotencyBinding {
            intent_id: IntentId::from(existing.0),
            request_fingerprint: existing.1,
        })
    }

    async fn persist_job(&self, job: &ExecutionJob) -> Result<(), StoreError> {
        let job_json = serde_json::to_value(job)
            .map_err(|e| StoreError::Backend(format!("serialize job: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO execution_core_jobs (
                job_id, tenant_id, intent_id, adapter_id, updated_at_ms, job_json
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (job_id)
            DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                intent_id = EXCLUDED.intent_id,
                adapter_id = EXCLUDED.adapter_id,
                updated_at_ms = EXCLUDED.updated_at_ms,
                job_json = EXCLUDED.job_json
            "#,
        )
        .bind(job.job_id.as_str())
        .bind(job.tenant_id.as_str())
        .bind(job.intent_id.as_str())
        .bind(job.adapter_id.as_str())
        .bind(job.updated_at_ms as i64)
        .bind(job_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn persist_submission(
        &self,
        intent: &NormalizedIntent,
        job: &ExecutionJob,
        transitions: &[StateTransition],
        receipts: &[ReceiptEntry],
        not_before_ms: Option<TimestampMs>,
    ) -> Result<(), StoreError> {
        let intent_json = serde_json::to_value(intent)
            .map_err(|e| StoreError::Backend(format!("serialize intent: {e}")))?;
        let job_json = serde_json::to_value(job)
            .map_err(|e| StoreError::Backend(format!("serialize job: {e}")))?;
        let run_at_ms = not_before_ms.unwrap_or_else(now_ms);
        let run_at = u64_to_datetime(run_at_ms);
        let dataset_id = Self::dataset_id_for(&self.cfg.dispatch_queue, run_at);
        let payload = serde_json::json!({
            "execution_job_id": job.job_id.as_str(),
        });

        self.ensure_dataset_partition(&dataset_id).await?;

        let mut tx = self.pool.begin().await.map_err(sqlx_err_to_store)?;

        sqlx::query(
            r#"
            INSERT INTO execution_core_intents (
                tenant_id, intent_id, intent_kind, received_at_ms, intent_json
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, intent_id)
            DO UPDATE SET
                intent_kind = EXCLUDED.intent_kind,
                received_at_ms = EXCLUDED.received_at_ms,
                intent_json = EXCLUDED.intent_json
            "#,
        )
        .bind(intent.tenant_id.as_str())
        .bind(intent.intent_id.as_str())
        .bind(intent.kind.as_str())
        .bind(intent.received_at_ms as i64)
        .bind(intent_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_err_to_store)?;

        sqlx::query(
            r#"
            INSERT INTO execution_core_jobs (
                job_id, tenant_id, intent_id, adapter_id, updated_at_ms, job_json
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (job_id)
            DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                intent_id = EXCLUDED.intent_id,
                adapter_id = EXCLUDED.adapter_id,
                updated_at_ms = EXCLUDED.updated_at_ms,
                job_json = EXCLUDED.job_json
            "#,
        )
        .bind(job.job_id.as_str())
        .bind(job.tenant_id.as_str())
        .bind(job.intent_id.as_str())
        .bind(job.adapter_id.as_str())
        .bind(job.updated_at_ms as i64)
        .bind(job_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_err_to_store)?;

        for transition in transitions {
            let transition_json = serde_json::to_value(transition)
                .map_err(|e| StoreError::Backend(format!("serialize transition: {e}")))?;
            sqlx::query(
                r#"
                INSERT INTO execution_core_state_transitions (
                    transition_id, tenant_id, intent_id, job_id, to_state, classification, occurred_at_ms, transition_json
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (transition_id) DO NOTHING
                "#,
            )
            .bind(transition.transition_id.as_str())
            .bind(transition.tenant_id.as_str())
            .bind(transition.intent_id.as_str())
            .bind(transition.job_id.as_str())
            .bind(format!("{:?}", transition.to_state))
            .bind(format!("{:?}", transition.classification))
            .bind(transition.occurred_at_ms as i64)
            .bind(transition_json)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_err_to_store)?;
        }

        for receipt in receipts {
            self.insert_receipt_tx(&mut tx, receipt).await?;
        }

        sqlx::query(
            r#"
            INSERT INTO jobs (
                dataset_id,
                queue,
                job_type,
                payload_schema,
                payload_schema_version,
                payload_json,
                run_at,
                status,
                priority,
                max_attempts,
                idempotency_key
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                1,
                $5,
                $6,
                'queued',
                0,
                $7,
                NULL
            )
            "#,
        )
        .bind(&dataset_id)
        .bind(&self.cfg.dispatch_queue)
        .bind(&self.cfg.dispatch_job_type)
        .bind(&self.cfg.dispatch_job_type)
        .bind(payload)
        .bind(run_at)
        .bind(self.cfg.queue_job_max_attempts)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_err_to_store)?;

        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(Self::notify_channel_for_queue(&self.cfg.dispatch_queue))
            .bind(&self.cfg.dispatch_job_type)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_err_to_store)?;

        tx.commit().await.map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn update_job(&self, job: &ExecutionJob) -> Result<(), StoreError> {
        self.persist_job(job).await
    }

    async fn get_job(&self, job_id: &JobId) -> Result<Option<ExecutionJob>, StoreError> {
        let row = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT job_json
            FROM execution_core_jobs
            WHERE job_id = $1
            "#,
        )
        .bind(job_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        match row {
            Some(value) => {
                let parsed = serde_json::from_value(value)
                    .map_err(|e| StoreError::Backend(format!("deserialize job: {e}")))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    async fn get_latest_job_for_intent(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<ExecutionJob>, StoreError> {
        let row = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT job_json
            FROM execution_core_jobs
            WHERE tenant_id = $1 AND intent_id = $2
            ORDER BY updated_at_ms DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        match row {
            Some(value) => {
                let parsed = serde_json::from_value(value)
                    .map_err(|e| StoreError::Backend(format!("deserialize latest job: {e}")))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    async fn record_transition(&self, transition: &StateTransition) -> Result<(), StoreError> {
        let transition_json = serde_json::to_value(transition)
            .map_err(|e| StoreError::Backend(format!("serialize transition: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO execution_core_state_transitions (
                transition_id, tenant_id, intent_id, job_id, to_state, classification, occurred_at_ms, transition_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (transition_id) DO NOTHING
            "#,
        )
        .bind(transition.transition_id.as_str())
        .bind(transition.tenant_id.as_str())
        .bind(transition.intent_id.as_str())
        .bind(transition.job_id.as_str())
        .bind(format!("{:?}", transition.to_state))
        .bind(format!("{:?}", transition.classification))
        .bind(transition.occurred_at_ms as i64)
        .bind(transition_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn append_receipt(&self, receipt: &ReceiptEntry) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_err_to_store)?;
        self.insert_receipt_tx(&mut tx, receipt).await?;
        tx.commit().await.map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn record_recon_intake_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<(), StoreError> {
        let signal_json = serde_json::to_value(signal)
            .map_err(|e| StoreError::Backend(format!("serialize recon signal: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO platform_recon_intake_signals (
                signal_id, source_system, signal_kind, tenant_id, intent_id, job_id,
                adapter_id, receipt_id, transition_id, callback_id, recon_subject_id,
                canonical_state, classification, execution_correlation_id,
                adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, payload_json, occurred_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            ON CONFLICT (signal_id) DO NOTHING
            "#,
        )
        .bind(signal.signal_id.as_str())
        .bind(&signal.source_system)
        .bind(signal.signal_kind.as_str())
        .bind(signal.tenant_id.as_str())
        .bind(signal.intent_id.as_str())
        .bind(signal.job_id.as_str())
        .bind(signal.adapter_id.as_ref().map(|value| value.to_string()))
        .bind(signal.receipt_id.as_ref().map(|value| value.to_string()))
        .bind(signal.transition_id.as_ref().map(|value| value.to_string()))
        .bind(signal.callback_id.as_ref().map(|value| value.to_string()))
        .bind(&signal.recon_subject_id)
        .bind(signal.canonical_state.map(|value| format!("{:?}", value)))
        .bind(signal.classification.map(|value| format!("{:?}", value)))
        .bind(&signal.execution_correlation_id)
        .bind(&signal.adapter_execution_reference)
        .bind(&signal.external_observation_key)
        .bind(&signal.expected_fact_snapshot)
        .bind(signal_json)
        .bind(signal.occurred_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn append_receipt_bundle(
        &self,
        receipt: &ReceiptEntry,
        signals: &[ReconIntakeSignal],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_err_to_store)?;
        self.insert_receipt_tx(&mut tx, receipt).await?;
        for signal in signals {
            self.insert_recon_signal_tx(&mut tx, signal).await?;
        }
        tx.commit().await.map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn record_replay_decision(
        &self,
        record: &ReplayDecisionRecord,
    ) -> Result<(), StoreError> {
        let replay_json = serde_json::to_value(record)
            .map_err(|e| StoreError::Backend(format!("serialize replay decision: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO execution_core_replay_decisions (
                replay_decision_id, tenant_id, intent_id, source_job_id, allowed, reason, requested_by, occurred_at_ms, replay_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (replay_decision_id) DO NOTHING
            "#,
        )
        .bind(record.replay_decision_id.as_str())
        .bind(record.tenant_id.as_str())
        .bind(record.intent_id.as_str())
        .bind(record.source_job_id.as_str())
        .bind(record.allowed)
        .bind(&record.reason)
        .bind(&record.requested_by)
        .bind(record.occurred_at_ms as i64)
        .bind(replay_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn enqueue_dispatch(
        &self,
        job_id: &JobId,
        not_before_ms: Option<TimestampMs>,
    ) -> Result<(), StoreError> {
        let run_at_ms = not_before_ms.unwrap_or_else(now_ms);
        let payload = serde_json::json!({
            "execution_job_id": job_id.as_str(),
        });
        self.enqueue_pg_job(
            &self.cfg.dispatch_queue,
            &self.cfg.dispatch_job_type,
            payload,
            run_at_ms,
        )
        .await
    }

    async fn enqueue_callback_job(&self, callback: &CallbackJob) -> Result<(), StoreError> {
        let run_at_ms = now_ms();
        let payload = serde_json::to_value(callback)
            .map_err(|e| StoreError::Backend(format!("serialize callback payload: {e}")))?;
        self.enqueue_pg_job(
            &self.cfg.callback_queue,
            &self.cfg.callback_job_type,
            payload,
            run_at_ms,
        )
        .await
    }
}

impl PostgresQStore {
    async fn insert_receipt_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        receipt: &ReceiptEntry,
    ) -> Result<(), StoreError> {
        let receipt_json = serde_json::to_value(receipt)
            .map_err(|e| StoreError::Backend(format!("serialize receipt: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO execution_core_receipts (
                receipt_id, tenant_id, intent_id, job_id, state, classification, occurred_at_ms, receipt_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (receipt_id) DO NOTHING
            "#,
        )
        .bind(receipt.receipt_id.as_str())
        .bind(receipt.tenant_id.as_str())
        .bind(receipt.intent_id.as_str())
        .bind(receipt.job_id.as_str())
        .bind(format!("{:?}", receipt.state))
        .bind(format!("{:?}", receipt.classification))
        .bind(receipt.occurred_at_ms as i64)
        .bind(receipt_json)
        .execute(&mut **tx)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn insert_recon_signal_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        signal: &ReconIntakeSignal,
    ) -> Result<(), StoreError> {
        let signal_json = serde_json::to_value(signal)
            .map_err(|e| StoreError::Backend(format!("serialize recon signal: {e}")))?;
        sqlx::query(
            r#"
            INSERT INTO platform_recon_intake_signals (
                signal_id, source_system, signal_kind, tenant_id, intent_id, job_id,
                adapter_id, receipt_id, transition_id, callback_id, recon_subject_id,
                canonical_state, classification, execution_correlation_id,
                adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, payload_json, occurred_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            ON CONFLICT (signal_id) DO NOTHING
            "#,
        )
        .bind(signal.signal_id.as_str())
        .bind(&signal.source_system)
        .bind(signal.signal_kind.as_str())
        .bind(signal.tenant_id.as_str())
        .bind(signal.intent_id.as_str())
        .bind(signal.job_id.as_str())
        .bind(signal.adapter_id.as_ref().map(|value| value.to_string()))
        .bind(signal.receipt_id.as_ref().map(|value| value.to_string()))
        .bind(signal.transition_id.as_ref().map(|value| value.to_string()))
        .bind(signal.callback_id.as_ref().map(|value| value.to_string()))
        .bind(&signal.recon_subject_id)
        .bind(signal.canonical_state.map(|value| format!("{:?}", value)))
        .bind(signal.classification.map(|value| format!("{:?}", value)))
        .bind(&signal.execution_correlation_id)
        .bind(&signal.adapter_execution_reference)
        .bind(&signal.external_observation_key)
        .bind(&signal.expected_fact_snapshot)
        .bind(signal_json)
        .bind(signal.occurred_at_ms as i64)
        .execute(&mut **tx)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PostgresQWorkerConfig {
    pub queue: String,
    pub worker_id: String,
    pub lease_seconds: i64,
    pub batch_size: i64,
    pub idle_sleep_ms: u64,
    pub notify_max_wait_ms: u64,
    pub listener_database_url: Option<String>,
    pub notify_channel: Option<String>,
    pub reap_interval_ms: u64,
    pub retry_base_delay_secs: i64,
    pub retry_max_delay_secs: i64,
}

impl Default for PostgresQWorkerConfig {
    fn default() -> Self {
        Self {
            queue: "execution.dispatch".to_owned(),
            worker_id: "execution-core-worker".to_owned(),
            lease_seconds: 30,
            batch_size: 32,
            idle_sleep_ms: 50,
            notify_max_wait_ms: 500,
            listener_database_url: None,
            notify_channel: None,
            reap_interval_ms: 5_000,
            retry_base_delay_secs: 1,
            retry_max_delay_secs: 300,
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LeasedQueueJob {
    id: Uuid,
    dataset_id: String,
    payload_json: Value,
    max_attempts: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct StartedAttempt {
    id: Uuid,
    attempt_no: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DispatchEnvelope {
    execution_job_id: String,
}

pub struct PostgresQWorker {
    core: std::sync::Arc<ExecutionCore>,
    store: std::sync::Arc<PostgresQStore>,
    cfg: PostgresQWorkerConfig,
}

impl PostgresQWorker {
    pub fn new(
        core: std::sync::Arc<ExecutionCore>,
        store: std::sync::Arc<PostgresQStore>,
        cfg: PostgresQWorkerConfig,
    ) -> Self {
        Self { core, store, cfg }
    }

    pub async fn run_once(&self) -> CoreResult<usize> {
        let batch = self.lease_dispatch_jobs().await?;
        if batch.is_empty() {
            return Ok(0);
        }

        for queued in &batch {
            self.process_one(queued).await?;
        }
        Ok(batch.len())
    }

    pub async fn run_forever(&self) -> CoreResult<()> {
        let mut listener = self.connect_dispatch_listener().await;
        let mut last_reap = Instant::now() - Duration::from_millis(self.cfg.reap_interval_ms);
        loop {
            if last_reap.elapsed().as_millis() >= self.cfg.reap_interval_ms as u128 {
                self.reap_expired_locks().await?;
                last_reap = Instant::now();
            }

            let processed = self.run_once().await?;
            if processed == 0 {
                self.wait_for_dispatch_signal(&mut listener).await;
            }
        }
    }

    async fn connect_dispatch_listener(&self) -> Option<PgListener> {
        let Some(database_url) = self.cfg.listener_database_url.as_deref() else {
            return None;
        };
        let Some(channel) = self.cfg.notify_channel.as_deref() else {
            return None;
        };

        match PgListener::connect(database_url).await {
            Ok(mut listener) => match listener.listen(channel).await {
                Ok(()) => Some(listener),
                Err(_) => None,
            },
            Err(_) => None,
        }
    }

    async fn wait_for_dispatch_signal(&self, listener: &mut Option<PgListener>) {
        if let Some(active_listener) = listener.as_mut() {
            match tokio::time::timeout(
                Duration::from_millis(self.cfg.notify_max_wait_ms.max(1)),
                active_listener.recv(),
            )
            .await
            {
                Ok(Ok(_)) => return,
                Ok(Err(_)) => {
                    *listener = self.connect_dispatch_listener().await;
                    return;
                }
                Err(_) => return,
            }
        }

        tokio::time::sleep(Duration::from_millis(self.cfg.idle_sleep_ms)).await;
        if listener.is_none() {
            *listener = self.connect_dispatch_listener().await;
        }
    }

    async fn process_one(&self, queued: &LeasedQueueJob) -> CoreResult<()> {
        let attempt = self.start_attempt(queued).await?;
        let started = Instant::now();

        let exec_res = self.execute_dispatched_job(queued).await;
        let latency_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;

        match exec_res {
            Ok(()) => {
                self.finish_attempt_succeeded(attempt.id, latency_ms)
                    .await?;
                self.mark_queue_job_succeeded(queued.id).await?;
                Ok(())
            }
            Err(err) => {
                let (error_code, error_message, retryable) = classify_core_error(&err);
                self.finish_attempt_failed(attempt.id, latency_ms, error_code, &error_message)
                    .await?;

                let can_retry = retryable && attempt.attempt_no < queued.max_attempts;
                if can_retry {
                    let delay_secs = next_retry_delay_secs(
                        attempt.attempt_no,
                        self.cfg.retry_base_delay_secs,
                        self.cfg.retry_max_delay_secs,
                    );
                    let next_run_at = Utc::now() + chrono::Duration::seconds(delay_secs);
                    self.reschedule_queue_job(queued.id, next_run_at, error_code, &error_message)
                        .await?;
                } else {
                    self.mark_queue_job_dlq(queued.id, "CORE_ERROR", error_code, &error_message)
                        .await?;
                }
                Ok(())
            }
        }
    }

    async fn execute_dispatched_job(&self, queued: &LeasedQueueJob) -> CoreResult<()> {
        let envelope: DispatchEnvelope = serde_json::from_value(queued.payload_json.clone())
            .map_err(|e| {
                CoreError::Store(StoreError::Backend(format!(
                    "invalid dispatch payload on queue job {}: {e}",
                    queued.id
                )))
            })?;

        let execution_job_id = JobId::from(envelope.execution_job_id);
        let job = self
            .store
            .get_job(&execution_job_id)
            .await?
            .ok_or(CoreError::JobNotFound(execution_job_id))?;

        let leased_job = LeasedJob {
            lease_id: LeaseId::new(),
            job,
            leased_at_ms: now_ms(),
            lease_expires_at_ms: now_ms()
                .saturating_add((self.cfg.lease_seconds.max(1) as u64) * 1_000),
        };
        self.core.dispatch_job(leased_job).await?;
        Ok(())
    }

    async fn lease_dispatch_jobs(&self) -> CoreResult<Vec<LeasedQueueJob>> {
        let batch_size = self.cfg.batch_size.clamp(1, 4096);
        let rows = sqlx::query_as::<_, LeasedQueueJob>(
            r#"
            WITH candidates AS (
                SELECT id
                FROM jobs
                WHERE queue = $1
                  AND job_type = $2
                  AND status = 'queued'
                  AND run_at <= now()
                ORDER BY priority DESC, run_at ASC, created_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $3
            ),
            leased AS (
                UPDATE jobs j
                SET status = 'running',
                    locked_by = $4,
                    locked_at = now(),
                    lock_expires_at = now() + ($5::int * interval '1 second'),
                    updated_at = now()
                FROM candidates c
                WHERE j.id = c.id
                RETURNING j.id, j.dataset_id, j.payload_json, j.max_attempts
            )
            SELECT id, dataset_id, payload_json, max_attempts
            FROM leased
            "#,
        )
        .bind(&self.cfg.queue)
        .bind(&self.store.cfg.dispatch_job_type)
        .bind(batch_size)
        .bind(&self.cfg.worker_id)
        .bind(self.cfg.lease_seconds)
        .fetch_all(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(rows)
    }

    async fn start_attempt(&self, queued: &LeasedQueueJob) -> CoreResult<StartedAttempt> {
        let row = sqlx::query_as::<_, StartedAttempt>(
            r#"
            INSERT INTO job_attempts (dataset_id, job_id, attempt_no, status, worker_id)
            VALUES (
              $1,
              $2,
              COALESCE(
                (SELECT MAX(attempt_no) FROM job_attempts WHERE job_id = $2 AND dataset_id = $1),
                0
              ) + 1,
              'running',
              $3
            )
            RETURNING id, attempt_no
            "#,
        )
        .bind(&queued.dataset_id)
        .bind(queued.id)
        .bind(&self.cfg.worker_id)
        .fetch_one(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(row)
    }

    async fn finish_attempt_succeeded(&self, attempt_id: Uuid, latency_ms: i32) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'succeeded',
                finished_at = now(),
                latency_ms = $2
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(latency_ms)
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }

    async fn finish_attempt_failed(
        &self,
        attempt_id: Uuid,
        latency_ms: i32,
        error_code: &str,
        error_message: &str,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'failed',
                finished_at = now(),
                latency_ms = $2,
                error_code = $3,
                error_message = $4
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }

    async fn mark_queue_job_succeeded(&self, queue_job_id: Uuid) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND locked_by = $2
            "#,
        )
        .bind(queue_job_id)
        .bind(&self.cfg.worker_id)
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }

    async fn reschedule_queue_job(
        &self,
        queue_job_id: Uuid,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'queued',
                run_at = $2,
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $3,
                last_error_message = $4
            WHERE id = $1
            "#,
        )
        .bind(queue_job_id)
        .bind(next_run_at)
        .bind(error_code)
        .bind(error_message)
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }

    async fn mark_queue_job_dlq(
        &self,
        queue_job_id: Uuid,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
    ) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'dlq',
                dlq_reason_code = $3,
                dlq_at = now(),
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $4,
                last_error_message = $5
            WHERE id = $1
              AND locked_by = $2
            "#,
        )
        .bind(queue_job_id)
        .bind(&self.cfg.worker_id)
        .bind(reason_code)
        .bind(error_code)
        .bind(error_message)
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }

    async fn reap_expired_locks(&self) -> CoreResult<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'queued',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE status = 'running'
              AND lock_expires_at IS NOT NULL
              AND lock_expires_at < now()
            "#,
        )
        .execute(self.store.pool())
        .await
        .map_err(|e| CoreError::Store(sqlx_err_to_store(e)))?;
        Ok(())
    }
}

fn classify_core_error(err: &CoreError) -> (&'static str, String, bool) {
    match err {
        CoreError::Store(StoreError::Backend(message)) => ("STORE_BACKEND", message.clone(), true),
        CoreError::Store(StoreError::Conflict(message)) => {
            ("STORE_CONFLICT", message.clone(), false)
        }
        CoreError::Store(StoreError::NotFound(message)) => {
            ("STORE_NOT_FOUND", message.clone(), false)
        }
        CoreError::Routing(RoutingError::Backend(message)) => {
            ("ROUTING_BACKEND", message.clone(), true)
        }
        CoreError::Routing(RoutingError::AdapterUnavailable(message)) => {
            ("ADAPTER_UNAVAILABLE", message.clone(), true)
        }
        CoreError::Routing(RoutingError::NoRoute(message)) => ("NO_ROUTE", message.clone(), false),
        CoreError::AdapterExecution(AdapterExecutionError::Unavailable(message))
        | CoreError::AdapterExecution(AdapterExecutionError::Timeout(message))
        | CoreError::AdapterExecution(AdapterExecutionError::Transport(message)) => {
            ("ADAPTER_EXECUTION", message.clone(), true)
        }
        CoreError::AdapterExecution(AdapterExecutionError::ContractViolation(message))
        | CoreError::AdapterExecution(AdapterExecutionError::UnsupportedIntent(message))
        | CoreError::AdapterExecution(AdapterExecutionError::Unauthorized(message)) => {
            ("ADAPTER_REJECTED", message.clone(), false)
        }
        other => ("CORE_ERROR", other.to_string(), false),
    }
}

fn next_retry_delay_secs(attempt_no: i32, base_delay_secs: i64, max_delay_secs: i64) -> i64 {
    let exp = attempt_no.saturating_sub(1).min(8) as u32;
    let multiplier = 1_i64.checked_shl(exp).unwrap_or(max_delay_secs.max(1));
    base_delay_secs
        .max(1)
        .saturating_mul(multiplier)
        .clamp(1, max_delay_secs.max(1))
}

fn u64_to_datetime(value: u64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(value as i64)
        .single()
        .unwrap_or_else(Utc::now)
}

fn now_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

fn sqlx_err_to_store(err: sqlx::Error) -> StoreError {
    StoreError::Backend(err.to_string())
}
