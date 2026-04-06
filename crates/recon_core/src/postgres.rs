use crate::engine::ReconEngineStore;
use crate::error::ReconError;
use crate::intake::ReconIntakeRepository;
use crate::model::{
    make_fact_id, ExpectedFact, ExpectedFactDraft, ObservedFact, ObservedFactDraft, ReconContext,
    ReconEvidenceSnapshot, ReconOperatorActionRecord, ReconOperatorActionType, ReconOutcomeRecord,
    ReconReceipt, ReconRun, ReconRunState, ReconRunStateTransition, ReconSubject,
};
use async_trait::async_trait;
use execution_core::{recon_subject_id_for_job_str, ReconIntakeSignal};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

#[derive(Clone)]
pub struct PostgresReconStore {
    pool: PgPool,
}

impl PostgresReconStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ensure_schema(&self) -> Result<(), ReconError> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_subjects (
                subject_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                canonical_state TEXT NOT NULL,
                platform_classification TEXT NOT NULL,
                latest_receipt_id TEXT NULL,
                latest_transition_id TEXT NULL,
                latest_callback_id TEXT NULL,
                latest_signal_id TEXT NULL,
                latest_signal_kind TEXT NULL,
                execution_correlation_id TEXT NULL,
                adapter_execution_reference TEXT NULL,
                external_observation_key TEXT NULL,
                expected_fact_snapshot_json JSONB NULL,
                dirty BOOLEAN NOT NULL DEFAULT TRUE,
                recon_attempt_count INTEGER NOT NULL DEFAULT 0,
                recon_retry_count INTEGER NOT NULL DEFAULT 0,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                scheduled_at_ms BIGINT NULL,
                next_reconcile_after_ms BIGINT NULL,
                last_reconciled_at_ms BIGINT NULL,
                last_recon_error TEXT NULL,
                last_run_state TEXT NULL,
                UNIQUE (tenant_id, intent_id, job_id)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_subjects_tenant_intent_idx
            ON recon_core_subjects(tenant_id, intent_id, updated_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_source_watermarks (
                source_name TEXT PRIMARY KEY,
                cursor_json JSONB NOT NULL,
                updated_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_runs (
                run_id TEXT PRIMARY KEY,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                rule_pack TEXT NOT NULL,
                lifecycle_state TEXT NOT NULL DEFAULT 'queued',
                normalized_result TEXT NULL,
                outcome TEXT NOT NULL,
                summary TEXT NOT NULL,
                machine_reason TEXT NOT NULL,
                expected_fact_count INTEGER NOT NULL,
                observed_fact_count INTEGER NOT NULL,
                matched_fact_count INTEGER NOT NULL,
                unmatched_fact_count INTEGER NOT NULL,
                exception_case_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                completed_at_ms BIGINT NULL,
                attempt_number INTEGER NOT NULL DEFAULT 1,
                retry_scheduled_at_ms BIGINT NULL,
                last_error TEXT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_runs_subject_idx
            ON recon_core_runs(subject_id, created_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_run_state_transitions (
                state_transition_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                from_state TEXT NULL,
                to_state TEXT NOT NULL,
                reason TEXT NOT NULL,
                payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                occurred_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_run_state_transitions_run_idx
            ON recon_core_run_state_transitions(run_id, occurred_at_ms ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_expected_facts (
                expected_fact_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                fact_type TEXT NOT NULL,
                fact_key TEXT NOT NULL,
                fact_value_json JSONB NOT NULL,
                metadata_json JSONB NOT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_expected_facts_run_idx
            ON recon_core_expected_facts(run_id, created_at_ms ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_observed_facts (
                observed_fact_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                fact_type TEXT NOT NULL,
                fact_key TEXT NOT NULL,
                fact_value_json JSONB NOT NULL,
                source_kind TEXT NOT NULL,
                source_table TEXT NULL,
                source_id TEXT NULL,
                metadata_json JSONB NOT NULL,
                observed_at_ms BIGINT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_observed_facts_run_idx
            ON recon_core_observed_facts(run_id, created_at_ms ASC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_receipts (
                recon_receipt_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                outcome TEXT NOT NULL,
                summary TEXT NOT NULL,
                details_json JSONB NOT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_receipts_subject_idx
            ON recon_core_receipts(subject_id, created_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_outcomes (
                outcome_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                lifecycle_state TEXT NOT NULL DEFAULT 'completed',
                normalized_result TEXT NULL,
                outcome TEXT NOT NULL,
                summary TEXT NOT NULL,
                machine_reason TEXT NOT NULL,
                details_json JSONB NOT NULL,
                exception_case_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_at_ms BIGINT NOT NULL,
                UNIQUE (run_id)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_outcomes_subject_idx
            ON recon_core_outcomes(subject_id, created_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_evidence_snapshots (
                evidence_snapshot_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES recon_core_runs(run_id) ON DELETE CASCADE,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                lifecycle_state TEXT NOT NULL,
                normalized_result TEXT NULL,
                context_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                adapter_rows_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                expected_facts_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                observed_facts_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                match_result_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                exceptions_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_at_ms BIGINT NOT NULL,
                UNIQUE (run_id)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_evidence_snapshots_subject_idx
            ON recon_core_evidence_snapshots(subject_id, created_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_intake_events (
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
                execution_correlation_id TEXT NULL,
                adapter_execution_reference TEXT NULL,
                external_observation_key TEXT NULL,
                expected_fact_snapshot_json JSONB NULL,
                payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                occurred_at_ms BIGINT NOT NULL,
                processed_at_ms BIGINT NOT NULL,
                subject_id TEXT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_intake_events_subject_idx
            ON recon_core_intake_events(subject_id, occurred_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS recon_core_operator_actions (
                action_id TEXT PRIMARY KEY,
                subject_id TEXT NOT NULL REFERENCES recon_core_subjects(subject_id) ON DELETE CASCADE,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                action_type TEXT NOT NULL,
                actor TEXT NOT NULL,
                reason TEXT NOT NULL,
                payload_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_operator_actions_subject_idx
            ON recon_core_operator_actions(subject_id, created_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS recon_core_operator_actions_tenant_intent_idx
            ON recon_core_operator_actions(tenant_id, intent_id, created_at_ms DESC)
            "#,
        ];

        let migrations = [
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS latest_signal_id TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS latest_signal_kind TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS execution_correlation_id TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS adapter_execution_reference TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS external_observation_key TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS expected_fact_snapshot_json JSONB NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS scheduled_at_ms BIGINT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS recon_attempt_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS recon_retry_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS next_reconcile_after_ms BIGINT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS last_recon_error TEXT NULL",
            "ALTER TABLE recon_core_subjects ADD COLUMN IF NOT EXISTS last_run_state TEXT NULL",
            "ALTER TABLE recon_core_runs ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'queued'",
            "ALTER TABLE recon_core_runs ADD COLUMN IF NOT EXISTS normalized_result TEXT NULL",
            "ALTER TABLE recon_core_runs ADD COLUMN IF NOT EXISTS attempt_number INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE recon_core_runs ADD COLUMN IF NOT EXISTS retry_scheduled_at_ms BIGINT NULL",
            "ALTER TABLE recon_core_runs ADD COLUMN IF NOT EXISTS last_error TEXT NULL",
            "ALTER TABLE recon_core_outcomes ADD COLUMN IF NOT EXISTS lifecycle_state TEXT NOT NULL DEFAULT 'completed'",
            "ALTER TABLE recon_core_outcomes ADD COLUMN IF NOT EXISTS normalized_result TEXT NULL",
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(sqlx_to_internal)?;
        }
        for stmt in migrations {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(sqlx_to_internal)?;
        }
        Ok(())
    }

    pub async fn upsert_subject_from_receipt(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        canonical_state: &str,
        platform_classification: &str,
        receipt_id: &str,
        updated_at_ms: u64,
    ) -> Result<ReconSubject, ReconError> {
        self.upsert_subject(
            tenant_id,
            intent_id,
            job_id,
            adapter_id,
            canonical_state,
            platform_classification,
            Some(receipt_id),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            updated_at_ms,
        )
        .await
    }

    pub async fn upsert_subject_from_transition(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        canonical_state: &str,
        platform_classification: &str,
        transition_id: &str,
        updated_at_ms: u64,
    ) -> Result<ReconSubject, ReconError> {
        self.upsert_subject(
            tenant_id,
            intent_id,
            job_id,
            adapter_id,
            canonical_state,
            platform_classification,
            None,
            Some(transition_id),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            updated_at_ms,
        )
        .await
    }

    pub async fn upsert_subject_from_signal(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        canonical_state: &str,
        platform_classification: &str,
        latest_receipt_id: Option<&str>,
        latest_transition_id: Option<&str>,
        latest_callback_id: Option<&str>,
        updated_at_ms: u64,
    ) -> Result<ReconSubject, ReconError> {
        self.upsert_subject(
            tenant_id,
            intent_id,
            job_id,
            adapter_id,
            canonical_state,
            platform_classification,
            latest_receipt_id,
            latest_transition_id,
            latest_callback_id,
            None,
            None,
            None,
            None,
            None,
            None,
            updated_at_ms,
        )
        .await
    }

    pub async fn attach_callback_to_subject(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        callback_id: &str,
        updated_at_ms: u64,
    ) -> Result<(), ReconError> {
        sqlx::query(
            r#"
            UPDATE recon_core_subjects
            SET latest_callback_id = $4,
                dirty = TRUE,
                updated_at_ms = GREATEST(updated_at_ms, $5),
                scheduled_at_ms = GREATEST(COALESCE(scheduled_at_ms, 0), $5)
            WHERE tenant_id = $1
              AND intent_id = $2
              AND job_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .bind(job_id)
        .bind(callback_id)
        .bind(updated_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn mark_subject_dirty(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        updated_at_ms: u64,
    ) -> Result<(), ReconError> {
        sqlx::query(
            r#"
            UPDATE recon_core_subjects
            SET dirty = TRUE,
                updated_at_ms = GREATEST(updated_at_ms, $4),
                scheduled_at_ms = GREATEST(COALESCE(scheduled_at_ms, 0), $4),
                next_reconcile_after_ms = $4
            WHERE tenant_id = $1
              AND intent_id = $2
              AND job_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .bind(job_id)
        .bind(updated_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn load_subject_for_execution(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
    ) -> Result<Option<ReconSubject>, ReconError> {
        let row = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE tenant_id = $1
              AND intent_id = $2
              AND job_id = $3
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(row.map(map_subject_row))
    }

    pub async fn load_subject_for_intent(
        &self,
        tenant_id: &str,
        intent_id: &str,
    ) -> Result<Option<ReconSubject>, ReconError> {
        let row = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY updated_at_ms DESC, subject_id DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(row.map(map_subject_row))
    }

    pub async fn queue_operator_action(
        &self,
        subject: &ReconSubject,
        action_type: ReconOperatorActionType,
        actor: &str,
        reason: &str,
        payload: Value,
        now_ms: u64,
    ) -> Result<(ReconOperatorActionRecord, ReconSubject), ReconError> {
        let action = ReconOperatorActionRecord {
            action_id: make_fact_id("reconact"),
            subject_id: subject.subject_id.clone(),
            tenant_id: subject.tenant_id.clone(),
            intent_id: subject.intent_id.clone(),
            job_id: subject.job_id.clone(),
            action_type,
            actor: actor.to_owned(),
            reason: reason.to_owned(),
            payload,
            created_at_ms: now_ms,
        };

        let mut tx = self.pool.begin().await.map_err(sqlx_to_internal)?;
        sqlx::query(
            r#"
            INSERT INTO recon_core_operator_actions (
                action_id, subject_id, tenant_id, intent_id, job_id,
                action_type, actor, reason, payload_json, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(&action.action_id)
        .bind(&action.subject_id)
        .bind(&action.tenant_id)
        .bind(&action.intent_id)
        .bind(&action.job_id)
        .bind(action.action_type.as_str())
        .bind(&action.actor)
        .bind(&action.reason)
        .bind(&action.payload)
        .bind(action.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        sqlx::query(
            r#"
            UPDATE recon_core_subjects
            SET dirty = TRUE,
                updated_at_ms = GREATEST(updated_at_ms, $2),
                scheduled_at_ms = GREATEST(COALESCE(scheduled_at_ms, 0), $2),
                next_reconcile_after_ms = $2
            WHERE subject_id = $1
            "#,
        )
        .bind(&subject.subject_id)
        .bind(now_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        let row = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE subject_id = $1
            LIMIT 1
            "#,
        )
        .bind(&subject.subject_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        tx.commit().await.map_err(sqlx_to_internal)?;
        Ok((action, map_subject_row(row)))
    }

    pub async fn claim_intake_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<bool, ReconError> {
        let inserted = sqlx::query_scalar::<_, String>(
            r#"
            INSERT INTO recon_core_intake_events (
                signal_id, source_system, signal_kind, tenant_id, intent_id, job_id, adapter_id,
                receipt_id, transition_id, callback_id, recon_subject_id,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, payload_json, occurred_at_ms, processed_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
            ON CONFLICT (signal_id) DO NOTHING
            RETURNING signal_id
            "#,
        )
        .bind(signal.signal_id.as_str())
        .bind(&signal.source_system)
        .bind(signal.signal_kind.as_str())
        .bind(signal.tenant_id.as_str())
        .bind(signal.intent_id.as_str())
        .bind(signal.job_id.as_str())
        .bind(signal.adapter_id.as_ref().map(|value| value.as_str()))
        .bind(signal.receipt_id.as_ref().map(|value| value.as_str()))
        .bind(signal.transition_id.as_ref().map(|value| value.as_str()))
        .bind(signal.callback_id.as_ref().map(|value| value.as_str()))
        .bind(&signal.recon_subject_id)
        .bind(&signal.execution_correlation_id)
        .bind(&signal.adapter_execution_reference)
        .bind(&signal.external_observation_key)
        .bind(&signal.expected_fact_snapshot)
        .bind(&signal.payload)
        .bind(signal.occurred_at_ms as i64)
        .bind(signal.occurred_at_ms as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(inserted.is_some())
    }

    pub async fn claim_dirty_subjects(&self, limit: u32) -> Result<Vec<ReconSubject>, ReconError> {
        let rows = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE dirty = TRUE
              AND COALESCE(next_reconcile_after_ms, 0) <= $2
            ORDER BY COALESCE(next_reconcile_after_ms, updated_at_ms) ASC, subject_id ASC
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .bind(system_now_ms() as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        Ok(rows.into_iter().map(map_subject_row).collect())
    }

    pub async fn load_recon_context(
        &self,
        subject: &ReconSubject,
    ) -> Result<ReconContext, ReconError> {
        let latest_receipt = if let Some(receipt_id) = subject.latest_receipt_id.as_ref() {
            sqlx::query_scalar::<_, Value>(
                "SELECT receipt_json FROM execution_core_receipts WHERE receipt_id = $1 LIMIT 1",
            )
            .bind(receipt_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_to_internal)?
        } else {
            None
        };

        let latest_transition = if let Some(transition_id) = subject.latest_transition_id.as_ref() {
            sqlx::query_scalar::<_, Value>(
                "SELECT transition_json FROM execution_core_state_transitions WHERE transition_id = $1 LIMIT 1",
            )
            .bind(transition_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_to_internal)?
        } else {
            None
        };

        let callback_delivery = if let Some(callback_id) = subject.latest_callback_id.as_ref() {
            sqlx::query(
                r#"
                SELECT callback_id, state, attempts, last_http_status, last_error_class, last_error_message,
                       next_attempt_at_ms, delivered_at_ms, updated_at_ms
                FROM callback_core_deliveries
                WHERE callback_id = $1
                LIMIT 1
                "#,
            )
            .bind(callback_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlx_to_internal)?
            .map(|row| {
                json!({
                    "callback_id": row.get::<String, _>("callback_id"),
                    "state": row.get::<String, _>("state"),
                    "attempts": row.get::<i32, _>("attempts"),
                    "last_http_status": row.try_get::<Option<i32>, _>("last_http_status").ok().flatten(),
                    "last_error_class": row.try_get::<Option<String>, _>("last_error_class").ok().flatten(),
                    "last_error_message": row.try_get::<Option<String>, _>("last_error_message").ok().flatten(),
                    "next_attempt_at_ms": row.try_get::<Option<i64>, _>("next_attempt_at_ms").ok().flatten(),
                    "delivered_at_ms": row.try_get::<Option<i64>, _>("delivered_at_ms").ok().flatten(),
                    "updated_at_ms": row.get::<i64, _>("updated_at_ms"),
                })
            })
        } else {
            None
        };

        let intent = sqlx::query_scalar::<_, Value>(
            "SELECT intent_json FROM execution_core_intents WHERE tenant_id = $1 AND intent_id = $2 LIMIT 1",
        )
        .bind(&subject.tenant_id)
        .bind(&subject.intent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let job = sqlx::query_scalar::<_, Value>(
            "SELECT job_json FROM execution_core_jobs WHERE job_id = $1 LIMIT 1",
        )
        .bind(&subject.job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        Ok(ReconContext {
            latest_receipt,
            latest_transition,
            callback_delivery,
            intent,
            job,
        })
    }

    pub async fn load_latest_solana_observations(
        &self,
        subject: &ReconSubject,
    ) -> Result<Vec<Value>, ReconError> {
        if subject.adapter_id != "adapter_solana" {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT
                attempt.id::text AS attempt_id,
                attempt.intent_id,
                attempt.job_id,
                attempt.status,
                attempt.signature,
                attempt.provider_used,
                attempt.last_confirmation_status,
                attempt.last_err_json,
                attempt.blockhash_used,
                attempt.simulation_outcome,
                intent.intent_type,
                intent.from_addr,
                intent.to_addr,
                intent.amount,
                intent.asset,
                intent.program_id,
                intent.action,
                intent.final_signature,
                intent.final_err_json,
                (EXTRACT(EPOCH FROM attempt.updated_at) * 1000)::BIGINT AS updated_at_ms
            FROM solana.tx_attempts attempt
            JOIN solana.tx_intents intent ON intent.id = attempt.intent_id
            WHERE attempt.tenant_id = $1
              AND attempt.intent_id = $2
              AND attempt.job_id = $3
            ORDER BY attempt.updated_at DESC, attempt.id DESC
            LIMIT 4
            "#,
        )
        .bind(&subject.tenant_id)
        .bind(&subject.intent_id)
        .bind(&subject.job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                json!({
                    "attempt_id": row.get::<String, _>("attempt_id"),
                    "intent_id": row.get::<String, _>("intent_id"),
                    "job_id": row.try_get::<Option<String>, _>("job_id").ok().flatten(),
                    "status": row.get::<String, _>("status"),
                    "signature": row.try_get::<Option<String>, _>("signature").ok().flatten(),
                    "provider_used": row.try_get::<Option<String>, _>("provider_used").ok().flatten(),
                    "last_confirmation_status": row.try_get::<Option<String>, _>("last_confirmation_status").ok().flatten(),
                    "last_err_json": row.try_get::<Option<Value>, _>("last_err_json").ok().flatten(),
                    "blockhash_used": row.try_get::<Option<String>, _>("blockhash_used").ok().flatten(),
                    "simulation_outcome": row.try_get::<Option<String>, _>("simulation_outcome").ok().flatten(),
                    "intent_type": row.get::<String, _>("intent_type"),
                    "from_addr": row.try_get::<Option<String>, _>("from_addr").ok().flatten(),
                    "to_addr": row.get::<String, _>("to_addr"),
                    "amount": row.get::<i64, _>("amount"),
                    "asset": row.try_get::<Option<String>, _>("asset").ok().flatten(),
                    "program_id": row.try_get::<Option<String>, _>("program_id").ok().flatten(),
                    "action": row.try_get::<Option<String>, _>("action").ok().flatten(),
                    "final_signature": row.try_get::<Option<String>, _>("final_signature").ok().flatten(),
                    "final_err_json": row.try_get::<Option<Value>, _>("final_err_json").ok().flatten(),
                    "updated_at_ms": row.get::<i64, _>("updated_at_ms").max(0) as u64,
                })
            })
            .collect())
    }

    pub async fn load_latest_paystack_observations(
        &self,
        subject: &ReconSubject,
    ) -> Result<Vec<Value>, ReconError> {
        if subject.adapter_id != "adapter_paystack" {
            return Ok(Vec::new());
        }

        let execution_row = sqlx::query(
            r#"
            SELECT
                intent_id,
                tenant_id,
                job_id,
                intent_kind,
                operation,
                status,
                provider_reference,
                remote_id,
                request_payload_json,
                last_response_json,
                last_error_code,
                last_error_message,
                amount_minor,
                currency,
                source_reference,
                destination_reference,
                connector_reference,
                (EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms
            FROM paystack.executions
            WHERE tenant_id = $1
              AND intent_id = $2
              AND job_id = $3
            ORDER BY updated_at DESC, intent_id DESC
            LIMIT 1
            "#,
        )
        .bind(&subject.tenant_id)
        .bind(&subject.intent_id)
        .bind(&subject.job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let webhook_rows = sqlx::query(
            r#"
            SELECT
                event_key,
                event_type,
                provider_reference,
                remote_id,
                correlated_intent_id,
                correlated_job_id,
                correlated_receipt_id,
                payload_json,
                received_at_ms
            FROM paystack.webhook_events
            WHERE tenant_id = $1
              AND correlated_intent_id = $2
              AND correlated_job_id = $3
            ORDER BY received_at_ms DESC, event_key DESC
            LIMIT 8
            "#,
        )
        .bind(&subject.tenant_id)
        .bind(&subject.intent_id)
        .bind(&subject.job_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let mut out = Vec::new();
        if let Some(row) = execution_row {
            out.push(json!({
                "row_kind": "execution",
                "intent_id": row.get::<String, _>("intent_id"),
                "tenant_id": row.get::<String, _>("tenant_id"),
                "job_id": row.try_get::<Option<String>, _>("job_id").ok().flatten(),
                "intent_kind": row.get::<String, _>("intent_kind"),
                "operation": row.get::<String, _>("operation"),
                "status": row.get::<String, _>("status"),
                "provider_reference": row.try_get::<Option<String>, _>("provider_reference").ok().flatten(),
                "remote_id": row.try_get::<Option<String>, _>("remote_id").ok().flatten(),
                "request_payload_json": row.get::<Value, _>("request_payload_json"),
                "last_response_json": row.try_get::<Option<Value>, _>("last_response_json").ok().flatten(),
                "last_error_code": row.try_get::<Option<String>, _>("last_error_code").ok().flatten(),
                "last_error_message": row.try_get::<Option<String>, _>("last_error_message").ok().flatten(),
                "amount_minor": row.try_get::<Option<i64>, _>("amount_minor").ok().flatten(),
                "currency": row.try_get::<Option<String>, _>("currency").ok().flatten(),
                "source_reference": row.try_get::<Option<String>, _>("source_reference").ok().flatten(),
                "destination_reference": row.try_get::<Option<String>, _>("destination_reference").ok().flatten(),
                "connector_reference": row.try_get::<Option<String>, _>("connector_reference").ok().flatten(),
                "updated_at_ms": row.get::<i64, _>("updated_at_ms").max(0) as u64,
            }));
        }

        out.extend(webhook_rows.into_iter().map(|row| {
            json!({
                "row_kind": "webhook",
                "event_key": row.get::<String, _>("event_key"),
                "event_type": row.get::<String, _>("event_type"),
                "provider_reference": row.try_get::<Option<String>, _>("provider_reference").ok().flatten(),
                "remote_id": row.try_get::<Option<String>, _>("remote_id").ok().flatten(),
                "correlated_intent_id": row.try_get::<Option<String>, _>("correlated_intent_id").ok().flatten(),
                "correlated_job_id": row.try_get::<Option<String>, _>("correlated_job_id").ok().flatten(),
                "correlated_receipt_id": row.try_get::<Option<String>, _>("correlated_receipt_id").ok().flatten(),
                "payload": row.get::<Value, _>("payload_json"),
                "received_at_ms": row.get::<i64, _>("received_at_ms").max(0) as u64,
            })
        }));

        Ok(out)
    }

    pub async fn load_adapter_observations(
        &self,
        subject: &ReconSubject,
    ) -> Result<Vec<Value>, ReconError> {
        match subject.adapter_id.as_str() {
            "adapter_solana" => self.load_latest_solana_observations(subject).await,
            "adapter_paystack" => self.load_latest_paystack_observations(subject).await,
            _ => Ok(Vec::new()),
        }
    }

    pub async fn create_run(&self, run: &ReconRun) -> Result<(), ReconError> {
        sqlx::query(
            r#"
            INSERT INTO recon_core_runs (
                run_id, subject_id, tenant_id, intent_id, job_id, adapter_id, rule_pack,
                lifecycle_state, normalized_result, outcome, summary, machine_reason,
                expected_fact_count, observed_fact_count, matched_fact_count, unmatched_fact_count,
                exception_case_ids, created_at_ms, updated_at_ms, completed_at_ms,
                attempt_number, retry_scheduled_at_ms, last_error
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23)
            ON CONFLICT (run_id) DO NOTHING
            "#,
        )
        .bind(&run.run_id)
        .bind(&run.subject_id)
        .bind(&run.tenant_id)
        .bind(&run.intent_id)
        .bind(&run.job_id)
        .bind(&run.adapter_id)
        .bind(&run.rule_pack)
        .bind(run.lifecycle_state.as_str())
        .bind(run.normalized_result.map(|value| value.as_str()))
        .bind(run.outcome.as_str())
        .bind(&run.summary)
        .bind(&run.machine_reason)
        .bind(run.expected_fact_count as i32)
        .bind(run.observed_fact_count as i32)
        .bind(run.matched_fact_count as i32)
        .bind(run.unmatched_fact_count as i32)
        .bind(json!(run.exception_case_ids))
        .bind(run.created_at_ms as i64)
        .bind(run.updated_at_ms as i64)
        .bind(run.completed_at_ms.map(|value| value as i64))
        .bind(run.attempt_number as i32)
        .bind(run.retry_scheduled_at_ms.map(|value| value as i64))
        .bind(&run.last_error)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn append_run_state_transition(
        &self,
        transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError> {
        sqlx::query(
            r#"
            INSERT INTO recon_core_run_state_transitions (
                state_transition_id, run_id, subject_id, from_state, to_state, reason, payload_json, occurred_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(&transition.state_transition_id)
        .bind(&transition.run_id)
        .bind(&transition.subject_id)
        .bind(transition.from_state.map(|value| value.as_str()))
        .bind(transition.to_state.as_str())
        .bind(&transition.reason)
        .bind(&transition.payload)
        .bind(transition.occurred_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn finalize_run(
        &self,
        subject: &ReconSubject,
        run: &ReconRun,
        receipt: &ReconReceipt,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        evidence: &ReconEvidenceSnapshot,
        final_transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError> {
        let mut tx = self.pool.begin().await.map_err(sqlx_to_internal)?;
        sqlx::query(
            r#"
            UPDATE recon_core_runs
            SET lifecycle_state = $2,
                normalized_result = $3,
                outcome = $4,
                summary = $5,
                machine_reason = $6,
                expected_fact_count = $7,
                observed_fact_count = $8,
                matched_fact_count = $9,
                unmatched_fact_count = $10,
                exception_case_ids = $11,
                updated_at_ms = $12,
                completed_at_ms = $13,
                retry_scheduled_at_ms = $14,
                last_error = $15
            WHERE run_id = $1
            "#,
        )
        .bind(&run.run_id)
        .bind(run.lifecycle_state.as_str())
        .bind(run.normalized_result.map(|value| value.as_str()))
        .bind(run.outcome.as_str())
        .bind(&run.summary)
        .bind(&run.machine_reason)
        .bind(run.expected_fact_count as i32)
        .bind(run.observed_fact_count as i32)
        .bind(run.matched_fact_count as i32)
        .bind(run.unmatched_fact_count as i32)
        .bind(json!(run.exception_case_ids))
        .bind(run.updated_at_ms as i64)
        .bind(run.completed_at_ms.map(|value| value as i64))
        .bind(run.retry_scheduled_at_ms.map(|value| value as i64))
        .bind(&run.last_error)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        for fact in expected {
            let record = ExpectedFact {
                expected_fact_id: make_fact_id("expfact"),
                run_id: run.run_id.clone(),
                subject_id: subject.subject_id.clone(),
                fact_type: fact.fact_type.clone(),
                fact_key: fact.fact_key.clone(),
                fact_value: fact.fact_value.clone(),
                derived_from: fact.derived_from.clone(),
                created_at_ms: run.completed_at_ms.unwrap_or(run.updated_at_ms),
            };
            sqlx::query(
                r#"
                INSERT INTO recon_core_expected_facts (
                    expected_fact_id, run_id, subject_id, fact_type, fact_key,
                    fact_value_json, metadata_json, created_at_ms
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                "#,
            )
            .bind(&record.expected_fact_id)
            .bind(&record.run_id)
            .bind(&record.subject_id)
            .bind(&record.fact_type)
            .bind(&record.fact_key)
            .bind(&record.fact_value)
            .bind(&record.derived_from)
            .bind(record.created_at_ms as i64)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_internal)?;
        }

        for fact in observed {
            let record = ObservedFact {
                observed_fact_id: make_fact_id("obsfact"),
                run_id: run.run_id.clone(),
                subject_id: subject.subject_id.clone(),
                fact_type: fact.fact_type.clone(),
                fact_key: fact.fact_key.clone(),
                fact_value: fact.fact_value.clone(),
                source_kind: fact.source_kind.clone(),
                source_table: fact.source_table.clone(),
                source_id: fact.source_id.clone(),
                metadata: fact.metadata.clone(),
                observed_at_ms: fact.observed_at_ms,
                created_at_ms: run.completed_at_ms.unwrap_or(run.updated_at_ms),
            };
            sqlx::query(
                r#"
                INSERT INTO recon_core_observed_facts (
                    observed_fact_id, run_id, subject_id, fact_type, fact_key,
                    fact_value_json, source_kind, source_table, source_id,
                    metadata_json, observed_at_ms, created_at_ms
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                "#,
            )
            .bind(&record.observed_fact_id)
            .bind(&record.run_id)
            .bind(&record.subject_id)
            .bind(&record.fact_type)
            .bind(&record.fact_key)
            .bind(&record.fact_value)
            .bind(&record.source_kind)
            .bind(&record.source_table)
            .bind(&record.source_id)
            .bind(&record.metadata)
            .bind(record.observed_at_ms.map(|value| value as i64))
            .bind(record.created_at_ms as i64)
            .execute(&mut *tx)
            .await
            .map_err(sqlx_to_internal)?;
        }

        sqlx::query(
            r#"
            INSERT INTO recon_core_receipts (
                recon_receipt_id, run_id, subject_id, outcome, summary, details_json, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            "#,
        )
        .bind(&receipt.recon_receipt_id)
        .bind(&receipt.run_id)
        .bind(&receipt.subject_id)
        .bind(receipt.outcome.as_str())
        .bind(&receipt.summary)
        .bind(json!(receipt.details))
        .bind(receipt.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        let outcome = ReconOutcomeRecord {
            outcome_id: make_fact_id("reconout"),
            run_id: run.run_id.clone(),
            subject_id: subject.subject_id.clone(),
            tenant_id: run.tenant_id.clone(),
            intent_id: run.intent_id.clone(),
            job_id: run.job_id.clone(),
            adapter_id: run.adapter_id.clone(),
            lifecycle_state: run.lifecycle_state,
            normalized_result: run.normalized_result,
            outcome: run.outcome,
            summary: run.summary.clone(),
            machine_reason: run.machine_reason.clone(),
            details: receipt.details.clone(),
            exception_case_ids: run.exception_case_ids.clone(),
            created_at_ms: run.completed_at_ms.unwrap_or(run.updated_at_ms),
        };

        sqlx::query(
            r#"
            INSERT INTO recon_core_outcomes (
                outcome_id, run_id, subject_id, tenant_id, intent_id, job_id, adapter_id,
                lifecycle_state, normalized_result, outcome, summary, machine_reason, details_json,
                exception_case_ids, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (run_id)
            DO UPDATE SET
                lifecycle_state = EXCLUDED.lifecycle_state,
                normalized_result = EXCLUDED.normalized_result,
                outcome = EXCLUDED.outcome,
                summary = EXCLUDED.summary,
                machine_reason = EXCLUDED.machine_reason,
                details_json = EXCLUDED.details_json,
                exception_case_ids = EXCLUDED.exception_case_ids,
                created_at_ms = EXCLUDED.created_at_ms
            "#,
        )
        .bind(&outcome.outcome_id)
        .bind(&outcome.run_id)
        .bind(&outcome.subject_id)
        .bind(&outcome.tenant_id)
        .bind(&outcome.intent_id)
        .bind(&outcome.job_id)
        .bind(&outcome.adapter_id)
        .bind(outcome.lifecycle_state.as_str())
        .bind(outcome.normalized_result.map(|value| value.as_str()))
        .bind(outcome.outcome.as_str())
        .bind(&outcome.summary)
        .bind(&outcome.machine_reason)
        .bind(json!(outcome.details))
        .bind(json!(outcome.exception_case_ids))
        .bind(outcome.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        sqlx::query(
            r#"
            INSERT INTO recon_core_evidence_snapshots (
                evidence_snapshot_id, run_id, subject_id, tenant_id, intent_id, job_id, adapter_id,
                lifecycle_state, normalized_result, context_json, adapter_rows_json,
                expected_facts_json, observed_facts_json, match_result_json, details_json,
                exceptions_json, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            ON CONFLICT (run_id)
            DO UPDATE SET
                lifecycle_state = EXCLUDED.lifecycle_state,
                normalized_result = EXCLUDED.normalized_result,
                context_json = EXCLUDED.context_json,
                adapter_rows_json = EXCLUDED.adapter_rows_json,
                expected_facts_json = EXCLUDED.expected_facts_json,
                observed_facts_json = EXCLUDED.observed_facts_json,
                match_result_json = EXCLUDED.match_result_json,
                details_json = EXCLUDED.details_json,
                exceptions_json = EXCLUDED.exceptions_json,
                created_at_ms = EXCLUDED.created_at_ms
            "#,
        )
        .bind(&evidence.evidence_snapshot_id)
        .bind(&evidence.run_id)
        .bind(&evidence.subject_id)
        .bind(&evidence.tenant_id)
        .bind(&evidence.intent_id)
        .bind(&evidence.job_id)
        .bind(&evidence.adapter_id)
        .bind(evidence.lifecycle_state.as_str())
        .bind(evidence.normalized_result.map(|value| value.as_str()))
        .bind(&evidence.context)
        .bind(&evidence.adapter_rows)
        .bind(&evidence.expected_facts)
        .bind(&evidence.observed_facts)
        .bind(&evidence.match_result)
        .bind(&evidence.details)
        .bind(&evidence.exceptions)
        .bind(evidence.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        sqlx::query(
            r#"
            INSERT INTO recon_core_run_state_transitions (
                state_transition_id, run_id, subject_id, from_state, to_state, reason, payload_json, occurred_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(&final_transition.state_transition_id)
        .bind(&final_transition.run_id)
        .bind(&final_transition.subject_id)
        .bind(final_transition.from_state.map(|value| value.as_str()))
        .bind(final_transition.to_state.as_str())
        .bind(&final_transition.reason)
        .bind(&final_transition.payload)
        .bind(final_transition.occurred_at_ms as i64)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        sqlx::query(
            r#"
            UPDATE recon_core_subjects
            SET dirty = CASE WHEN $3 = 'retry_scheduled' THEN TRUE ELSE FALSE END,
                recon_attempt_count = GREATEST(recon_attempt_count, $4),
                recon_retry_count = CASE
                    WHEN $3 = 'retry_scheduled' THEN GREATEST(recon_retry_count, $5)
                    ELSE recon_retry_count
                END,
                last_reconciled_at_ms = CASE
                    WHEN $3 = 'retry_scheduled' THEN last_reconciled_at_ms
                    ELSE $2
                END,
                updated_at_ms = GREATEST(updated_at_ms, $2),
                next_reconcile_after_ms = CASE
                    WHEN $3 = 'retry_scheduled' THEN $6
                    ELSE NULL
                END,
                last_recon_error = $7,
                last_run_state = $3
            WHERE subject_id = $1
            "#,
        )
        .bind(&subject.subject_id)
        .bind(run.completed_at_ms.unwrap_or(run.updated_at_ms) as i64)
        .bind(run.lifecycle_state.as_str())
        .bind(run.attempt_number as i32)
        .bind(run.attempt_number as i32)
        .bind(run.retry_scheduled_at_ms.map(|value| value as i64))
        .bind(&run.last_error)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        tx.commit().await.map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn get_watermark(&self, source_name: &str) -> Result<Value, ReconError> {
        let value = sqlx::query_scalar::<_, Value>(
            "SELECT cursor_json FROM recon_core_source_watermarks WHERE source_name = $1 LIMIT 1",
        )
        .bind(source_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(value.unwrap_or_else(|| json!({"ts": 0_u64, "id": ""})))
    }

    pub async fn set_watermark(
        &self,
        source_name: &str,
        cursor: Value,
        updated_at_ms: u64,
    ) -> Result<(), ReconError> {
        sqlx::query(
            r#"
            INSERT INTO recon_core_source_watermarks (source_name, cursor_json, updated_at_ms)
            VALUES ($1,$2,$3)
            ON CONFLICT (source_name)
            DO UPDATE SET cursor_json = EXCLUDED.cursor_json, updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(source_name)
        .bind(cursor)
        .bind(updated_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn load_request_reconciliation(
        &self,
        tenant_id: &str,
        intent_id: &str,
    ) -> Result<
        Option<(
            ReconSubject,
            Vec<ReconRun>,
            Option<ReconReceipt>,
            Vec<ExpectedFact>,
            Vec<ObservedFact>,
        )>,
        ReconError,
    > {
        let row = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY updated_at_ms DESC, subject_id DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let Some(row) = row else {
            return Ok(None);
        };
        let subject = map_subject_row(row);

        let run_rows = sqlx::query(
            r#"
            SELECT
                run_id, subject_id, tenant_id, intent_id, job_id, adapter_id, rule_pack,
                lifecycle_state, normalized_result, outcome, summary, machine_reason,
                expected_fact_count, observed_fact_count, matched_fact_count, unmatched_fact_count,
                exception_case_ids, created_at_ms, updated_at_ms, completed_at_ms,
                attempt_number, retry_scheduled_at_ms, last_error
            FROM recon_core_runs
            WHERE subject_id = $1
            ORDER BY created_at_ms DESC, run_id DESC
            LIMIT 20
            "#,
        )
        .bind(&subject.subject_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let runs: Vec<ReconRun> = run_rows.into_iter().map(map_run_row).collect();
        let latest_receipt = sqlx::query(
            r#"
            SELECT recon_receipt_id, run_id, subject_id, outcome, summary, details_json, created_at_ms
            FROM recon_core_receipts
            WHERE subject_id = $1
            ORDER BY created_at_ms DESC, recon_receipt_id DESC
            LIMIT 1
            "#,
        )
        .bind(&subject.subject_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?
        .map(map_receipt_row);

        let latest_run_id = runs.first().map(|run| run.run_id.clone());
        let mut expected = Vec::new();
        let mut observed = Vec::new();
        if let Some(run_id) = latest_run_id {
            let rows = sqlx::query(
                r#"
                SELECT expected_fact_id, run_id, subject_id, fact_type, fact_key, fact_value_json, metadata_json, created_at_ms
                FROM recon_core_expected_facts
                WHERE run_id = $1
                ORDER BY created_at_ms ASC, expected_fact_id ASC
                "#,
            )
            .bind(&run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;
            expected = rows
                .into_iter()
                .map(|row| ExpectedFact {
                    expected_fact_id: row.get("expected_fact_id"),
                    run_id: row.get("run_id"),
                    subject_id: row.get("subject_id"),
                    fact_type: row.get("fact_type"),
                    fact_key: row.get("fact_key"),
                    fact_value: row.get("fact_value_json"),
                    derived_from: row.get("metadata_json"),
                    created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                })
                .collect();

            let rows = sqlx::query(
                r#"
                SELECT observed_fact_id, run_id, subject_id, fact_type, fact_key, fact_value_json,
                       source_kind, source_table, source_id, metadata_json, observed_at_ms, created_at_ms
                FROM recon_core_observed_facts
                WHERE run_id = $1
                ORDER BY created_at_ms ASC, observed_fact_id ASC
                "#,
            )
            .bind(&run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;
            observed = rows
                .into_iter()
                .map(|row| ObservedFact {
                    observed_fact_id: row.get("observed_fact_id"),
                    run_id: row.get("run_id"),
                    subject_id: row.get("subject_id"),
                    fact_type: row.get("fact_type"),
                    fact_key: row.get("fact_key"),
                    fact_value: row.get("fact_value_json"),
                    source_kind: row.get("source_kind"),
                    source_table: row
                        .try_get::<Option<String>, _>("source_table")
                        .ok()
                        .flatten(),
                    source_id: row.try_get::<Option<String>, _>("source_id").ok().flatten(),
                    metadata: row.get("metadata_json"),
                    observed_at_ms: row
                        .try_get::<Option<i64>, _>("observed_at_ms")
                        .ok()
                        .flatten()
                        .map(|value| value.max(0) as u64),
                    created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                })
                .collect();
        } else if let Some(snapshot) = subject.expected_fact_snapshot.clone() {
            expected.push(ExpectedFact {
                expected_fact_id: format!("intake_snapshot_{}", subject.subject_id),
                run_id: "intake_snapshot".to_owned(),
                subject_id: subject.subject_id.clone(),
                fact_type: "execution".to_owned(),
                fact_key: "execution.expected_snapshot".to_owned(),
                fact_value: snapshot,
                derived_from: json!({
                    "source": "recon_core_subjects",
                    "subject_id": subject.subject_id.clone(),
                }),
                created_at_ms: subject.scheduled_at_ms.unwrap_or(subject.updated_at_ms),
            });
        }

        Ok(Some((subject, runs, latest_receipt, expected, observed)))
    }

    pub async fn materialize_subject_from_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<ReconSubject, ReconError> {
        let adapter_id = signal
            .adapter_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "adapter_unknown".to_owned());
        let canonical_state = signal
            .canonical_state
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "queued".to_owned());
        let classification = signal
            .classification
            .map(|value| format!("{value:?}"))
            .unwrap_or_else(|| "Success".to_owned());
        let subject = self
            .upsert_subject(
                signal.tenant_id.as_str(),
                signal.intent_id.as_str(),
                signal.job_id.as_str(),
                &adapter_id,
                &canonical_state,
                &classification,
                signal.receipt_id.as_ref().map(|value| value.as_str()),
                signal.transition_id.as_ref().map(|value| value.as_str()),
                signal.callback_id.as_ref().map(|value| value.as_str()),
                Some(signal.signal_id.as_str()),
                Some(signal.signal_kind.as_str()),
                signal.execution_correlation_id.as_deref(),
                signal.adapter_execution_reference.as_deref(),
                signal.external_observation_key.as_deref(),
                signal.expected_fact_snapshot.as_ref(),
                signal.occurred_at_ms,
            )
            .await?;

        sqlx::query(
            r#"
            UPDATE recon_core_intake_events
            SET subject_id = $2
            WHERE signal_id = $1
            "#,
        )
        .bind(signal.signal_id.as_str())
        .bind(&subject.subject_id)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        Ok(subject)
    }

    async fn upsert_subject(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        canonical_state: &str,
        platform_classification: &str,
        latest_receipt_id: Option<&str>,
        latest_transition_id: Option<&str>,
        latest_callback_id: Option<&str>,
        latest_signal_id: Option<&str>,
        latest_signal_kind: Option<&str>,
        execution_correlation_id: Option<&str>,
        adapter_execution_reference: Option<&str>,
        external_observation_key: Option<&str>,
        expected_fact_snapshot: Option<&Value>,
        updated_at_ms: u64,
    ) -> Result<ReconSubject, ReconError> {
        let subject_id = sqlx::query_scalar::<_, String>(
            r#"
            INSERT INTO recon_core_subjects (
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, created_at_ms, updated_at_ms, scheduled_at_ms,
                next_reconcile_after_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,TRUE,$17,$18,$19,$20)
            ON CONFLICT (tenant_id, intent_id, job_id)
            DO UPDATE SET
                adapter_id = EXCLUDED.adapter_id,
                canonical_state = EXCLUDED.canonical_state,
                platform_classification = EXCLUDED.platform_classification,
                latest_receipt_id = COALESCE(EXCLUDED.latest_receipt_id, recon_core_subjects.latest_receipt_id),
                latest_transition_id = COALESCE(EXCLUDED.latest_transition_id, recon_core_subjects.latest_transition_id),
                latest_callback_id = COALESCE(EXCLUDED.latest_callback_id, recon_core_subjects.latest_callback_id),
                latest_signal_id = COALESCE(EXCLUDED.latest_signal_id, recon_core_subjects.latest_signal_id),
                latest_signal_kind = COALESCE(EXCLUDED.latest_signal_kind, recon_core_subjects.latest_signal_kind),
                execution_correlation_id = COALESCE(EXCLUDED.execution_correlation_id, recon_core_subjects.execution_correlation_id),
                adapter_execution_reference = COALESCE(EXCLUDED.adapter_execution_reference, recon_core_subjects.adapter_execution_reference),
                external_observation_key = COALESCE(EXCLUDED.external_observation_key, recon_core_subjects.external_observation_key),
                expected_fact_snapshot_json = COALESCE(EXCLUDED.expected_fact_snapshot_json, recon_core_subjects.expected_fact_snapshot_json),
                dirty = TRUE,
                updated_at_ms = EXCLUDED.updated_at_ms,
                scheduled_at_ms = COALESCE(EXCLUDED.scheduled_at_ms, recon_core_subjects.scheduled_at_ms),
                next_reconcile_after_ms = COALESCE(EXCLUDED.next_reconcile_after_ms, recon_core_subjects.next_reconcile_after_ms)
            RETURNING subject_id
            "#,
        )
        .bind(recon_subject_id_for_job_str(job_id))
        .bind(tenant_id)
        .bind(intent_id)
        .bind(job_id)
        .bind(adapter_id)
        .bind(canonical_state)
        .bind(platform_classification)
        .bind(latest_receipt_id)
        .bind(latest_transition_id)
        .bind(latest_callback_id)
        .bind(latest_signal_id)
        .bind(latest_signal_kind)
        .bind(execution_correlation_id)
        .bind(adapter_execution_reference)
        .bind(external_observation_key)
        .bind(expected_fact_snapshot)
        .bind(updated_at_ms as i64)
        .bind(updated_at_ms as i64)
        .bind(updated_at_ms as i64)
        .bind(updated_at_ms as i64)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let row = sqlx::query(
            r#"
            SELECT
                subject_id, tenant_id, intent_id, job_id, adapter_id,
                canonical_state, platform_classification, latest_receipt_id,
                latest_transition_id, latest_callback_id, latest_signal_id, latest_signal_kind,
                execution_correlation_id, adapter_execution_reference, external_observation_key,
                expected_fact_snapshot_json, dirty, recon_attempt_count, recon_retry_count,
                created_at_ms, updated_at_ms, scheduled_at_ms, next_reconcile_after_ms,
                last_reconciled_at_ms, last_recon_error, last_run_state
            FROM recon_core_subjects
            WHERE subject_id = $1
            LIMIT 1
            "#,
        )
        .bind(subject_id)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(map_subject_row(row))
    }
}

#[async_trait]
impl ReconIntakeRepository for PostgresReconStore {
    async fn claim_intake_signal(&self, signal: &ReconIntakeSignal) -> Result<bool, ReconError> {
        PostgresReconStore::claim_intake_signal(self, signal).await
    }

    async fn materialize_subject_from_signal(
        &self,
        signal: &ReconIntakeSignal,
    ) -> Result<ReconSubject, ReconError> {
        PostgresReconStore::materialize_subject_from_signal(self, signal).await
    }

    async fn load_subject_for_execution(
        &self,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
    ) -> Result<Option<ReconSubject>, ReconError> {
        PostgresReconStore::load_subject_for_execution(self, tenant_id, intent_id, job_id).await
    }
}

fn map_subject_row(row: sqlx::postgres::PgRow) -> ReconSubject {
    ReconSubject {
        subject_id: row.get("subject_id"),
        tenant_id: row.get("tenant_id"),
        intent_id: row.get("intent_id"),
        job_id: row.get("job_id"),
        adapter_id: row.get("adapter_id"),
        canonical_state: row.get("canonical_state"),
        platform_classification: row.get("platform_classification"),
        latest_receipt_id: row
            .try_get::<Option<String>, _>("latest_receipt_id")
            .ok()
            .flatten(),
        latest_transition_id: row
            .try_get::<Option<String>, _>("latest_transition_id")
            .ok()
            .flatten(),
        latest_callback_id: row
            .try_get::<Option<String>, _>("latest_callback_id")
            .ok()
            .flatten(),
        latest_signal_id: row
            .try_get::<Option<String>, _>("latest_signal_id")
            .ok()
            .flatten(),
        latest_signal_kind: row
            .try_get::<Option<String>, _>("latest_signal_kind")
            .ok()
            .flatten(),
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
            .try_get::<Option<Value>, _>("expected_fact_snapshot_json")
            .ok()
            .flatten(),
        dirty: row.get("dirty"),
        recon_attempt_count: row.get::<i32, _>("recon_attempt_count").max(0) as u32,
        recon_retry_count: row.get::<i32, _>("recon_retry_count").max(0) as u32,
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        scheduled_at_ms: row
            .try_get::<Option<i64>, _>("scheduled_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        next_reconcile_after_ms: row
            .try_get::<Option<i64>, _>("next_reconcile_after_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        last_reconciled_at_ms: row
            .try_get::<Option<i64>, _>("last_reconciled_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        last_recon_error: row
            .try_get::<Option<String>, _>("last_recon_error")
            .ok()
            .flatten(),
        last_run_state: row
            .try_get::<Option<String>, _>("last_run_state")
            .ok()
            .flatten()
            .and_then(|value| ReconRunState::parse(value.as_str())),
    }
}

fn map_run_row(row: sqlx::postgres::PgRow) -> ReconRun {
    ReconRun {
        run_id: row.get("run_id"),
        subject_id: row.get("subject_id"),
        tenant_id: row.get("tenant_id"),
        intent_id: row.get("intent_id"),
        job_id: row.get("job_id"),
        adapter_id: row.get("adapter_id"),
        rule_pack: row.get("rule_pack"),
        lifecycle_state: row
            .try_get::<String, _>("lifecycle_state")
            .ok()
            .and_then(|value| ReconRunState::parse(value.as_str()))
            .unwrap_or(ReconRunState::Completed),
        normalized_result: row
            .try_get::<Option<String>, _>("normalized_result")
            .ok()
            .flatten()
            .and_then(|value| crate::model::ReconResult::parse(value.as_str())),
        outcome: crate::model::ReconOutcome::parse(row.get::<String, _>("outcome").as_str())
            .unwrap_or(crate::model::ReconOutcome::ManualReviewRequired),
        summary: row.get("summary"),
        machine_reason: row.get("machine_reason"),
        expected_fact_count: row.get::<i32, _>("expected_fact_count").max(0) as u32,
        observed_fact_count: row.get::<i32, _>("observed_fact_count").max(0) as u32,
        matched_fact_count: row.get::<i32, _>("matched_fact_count").max(0) as u32,
        unmatched_fact_count: row.get::<i32, _>("unmatched_fact_count").max(0) as u32,
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        completed_at_ms: row
            .try_get::<Option<i64>, _>("completed_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        attempt_number: row.get::<i32, _>("attempt_number").max(0) as u32,
        retry_scheduled_at_ms: row
            .try_get::<Option<i64>, _>("retry_scheduled_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        last_error: row
            .try_get::<Option<String>, _>("last_error")
            .ok()
            .flatten(),
        exception_case_ids: row
            .try_get::<Value, _>("exception_case_ids")
            .ok()
            .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
            .unwrap_or_default(),
    }
}

fn map_receipt_row(row: sqlx::postgres::PgRow) -> ReconReceipt {
    let details: std::collections::BTreeMap<String, String> =
        serde_json::from_value(row.get::<Value, _>("details_json")).unwrap_or_default();
    ReconReceipt {
        recon_receipt_id: row.get("recon_receipt_id"),
        run_id: row.get("run_id"),
        subject_id: row.get("subject_id"),
        normalized_result: details
            .get("normalized_result")
            .and_then(|value| crate::model::ReconResult::parse(value.as_str())),
        outcome: crate::model::ReconOutcome::parse(row.get::<String, _>("outcome").as_str())
            .unwrap_or(crate::model::ReconOutcome::ManualReviewRequired),
        summary: row.get("summary"),
        details,
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
    }
}

#[async_trait]
impl ReconEngineStore for PostgresReconStore {
    async fn load_recon_context(&self, subject: &ReconSubject) -> Result<ReconContext, ReconError> {
        PostgresReconStore::load_recon_context(self, subject).await
    }

    async fn load_adapter_observations(
        &self,
        subject: &ReconSubject,
    ) -> Result<Vec<Value>, ReconError> {
        PostgresReconStore::load_adapter_observations(self, subject).await
    }

    async fn create_run(&self, run: &ReconRun) -> Result<(), ReconError> {
        PostgresReconStore::create_run(self, run).await
    }

    async fn append_run_state_transition(
        &self,
        transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError> {
        PostgresReconStore::append_run_state_transition(self, transition).await
    }

    async fn finalize_run(
        &self,
        subject: &ReconSubject,
        run: &ReconRun,
        receipt: &ReconReceipt,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        evidence: &ReconEvidenceSnapshot,
        final_transition: &ReconRunStateTransition,
    ) -> Result<(), ReconError> {
        PostgresReconStore::finalize_run(
            self,
            subject,
            run,
            receipt,
            expected,
            observed,
            evidence,
            final_transition,
        )
        .await
    }
}

fn sqlx_to_internal(err: sqlx::Error) -> ReconError {
    ReconError::Backend(err.to_string())
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}
