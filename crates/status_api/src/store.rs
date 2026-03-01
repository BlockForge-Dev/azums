use crate::error::StatusApiError;
use crate::model::{
    CallbackDeliveryAttemptRecord, CallbackDeliveryRecord, IntakeAuditRecord, JobListItem,
    RequestStatusResponse,
};
use execution_core::{
    CanonicalState, ExecutionJob, IntentId, NormalizedIntent, PlatformClassification, ReceiptEntry,
    StateTransition, TenantId,
};
use serde_json::Value;
use sqlx::{Error as SqlxError, PgPool, Row};
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
