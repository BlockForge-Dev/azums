use crate::error::StatusApiError;
use crate::model::{
    CallbackDeliveryAttemptRecord, CallbackDeliveryRecord, IntakeAuditRecord, JobListItem,
    ReconciliationRolloutExceptionMetrics, ReconciliationRolloutIntakeMetrics,
    ReconciliationRolloutLatencyMetrics, ReconciliationRolloutOutcomeMetrics,
    ReconciliationRolloutQueryMetrics, ReconciliationRolloutSummaryResponse,
    ReconciliationRolloutWindow, RequestStatusResponse,
};
use execution_core::{
    CanonicalState, ExecutionJob, IntentId, NormalizedIntent, PlatformClassification, ReceiptEntry,
    StateTransition, TenantId,
};
use serde_json::Value;
use sqlx::{Error as SqlxError, PgPool, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresStatusStore {
    pool: PgPool,
}

impl PostgresStatusStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ensure_schema(&self) -> Result<(), StatusApiError> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS status_api_query_audit (
                audit_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                principal_role TEXT NOT NULL,
                method TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                resource_id TEXT NULL,
                request_id TEXT NULL,
                allowed BOOLEAN NOT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS status_api_query_audit_tenant_created_idx
            ON status_api_query_audit(tenant_id, created_at DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS status_api_operator_action_audit (
                action_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                principal_role TEXT NOT NULL,
                action_type TEXT NOT NULL,
                target_intent_id TEXT NOT NULL,
                allowed BOOLEAN NOT NULL,
                reason TEXT NOT NULL,
                result_json JSONB NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS status_api_operator_action_tenant_created_idx
            ON status_api_operator_action_audit(tenant_id, created_at DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS callback_core_tenant_destinations (
                tenant_id TEXT PRIMARY KEY,
                delivery_url TEXT NOT NULL,
                bearer_token TEXT NULL,
                signature_secret TEXT NULL,
                signature_key_id TEXT NULL,
                timeout_ms BIGINT NOT NULL DEFAULT 10000,
                allow_private_destinations BOOLEAN NOT NULL DEFAULT FALSE,
                allowed_hosts TEXT NULL,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                updated_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS callback_core_tenant_destinations_enabled_idx
            ON callback_core_tenant_destinations(enabled, updated_at_ms DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(sqlx_to_internal)?;
        }

        Ok(())
    }

    pub async fn load_request_status(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Option<RequestStatusResponse>, StatusApiError> {
        let job_json = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT job_json
            FROM execution_core_jobs
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY updated_at_ms DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let Some(job_json) = job_json else {
            return Ok(None);
        };

        let job: ExecutionJob = serde_json::from_value(job_json).map_err(|err| {
            StatusApiError::Internal(format!("failed to parse execution job snapshot: {err}"))
        })?;

        let intent_row = sqlx::query(
            r#"
            SELECT intent_kind, intent_json
            FROM execution_core_intents
            WHERE tenant_id = $1
              AND intent_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let mut request_id = job.intent_id.to_string();
        let mut kind = None;
        let mut correlation_id = None;
        let mut idempotency_key = None;
        let mut auth_context = None;

        if let Some(row) = intent_row {
            kind = Some(row.get::<String, _>("intent_kind"));
            let intent_json: Value = row.get("intent_json");
            if let Ok(intent) = serde_json::from_value::<NormalizedIntent>(intent_json) {
                if let Some(value) = intent.request_id {
                    request_id = value.to_string();
                } else if let Some(value) = intent.metadata.get("request_id") {
                    request_id = value.clone();
                }
                if let Some(value) = intent.correlation_id {
                    correlation_id = Some(value);
                } else {
                    correlation_id = intent.metadata.get("correlation_id").cloned();
                }
                if let Some(value) = intent.idempotency_key {
                    idempotency_key = Some(value);
                } else {
                    idempotency_key = intent.metadata.get("idempotency_key").cloned();
                }
                auth_context = intent.auth_context;
            }
        }

        Ok(Some(RequestStatusResponse {
            tenant_id: job.tenant_id.to_string(),
            request_id,
            intent_id: job.intent_id.to_string(),
            kind,
            correlation_id,
            idempotency_key,
            auth_context,
            state: job.state,
            classification: derive_classification(&job),
            adapter_id: job.adapter_id.to_string(),
            attempt: job.attempt,
            max_attempts: job.max_attempts,
            replay_count: job.replay_count,
            replay_of_job_id: job.replay_of_job_id.as_ref().map(ToString::to_string),
            last_failure: job.last_failure,
            created_at_ms: job.created_at_ms,
            updated_at_ms: job.updated_at_ms,
        }))
    }

    pub async fn load_receipts(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Vec<ReceiptEntry>, StatusApiError> {
        let rows = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT receipt_json
            FROM execution_core_receipts
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY occurred_at_ms ASC
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry: ReceiptEntry = serde_json::from_value(row).map_err(|err| {
                StatusApiError::Internal(format!("failed to parse receipt row: {err}"))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub async fn load_receipt_by_id(
        &self,
        tenant_id: &TenantId,
        receipt_id: &str,
    ) -> Result<Option<ReceiptEntry>, StatusApiError> {
        let row = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT receipt_json
            FROM execution_core_receipts
            WHERE tenant_id = $1
              AND receipt_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(receipt_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let entry: ReceiptEntry = serde_json::from_value(row).map_err(|err| {
            StatusApiError::Internal(format!("failed to parse receipt row: {err}"))
        })?;
        Ok(Some(entry))
    }

    pub async fn load_history(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
    ) -> Result<Vec<StateTransition>, StatusApiError> {
        let rows = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT transition_json
            FROM execution_core_state_transitions
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY occurred_at_ms ASC
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let entry: StateTransition = serde_json::from_value(row).map_err(|err| {
                StatusApiError::Internal(format!("failed to parse transition row: {err}"))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    pub async fn load_callback_history(
        &self,
        tenant_id: &TenantId,
        intent_id: &IntentId,
        include_attempts: bool,
        attempt_limit: u32,
    ) -> Result<Vec<CallbackDeliveryRecord>, StatusApiError> {
        let delivery_rows = match sqlx::query(
            r#"
            SELECT
                callback_id,
                state,
                attempts,
                last_http_status,
                last_error_class,
                last_error_message,
                next_attempt_at_ms,
                delivered_at_ms,
                updated_at_ms
            FROM callback_core_deliveries
            WHERE tenant_id = $1
              AND intent_id = $2
            ORDER BY updated_at_ms DESC
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(intent_id.as_str())
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(err) if is_undefined_table(&err) => return Ok(Vec::new()),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        let mut deliveries = Vec::with_capacity(delivery_rows.len());
        for row in delivery_rows {
            let callback_id: String = row.get("callback_id");
            let attempt_history = if include_attempts {
                self.load_delivery_attempts(&callback_id, attempt_limit)
                    .await?
            } else {
                Vec::new()
            };

            deliveries.push(CallbackDeliveryRecord {
                callback_id,
                state: row.get::<String, _>("state"),
                attempts: row.get::<i32, _>("attempts").max(0) as u32,
                last_http_status: row
                    .get::<Option<i32>, _>("last_http_status")
                    .map(|v| v.max(0) as u16),
                last_error_class: row.get("last_error_class"),
                last_error_message: row.get("last_error_message"),
                next_attempt_at_ms: row
                    .get::<Option<i64>, _>("next_attempt_at_ms")
                    .map(|v| v.max(0) as u64),
                delivered_at_ms: row
                    .get::<Option<i64>, _>("delivered_at_ms")
                    .map(|v| v.max(0) as u64),
                updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
                attempt_history,
            });
        }

        Ok(deliveries)
    }

    pub async fn load_callback_delivery(
        &self,
        tenant_id: &TenantId,
        callback_id: &str,
        include_attempts: bool,
        attempt_limit: u32,
    ) -> Result<Option<(IntentId, CallbackDeliveryRecord)>, StatusApiError> {
        let row = match sqlx::query(
            r#"
            SELECT
                intent_id,
                callback_id,
                state,
                attempts,
                last_http_status,
                last_error_class,
                last_error_message,
                next_attempt_at_ms,
                delivered_at_ms,
                updated_at_ms
            FROM callback_core_deliveries
            WHERE tenant_id = $1
              AND callback_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(callback_id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(row) => row,
            Err(err) if is_undefined_table(&err) => return Ok(None),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        let Some(row) = row else {
            return Ok(None);
        };

        let attempt_history = if include_attempts {
            self.load_delivery_attempts(callback_id, attempt_limit)
                .await?
        } else {
            Vec::new()
        };

        let intent_id: String = row.get("intent_id");
        let callback = CallbackDeliveryRecord {
            callback_id: row.get("callback_id"),
            state: row.get("state"),
            attempts: row.get::<i32, _>("attempts").max(0) as u32,
            last_http_status: row
                .get::<Option<i32>, _>("last_http_status")
                .map(|value| value.max(0) as u16),
            last_error_class: row.get("last_error_class"),
            last_error_message: row.get("last_error_message"),
            next_attempt_at_ms: row
                .get::<Option<i64>, _>("next_attempt_at_ms")
                .map(|value| value.max(0) as u64),
            delivered_at_ms: row
                .get::<Option<i64>, _>("delivered_at_ms")
                .map(|value| value.max(0) as u64),
            updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
            attempt_history,
        };

        Ok(Some((IntentId::from(intent_id), callback)))
    }

    pub async fn load_callback_destination(
        &self,
        tenant_id: &TenantId,
    ) -> Result<Option<StoredCallbackDestination>, StatusApiError> {
        let row = match sqlx::query(
            r#"
            SELECT
                tenant_id,
                delivery_url,
                bearer_token,
                signature_secret,
                signature_key_id,
                timeout_ms,
                allow_private_destinations,
                allowed_hosts,
                enabled,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM callback_core_tenant_destinations
            WHERE tenant_id = $1
            LIMIT 1
            "#,
        )
        .bind(tenant_id.as_str())
        .fetch_optional(&self.pool)
        .await
        {
            Ok(row) => row,
            Err(err) if is_undefined_table(&err) => return Ok(None),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        Ok(row.map(parse_callback_destination_row))
    }

    pub async fn upsert_callback_destination(
        &self,
        input: &UpsertCallbackDestinationStoreInput,
    ) -> Result<StoredCallbackDestination, StatusApiError> {
        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        let existing_created_at = self
            .load_callback_destination(&TenantId::from(input.tenant_id.clone()))
            .await?
            .map(|value| value.created_at_ms)
            .unwrap_or(now_ms);

        sqlx::query(
            r#"
            INSERT INTO callback_core_tenant_destinations (
                tenant_id,
                delivery_url,
                bearer_token,
                signature_secret,
                signature_key_id,
                timeout_ms,
                allow_private_destinations,
                allowed_hosts,
                enabled,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (tenant_id)
            DO UPDATE SET
                delivery_url = EXCLUDED.delivery_url,
                bearer_token = EXCLUDED.bearer_token,
                signature_secret = EXCLUDED.signature_secret,
                signature_key_id = EXCLUDED.signature_key_id,
                timeout_ms = EXCLUDED.timeout_ms,
                allow_private_destinations = EXCLUDED.allow_private_destinations,
                allowed_hosts = EXCLUDED.allowed_hosts,
                enabled = EXCLUDED.enabled,
                updated_by_principal_id = EXCLUDED.updated_by_principal_id,
                updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(&input.tenant_id)
        .bind(&input.delivery_url)
        .bind(&input.bearer_token)
        .bind(&input.signature_secret)
        .bind(&input.signature_key_id)
        .bind(input.timeout_ms as i64)
        .bind(input.allow_private_destinations)
        .bind(&input.allowed_hosts)
        .bind(input.enabled)
        .bind(&input.updated_by_principal_id)
        .bind(existing_created_at as i64)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        self.load_callback_destination(&TenantId::from(input.tenant_id.clone()))
            .await?
            .ok_or_else(|| {
                StatusApiError::Internal(
                    "callback destination upsert succeeded but row could not be loaded".to_owned(),
                )
            })
    }

    pub async fn delete_callback_destination(
        &self,
        tenant_id: &TenantId,
    ) -> Result<bool, StatusApiError> {
        let result = match sqlx::query(
            r#"
            DELETE FROM callback_core_tenant_destinations
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id.as_str())
        .execute(&self.pool)
        .await
        {
            Ok(result) => result,
            Err(err) if is_undefined_table(&err) => return Ok(false),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_jobs(
        &self,
        tenant_id: &TenantId,
        state_filter: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<JobListItem>, StatusApiError> {
        let rows = sqlx::query_scalar::<_, Value>(
            r#"
            SELECT job_json
            FROM execution_core_jobs
            WHERE tenant_id = $1
              AND (
                $2::text IS NULL
                OR LOWER(COALESCE(job_json ->> 'state', '')) = ANY(string_to_array(LOWER($2), '|'))
              )
            ORDER BY updated_at_ms DESC
            LIMIT $3
            OFFSET $4
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(state_filter)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let job: ExecutionJob = serde_json::from_value(row).map_err(|err| {
                StatusApiError::Internal(format!("failed to parse execution job in list: {err}"))
            })?;
            jobs.push(JobListItem {
                job_id: job.job_id.to_string(),
                intent_id: job.intent_id.to_string(),
                adapter_id: job.adapter_id.to_string(),
                state: job.state,
                classification: derive_classification(&job),
                attempt: job.attempt,
                max_attempts: job.max_attempts,
                replay_count: job.replay_count,
                replay_of_job_id: job.replay_of_job_id.as_ref().map(ToString::to_string),
                next_retry_at_ms: job.next_retry_at_ms,
                updated_at_ms: job.updated_at_ms,
                created_at_ms: job.created_at_ms,
                failure_code: job.last_failure.as_ref().map(|f| f.code.clone()),
                failure_message: job.last_failure.as_ref().map(|f| f.message.clone()),
            });
        }

        Ok(jobs)
    }

    pub async fn load_intake_audits(
        &self,
        tenant_id: &TenantId,
        validation_result: Option<&str>,
        channel: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<IntakeAuditRecord>, StatusApiError> {
        let rows = match sqlx::query(
            r#"
            SELECT
                audit_id::text AS audit_id,
                request_id,
                channel,
                endpoint,
                method,
                principal_id,
                submitter_kind,
                auth_scheme,
                intent_kind,
                correlation_id,
                idempotency_key,
                idempotency_decision,
                validation_result,
                rejection_reason,
                error_status,
                error_message,
                accepted_intent_id,
                accepted_job_id,
                details_json,
                created_at_ms
            FROM ingress_api_intake_audits
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR LOWER(validation_result) = LOWER($2))
              AND ($3::text IS NULL OR LOWER(channel) = LOWER($3))
            ORDER BY created_at_ms DESC
            LIMIT $4
            OFFSET $5
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(validation_result)
        .bind(channel)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(err) if is_undefined_table(&err) => return Ok(Vec::new()),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        let mut audits = Vec::with_capacity(rows.len());
        for row in rows {
            audits.push(IntakeAuditRecord {
                audit_id: row.get("audit_id"),
                request_id: row.get("request_id"),
                channel: row.get("channel"),
                endpoint: row.get("endpoint"),
                method: row.get("method"),
                principal_id: row.get("principal_id"),
                submitter_kind: row.get("submitter_kind"),
                auth_scheme: row.get("auth_scheme"),
                intent_kind: row.get("intent_kind"),
                correlation_id: row.get("correlation_id"),
                idempotency_key: row.get("idempotency_key"),
                idempotency_decision: row.get("idempotency_decision"),
                validation_result: row.get("validation_result"),
                rejection_reason: row.get("rejection_reason"),
                error_status: row
                    .get::<Option<i32>, _>("error_status")
                    .map(|value| value.max(0) as u16),
                error_message: row.get("error_message"),
                accepted_intent_id: row.get("accepted_intent_id"),
                accepted_job_id: row.get("accepted_job_id"),
                details_json: row.get("details_json"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
            });
        }

        Ok(audits)
    }

    pub async fn load_reconciliation_rollout_summary(
        &self,
        tenant_id: &TenantId,
        lookback_hours: u32,
    ) -> Result<ReconciliationRolloutSummaryResponse, StatusApiError> {
        let generated_at_ms = current_unix_ms();
        let window_ms = u64::from(lookback_hours) * 60 * 60 * 1000;
        let started_at_ms = generated_at_ms.saturating_sub(window_ms);
        let started_at_ms_i64 = started_at_ms.min(i64::MAX as u64) as i64;
        let generated_at_ms_i64 = generated_at_ms.min(i64::MAX as u64) as i64;

        let intake_row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (
                    WHERE COALESCE((receipt_json->>'reconciliation_eligible')::boolean, FALSE)
                ) AS eligible_execution_receipts,
                (
                    SELECT COUNT(*)
                    FROM platform_recon_intake_signals
                    WHERE tenant_id = $1
                      AND occurred_at_ms >= $2
                ) AS intake_signals,
                (
                    SELECT COUNT(*)
                    FROM recon_core_subjects
                    WHERE tenant_id = $1
                      AND updated_at_ms >= $2
                ) AS subjects_total,
                (
                    SELECT COUNT(*)
                    FROM recon_core_subjects
                    WHERE tenant_id = $1
                      AND updated_at_ms >= $2
                      AND dirty = TRUE
                ) AS dirty_subjects,
                (
                    SELECT COUNT(*)
                    FROM recon_core_subjects
                    WHERE tenant_id = $1
                      AND updated_at_ms >= $2
                      AND (
                          COALESCE(last_run_state, '') = 'retry_scheduled'
                          OR COALESCE(next_reconcile_after_ms, 0) > $3
                      )
                ) AS retry_scheduled_subjects
            FROM execution_core_receipts
            WHERE tenant_id = $1
              AND occurred_at_ms >= $2
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(started_at_ms_i64)
        .bind(generated_at_ms_i64)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let outcomes_row = sqlx::query(
            r#"
            WITH latest_outcomes AS (
                SELECT DISTINCT ON (subject_id)
                    subject_id,
                    COALESCE(NULLIF(normalized_result, ''), outcome) AS result
                FROM recon_core_outcomes
                WHERE tenant_id = $1
                  AND created_at_ms >= $2
                ORDER BY subject_id, created_at_ms DESC, outcome_id DESC
            ),
            pending_subjects AS (
                SELECT COUNT(*) AS pending_count
                FROM recon_core_subjects
                WHERE tenant_id = $1
                  AND updated_at_ms >= $2
                  AND (
                      dirty = TRUE
                      OR COALESCE(last_run_state, '') IN (
                          'queued',
                          'collecting_observations',
                          'matching',
                          'writing_receipt',
                          'retry_scheduled'
                      )
                  )
            )
            SELECT
                COUNT(*) FILTER (WHERE result = 'matched') AS matched,
                COUNT(*) FILTER (WHERE result = 'partially_matched') AS partially_matched,
                COUNT(*) FILTER (WHERE result = 'unmatched') AS unmatched,
                COUNT(*) FILTER (WHERE result = 'stale') AS stale,
                COUNT(*) FILTER (WHERE result = 'manual_review_required') AS manual_review_required,
                (SELECT pending_count FROM pending_subjects) AS pending_observation
            FROM latest_outcomes
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(started_at_ms_i64)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let exceptions_row = sqlx::query(
            r#"
            SELECT
                COUNT(*) AS total_cases,
                COUNT(*) FILTER (
                    WHERE state NOT IN ('resolved', 'dismissed', 'false_positive')
                ) AS unresolved_cases,
                COUNT(*) FILTER (
                    WHERE state NOT IN ('resolved', 'dismissed', 'false_positive')
                      AND severity IN ('high', 'critical')
                ) AS high_or_critical_cases,
                COUNT(*) FILTER (WHERE state = 'false_positive') AS false_positive_cases
            FROM exception_cases
            WHERE tenant_id = $1
              AND updated_at_ms >= $2
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(started_at_ms_i64)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let latency_row = sqlx::query(
            r#"
            WITH eligible_execution AS (
                SELECT
                    tenant_id,
                    intent_id,
                    job_id,
                    MIN(occurred_at_ms) AS first_execution_at_ms
                FROM execution_core_receipts
                WHERE tenant_id = $1
                  AND occurred_at_ms >= $2
                  AND COALESCE((receipt_json->>'reconciliation_eligible')::boolean, FALSE)
                GROUP BY tenant_id, intent_id, job_id
            ),
            first_reconciliation AS (
                SELECT
                    s.tenant_id,
                    s.intent_id,
                    s.job_id,
                    MIN(r.created_at_ms) AS first_recon_at_ms
                FROM recon_core_receipts r
                INNER JOIN recon_core_subjects s
                    ON s.subject_id = r.subject_id
                WHERE s.tenant_id = $1
                  AND r.created_at_ms >= $2
                GROUP BY s.tenant_id, s.intent_id, s.job_id
            ),
            recon_latency AS (
                SELECT
                    GREATEST(0, first_reconciliation.first_recon_at_ms - eligible_execution.first_execution_at_ms) AS latency_ms
                FROM eligible_execution
                INNER JOIN first_reconciliation
                    ON first_reconciliation.tenant_id = eligible_execution.tenant_id
                   AND first_reconciliation.intent_id = eligible_execution.intent_id
                   AND first_reconciliation.job_id = eligible_execution.job_id
            ),
            first_resolution AS (
                SELECT
                    case_id,
                    MIN(created_at_ms) FILTER (
                        WHERE resolution_state IN ('resolved', 'dismissed', 'false_positive')
                    ) AS resolved_at_ms
                FROM exception_resolution_history
                GROUP BY case_id
            ),
            operator_latency AS (
                SELECT
                    GREATEST(0, first_resolution.resolved_at_ms - exception_cases.created_at_ms) AS handling_ms
                FROM exception_cases
                INNER JOIN first_resolution
                    ON first_resolution.case_id = exception_cases.case_id
                WHERE exception_cases.tenant_id = $1
                  AND exception_cases.updated_at_ms >= $2
                  AND first_resolution.resolved_at_ms IS NOT NULL
            )
            SELECT
                (SELECT AVG(latency_ms)::double precision FROM recon_latency) AS avg_recon_latency_ms,
                (SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms)::double precision FROM recon_latency) AS p95_recon_latency_ms,
                (SELECT MAX(latency_ms)::double precision FROM recon_latency) AS max_recon_latency_ms,
                (SELECT AVG(handling_ms)::double precision FROM operator_latency) AS avg_operator_handling_ms,
                (SELECT percentile_cont(0.95) WITHIN GROUP (ORDER BY handling_ms)::double precision FROM operator_latency) AS p95_operator_handling_ms
            "#,
        )
        .bind(tenant_id.as_str())
        .bind(started_at_ms_i64)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        let intake = ReconciliationRolloutIntakeMetrics {
            eligible_execution_receipts: row_u64(&intake_row, "eligible_execution_receipts"),
            intake_signals: row_u64(&intake_row, "intake_signals"),
            subjects_total: row_u64(&intake_row, "subjects_total"),
            dirty_subjects: row_u64(&intake_row, "dirty_subjects"),
            retry_scheduled_subjects: row_u64(&intake_row, "retry_scheduled_subjects"),
        };

        let outcomes = ReconciliationRolloutOutcomeMetrics {
            matched: row_u64(&outcomes_row, "matched"),
            partially_matched: row_u64(&outcomes_row, "partially_matched"),
            unmatched: row_u64(&outcomes_row, "unmatched"),
            pending_observation: row_u64(&outcomes_row, "pending_observation"),
            stale: row_u64(&outcomes_row, "stale"),
            manual_review_required: row_u64(&outcomes_row, "manual_review_required"),
        };

        let total_cases = row_u64(&exceptions_row, "total_cases");
        let false_positive_cases = row_u64(&exceptions_row, "false_positive_cases");
        let latest_outcome_total = outcomes.matched
            + outcomes.partially_matched
            + outcomes.unmatched
            + outcomes.stale
            + outcomes.manual_review_required;
        let exceptions = ReconciliationRolloutExceptionMetrics {
            total_cases,
            unresolved_cases: row_u64(&exceptions_row, "unresolved_cases"),
            high_or_critical_cases: row_u64(&exceptions_row, "high_or_critical_cases"),
            false_positive_cases,
            exception_rate: ratio(total_cases, latest_outcome_total.max(intake.subjects_total)),
            false_positive_rate: ratio(false_positive_cases, total_cases),
            stale_rate: ratio(outcomes.stale, latest_outcome_total),
        };

        let latency = ReconciliationRolloutLatencyMetrics {
            avg_recon_latency_ms: row_opt_u64(&latency_row, "avg_recon_latency_ms"),
            p95_recon_latency_ms: row_opt_u64(&latency_row, "p95_recon_latency_ms"),
            max_recon_latency_ms: row_opt_u64(&latency_row, "max_recon_latency_ms"),
            avg_operator_handling_ms: row_opt_u64(&latency_row, "avg_operator_handling_ms"),
            p95_operator_handling_ms: row_opt_u64(&latency_row, "p95_operator_handling_ms"),
        };

        Ok(ReconciliationRolloutSummaryResponse {
            tenant_id: tenant_id.to_string(),
            window: ReconciliationRolloutWindow {
                lookback_hours,
                started_at_ms,
                generated_at_ms,
            },
            intake,
            outcomes,
            exceptions,
            latency,
            queries: ReconciliationRolloutQueryMetrics {
                sampled_intent_id: None,
                exception_index_query_ms: None,
                unified_request_query_ms: None,
            },
        })
    }

    pub async fn record_query_audit(&self, entry: &QueryAuditEntry) -> Result<(), StatusApiError> {
        sqlx::query(
            r#"
            INSERT INTO status_api_query_audit (
                audit_id,
                tenant_id,
                principal_id,
                principal_role,
                method,
                endpoint,
                resource_id,
                request_id,
                allowed,
                details_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(entry.audit_id)
        .bind(&entry.tenant_id)
        .bind(&entry.principal_id)
        .bind(&entry.principal_role)
        .bind(&entry.method)
        .bind(&entry.endpoint)
        .bind(&entry.resource_id)
        .bind(&entry.request_id)
        .bind(entry.allowed)
        .bind(&entry.details_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    pub async fn record_operator_action(
        &self,
        entry: &OperatorActionAuditEntry,
    ) -> Result<(), StatusApiError> {
        sqlx::query(
            r#"
            INSERT INTO status_api_operator_action_audit (
                action_id,
                tenant_id,
                principal_id,
                principal_role,
                action_type,
                target_intent_id,
                allowed,
                reason,
                result_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(entry.action_id)
        .bind(&entry.tenant_id)
        .bind(&entry.principal_id)
        .bind(&entry.principal_role)
        .bind(&entry.action_type)
        .bind(&entry.target_intent_id)
        .bind(entry.allowed)
        .bind(&entry.reason)
        .bind(&entry.result_json)
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    async fn load_delivery_attempts(
        &self,
        callback_id: &str,
        attempt_limit: u32,
    ) -> Result<Vec<CallbackDeliveryAttemptRecord>, StatusApiError> {
        let rows = match sqlx::query(
            r#"
            SELECT
                attempt_no,
                outcome,
                failure_class,
                error_message,
                http_status,
                response_excerpt,
                occurred_at_ms
            FROM callback_core_delivery_attempts
            WHERE callback_id = $1
            ORDER BY occurred_at_ms DESC
            LIMIT $2
            "#,
        )
        .bind(callback_id)
        .bind(attempt_limit as i64)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(err) if is_undefined_table(&err) => return Ok(Vec::new()),
            Err(err) => return Err(sqlx_to_internal(err)),
        };

        let mut attempts = Vec::with_capacity(rows.len());
        for row in rows {
            attempts.push(CallbackDeliveryAttemptRecord {
                attempt_no: row.get::<i32, _>("attempt_no").max(0) as u32,
                outcome: row.get("outcome"),
                failure_class: row.get("failure_class"),
                error_message: row.get("error_message"),
                http_status: row
                    .get::<Option<i32>, _>("http_status")
                    .map(|v| v.max(0) as u16),
                response_excerpt: row.get("response_excerpt"),
                occurred_at_ms: row.get::<i64, _>("occurred_at_ms").max(0) as u64,
            });
        }
        Ok(attempts)
    }
}

#[derive(Debug, Clone)]
pub struct QueryAuditEntry {
    pub audit_id: Uuid,
    pub tenant_id: String,
    pub principal_id: String,
    pub principal_role: String,
    pub method: String,
    pub endpoint: String,
    pub resource_id: Option<String>,
    pub request_id: Option<String>,
    pub allowed: bool,
    pub details_json: Value,
}

#[derive(Debug, Clone)]
pub struct OperatorActionAuditEntry {
    pub action_id: Uuid,
    pub tenant_id: String,
    pub principal_id: String,
    pub principal_role: String,
    pub action_type: String,
    pub target_intent_id: String,
    pub allowed: bool,
    pub reason: String,
    pub result_json: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct StoredCallbackDestination {
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

#[derive(Debug, Clone)]
pub struct UpsertCallbackDestinationStoreInput {
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
}

pub fn role_label(role: execution_core::OperatorRole) -> &'static str {
    match role {
        execution_core::OperatorRole::Viewer => "viewer",
        execution_core::OperatorRole::Operator => "operator",
        execution_core::OperatorRole::Admin => "admin",
    }
}

pub fn normalize_state_filter(raw: Option<String>) -> Result<Option<String>, StatusApiError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }

    let mapped = match normalized.as_str() {
        "received" | "submitted" => "received|submitted",
        "validated" => "validated",
        "rejected" => "rejected",
        "queued" | "routed" => "queued|routed",
        "leased" => "leased",
        "executing" | "dispatching" => "executing|dispatching",
        "retryscheduled" | "retry_scheduled" => "retry_scheduled|retryscheduled",
        "succeeded" => "succeeded",
        "failedterminal" | "failed_terminal" | "terminalfailure" | "terminal_failure"
        | "blocked" | "manualreview" | "manual_review" => {
            "failed_terminal|terminalfailure|blocked|manualreview"
        }
        "deadlettered" | "dead_lettered" => "dead_lettered|deadlettered",
        "replayed" => "replayed",
        _ => {
            return Err(StatusApiError::BadRequest(format!(
                "unsupported state filter `{raw}`"
            )))
        }
    };
    Ok(Some(mapped.to_owned()))
}

fn parse_callback_destination_row(row: sqlx::postgres::PgRow) -> StoredCallbackDestination {
    StoredCallbackDestination {
        tenant_id: row.get("tenant_id"),
        delivery_url: row.get("delivery_url"),
        bearer_token: row.get("bearer_token"),
        signature_secret: row.get("signature_secret"),
        signature_key_id: row.get("signature_key_id"),
        timeout_ms: row.get::<i64, _>("timeout_ms").max(1) as u64,
        allow_private_destinations: row.get("allow_private_destinations"),
        allowed_hosts: row.get("allowed_hosts"),
        enabled: row.get("enabled"),
        updated_by_principal_id: row.get("updated_by_principal_id"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
    }
}

fn derive_classification(job: &ExecutionJob) -> PlatformClassification {
    match job.state {
        CanonicalState::Succeeded => PlatformClassification::Success,
        CanonicalState::FailedTerminal => job
            .last_failure
            .as_ref()
            .map(|failure| failure.classification)
            .unwrap_or(PlatformClassification::TerminalFailure),
        CanonicalState::DeadLettered => PlatformClassification::TerminalFailure,
        CanonicalState::Rejected => PlatformClassification::TerminalFailure,
        CanonicalState::RetryScheduled => job
            .last_failure
            .as_ref()
            .map(|failure| failure.classification)
            .unwrap_or(PlatformClassification::RetryableFailure),
        CanonicalState::Received
        | CanonicalState::Validated
        | CanonicalState::Queued
        | CanonicalState::Leased
        | CanonicalState::Executing
        | CanonicalState::Replayed => PlatformClassification::Success,
    }
}

fn is_undefined_table(err: &SqlxError) -> bool {
    match err {
        SqlxError::Database(db_err) => db_err.code().as_deref() == Some("42P01"),
        _ => false,
    }
}

fn sqlx_to_internal(err: SqlxError) -> StatusApiError {
    StatusApiError::Internal(err.to_string())
}

fn row_u64(row: &sqlx::postgres::PgRow, column: &str) -> u64 {
    row.try_get::<i64, _>(column).unwrap_or_default().max(0) as u64
}

fn row_opt_u64(row: &sqlx::postgres::PgRow, column: &str) -> Option<u64> {
    row.try_get::<Option<f64>, _>(column)
        .ok()
        .flatten()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.round() as u64)
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_store_deserializes_legacy_receipt_rows() {
        let entry: ReceiptEntry = serde_json::from_value(serde_json::json!({
            "receipt_id": "receipt_legacy",
            "tenant_id": "tenant_a",
            "intent_id": "intent_legacy",
            "job_id": "job_legacy",
            "attempt_no": 0,
            "state": "received",
            "classification": "Success",
            "summary": "legacy receipt",
            "details": { "reason_code": "request_received" },
            "occurred_at_ms": 1
        }))
        .unwrap();

        assert_eq!(entry.receipt_version, 1);
        assert!(entry.recon_subject_id.is_none());
        assert!(!entry.reconciliation_eligible);
        assert!(entry.agent_action.is_none());
        assert!(entry.agent_identity.is_none());
        assert!(entry.runtime_identity.is_none());
        assert!(entry.policy_decision.is_none());
        assert!(entry.approval_result.is_none());
        assert!(entry.grant_reference.is_none());
        assert!(entry.execution_mode.is_none());
        assert!(entry.connector_outcome.is_none());
        assert!(entry.recon_linkage.is_none());
    }

    #[test]
    fn status_store_deserializes_recon_upgraded_receipt_rows() {
        let entry: ReceiptEntry = serde_json::from_value(serde_json::json!({
            "receipt_id": "receipt_v2",
            "tenant_id": "tenant_a",
            "intent_id": "intent_v2",
            "job_id": "job_v2",
            "receipt_version": 2,
            "recon_subject_id": "reconsub_job_v2",
            "reconciliation_eligible": true,
            "execution_correlation_id": "corr-1",
            "adapter_execution_reference": "sig-final",
            "external_observation_key": "sig-final",
            "expected_fact_snapshot": {
                "version": 1,
                "canonical_state": "succeeded"
            },
            "attempt_no": 1,
            "state": "succeeded",
            "classification": "Success",
            "summary": "adapter execution succeeded",
            "details": {
                "reason_code": "adapter_succeeded"
            },
            "occurred_at_ms": 2
        }))
        .unwrap();

        assert_eq!(entry.receipt_version, 2);
        assert_eq!(entry.recon_subject_id.as_deref(), Some("reconsub_job_v2"));
        assert!(entry.reconciliation_eligible);
        assert_eq!(
            entry.adapter_execution_reference.as_deref(),
            Some("sig-final")
        );
        assert!(entry.agent_action.is_none());
        assert!(entry.approval_result.is_none());
    }

    #[test]
    fn status_store_deserializes_agent_receipt_rows() {
        let entry: ReceiptEntry = serde_json::from_value(serde_json::json!({
            "receipt_id": "receipt_v3",
            "tenant_id": "tenant_a",
            "intent_id": "intent_v3",
            "job_id": "job_v3",
            "receipt_version": 3,
            "recon_subject_id": "reconsub_job_v3",
            "reconciliation_eligible": true,
            "execution_correlation_id": "corr-3",
            "adapter_execution_reference": "sig-final",
            "external_observation_key": "sig-final",
            "attempt_no": 1,
            "state": "succeeded",
            "classification": "Success",
            "summary": "agent execution succeeded",
            "details": {
                "reason_code": "adapter_succeeded"
            },
            "agent_action": {
                "action_request_id": "act_123",
                "intent_type": "transfer",
                "adapter_type": "solana_adapter",
                "requested_scope": ["payments", "treasury"],
                "effective_scope": ["payments"],
                "reason": "vendor payout",
                "submitted_by": "ops-bot"
            },
            "agent_identity": {
                "agent_id": "agent_123",
                "environment_id": "env_prod",
                "environment_kind": "production",
                "status": "active",
                "trust_tier": "reviewed",
                "risk_tier": "high",
                "owner_team": "ops"
            },
            "runtime_identity": {
                "runtime_type": "slack",
                "runtime_identity": "runtime://slack/bot",
                "submitter_kind": "agent_runtime",
                "channel": "agent_gateway"
            },
            "policy_decision": {
                "decision": "require_approval",
                "explanation": "prod transfers require approval",
                "bundle_id": "bundle_prod",
                "bundle_version": 7
            },
            "approval_result": {
                "result": "approved",
                "approval_request_id": "apr_123",
                "state": "approved",
                "required_approvals": 1,
                "approvals_received": 1,
                "approved_by": ["alice"]
            },
            "grant_reference": {
                "grant_id": "grant_123",
                "source_action_request_id": "act_123",
                "source_approval_request_id": "apr_123",
                "source_policy_bundle_id": "bundle_prod",
                "source_policy_bundle_version": 7,
                "expires_at_ms": 45000
            },
            "execution_mode": {
                "mode": "mode_c_protected_execution",
                "owner": "azums_protected_execution",
                "effective_policy": "sponsored",
                "base_policy": "customer_signed",
                "signing_mode": "sponsored",
                "payer_source": "azums",
                "fee_payer": "wallet_1"
            },
            "connector_outcome": {
                "status": "queued",
                "connector_type": "slack",
                "binding_id": "binding_slack_1",
                "reference": "slack_action_123"
            },
            "recon_linkage": {
                "recon_subject_id": "reconsub_job_v3",
                "reconciliation_eligible": true,
                "execution_correlation_id": "corr-3",
                "adapter_execution_reference": "sig-final",
                "external_observation_key": "sig-final",
                "connector_type": "slack",
                "connector_binding_id": "binding_slack_1",
                "connector_reference": "slack_action_123"
            },
            "occurred_at_ms": 3
        }))
        .unwrap();

        assert_eq!(entry.receipt_version, 3);
        assert_eq!(
            entry
                .agent_action
                .as_ref()
                .and_then(|value| value.intent_type.as_deref()),
            Some("transfer")
        );
        assert_eq!(
            entry
                .approval_result
                .as_ref()
                .map(|value| value.result.as_str()),
            Some("approved")
        );
        assert_eq!(
            entry
                .connector_outcome
                .as_ref()
                .map(|value| value.status.as_str()),
            Some("queued")
        );
        assert_eq!(
            entry
                .connector_outcome
                .as_ref()
                .and_then(|value| value.reference.as_deref()),
            Some("slack_action_123")
        );
        assert_eq!(
            entry
                .recon_linkage
                .as_ref()
                .and_then(|value| value.connector_reference.as_deref()),
            Some("slack_action_123")
        );
    }
}
