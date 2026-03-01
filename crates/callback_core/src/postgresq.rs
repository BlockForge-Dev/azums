use crate::dispatcher::CallbackDispatcher;
use crate::dispatcher::{HttpCallbackDispatcher, HttpCallbackDispatcherConfig};
use crate::error::CallbackCoreError;
use crate::model::{
    DeliveryAttempt, DeliveryAttemptOutcome, DeliveryFailureClass, DeliveryState, DeliveryStatus,
    DispatchFailure, DispatchOutcome, TenantCallbackDestination,
};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use execution_core::CallbackJob;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PostgresQCallbackWorkerConfig {
    pub queue: String,
    pub job_type: String,
    pub worker_id: String,
    pub lease_seconds: i64,
    pub batch_size: i64,
    pub idle_sleep_ms: u64,
    pub reap_interval_ms: u64,
    pub duplicate_retry_delay_secs: i64,
}

impl Default for PostgresQCallbackWorkerConfig {
    fn default() -> Self {
        Self {
            queue: "execution.callback".to_owned(),
            job_type: "execution.callback".to_owned(),
            worker_id: "execution-callback-worker".to_owned(),
            lease_seconds: 30,
            batch_size: 32,
            idle_sleep_ms: 250,
            reap_interval_ms: 5_000,
            duplicate_retry_delay_secs: 2,
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

enum DeliveryClaim {
    Claimed { attempt_no: u32 },
    AlreadyDelivered,
    InFlight,
}

enum DeliveryDecision {
    Delivered,
    Retry {
        code: String,
        message: String,
        failure_class: DeliveryFailureClass,
        callback_id: Option<String>,
        preferred_delay_secs: Option<i64>,
    },
    Terminal {
        code: String,
        message: String,
        failure_class: DeliveryFailureClass,
        callback_id: Option<String>,
    },
    SkipDuplicateDelivered,
}

#[derive(Clone)]
pub struct PostgresQDeliveryStore {
    pool: PgPool,
}

impl PostgresQDeliveryStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ensure_schema(&self) -> Result<(), CallbackCoreError> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS callback_core_deliveries (
                callback_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                state TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_http_status INTEGER NULL,
                last_error_class TEXT NULL,
                last_error_message TEXT NULL,
                next_attempt_at_ms BIGINT NULL,
                delivered_at_ms BIGINT NULL,
                first_seen_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS callback_core_deliveries_tenant_intent_idx
            ON callback_core_deliveries(tenant_id, intent_id, updated_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS callback_core_delivery_attempts (
                attempt_id UUID PRIMARY KEY,
                callback_id TEXT NOT NULL REFERENCES callback_core_deliveries(callback_id) ON DELETE CASCADE,
                attempt_no INTEGER NOT NULL,
                outcome TEXT NOT NULL,
                failure_class TEXT NULL,
                error_message TEXT NULL,
                http_status INTEGER NULL,
                response_excerpt TEXT NULL,
                occurred_at_ms BIGINT NOT NULL,
                UNIQUE (callback_id, attempt_no)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS callback_core_delivery_attempts_callback_idx
            ON callback_core_delivery_attempts(callback_id, occurred_at_ms DESC)
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
                .map_err(sqlx_err_to_store)?;
        }

        Ok(())
    }

    pub async fn publish_callback(&self, callback: &CallbackJob) -> Result<(), CallbackCoreError> {
        let now_ms = now_ms();
        sqlx::query(
            r#"
            INSERT INTO callback_core_deliveries (
                callback_id,
                tenant_id,
                intent_id,
                job_id,
                state,
                attempts,
                first_seen_at_ms,
                updated_at_ms
            )
            VALUES ($1, $2, $3, $4, 'queued', 0, $5, $6)
            ON CONFLICT (callback_id)
            DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                intent_id = EXCLUDED.intent_id,
                job_id = EXCLUDED.job_id,
                updated_at_ms = EXCLUDED.updated_at_ms
            "#,
        )
        .bind(callback.callback_id.as_str())
        .bind(callback.summary.tenant_id.as_str())
        .bind(callback.summary.intent_id.as_str())
        .bind(callback.summary.job_id.as_str())
        .bind(now_ms as i64)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        Ok(())
    }

    pub async fn retry_callback(
        &self,
        callback_id: &str,
        failure_class: DeliveryFailureClass,
        message: &str,
        next_attempt_at_ms: u64,
    ) -> Result<(), CallbackCoreError> {
        sqlx::query(
            r#"
            UPDATE callback_core_deliveries
            SET state = 'retry_scheduled',
                attempts = attempts + 1,
                last_error_class = $2,
                last_error_message = $3,
                next_attempt_at_ms = $4,
                updated_at_ms = $5
            WHERE callback_id = $1
            "#,
        )
        .bind(callback_id)
        .bind(failure_class.as_str())
        .bind(message)
        .bind(next_attempt_at_ms as i64)
        .bind(now_ms() as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    pub async fn mark_terminal_failure(
        &self,
        callback_id: &str,
        failure_class: DeliveryFailureClass,
        message: &str,
    ) -> Result<(), CallbackCoreError> {
        sqlx::query(
            r#"
            UPDATE callback_core_deliveries
            SET state = 'terminal_failure',
                attempts = attempts + 1,
                last_error_class = $2,
                last_error_message = $3,
                next_attempt_at_ms = NULL,
                updated_at_ms = $4
            WHERE callback_id = $1
            "#,
        )
        .bind(callback_id)
        .bind(failure_class.as_str())
        .bind(message)
        .bind(now_ms() as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    pub async fn mark_delivered(
        &self,
        callback_id: &str,
        http_status: u16,
    ) -> Result<(), CallbackCoreError> {
        let now = now_ms();
        sqlx::query(
            r#"
            UPDATE callback_core_deliveries
            SET state = 'delivered',
                attempts = attempts + 1,
                last_http_status = $2,
                last_error_class = NULL,
                last_error_message = NULL,
                next_attempt_at_ms = NULL,
                delivered_at_ms = $3,
                updated_at_ms = $4
            WHERE callback_id = $1
            "#,
        )
        .bind(callback_id)
        .bind(http_status as i32)
        .bind(now as i64)
        .bind(now as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    pub async fn record_delivery_attempt(
        &self,
        attempt: &DeliveryAttempt,
    ) -> Result<(), CallbackCoreError> {
        sqlx::query(
            r#"
            INSERT INTO callback_core_delivery_attempts (
                attempt_id,
                callback_id,
                attempt_no,
                outcome,
                failure_class,
                error_message,
                http_status,
                response_excerpt,
                occurred_at_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (callback_id, attempt_no) DO NOTHING
            "#,
        )
        .bind(attempt.attempt_id)
        .bind(&attempt.callback_id)
        .bind(attempt.attempt_no as i32)
        .bind(attempt.outcome.as_str())
        .bind(attempt.failure_class.map(|v| v.as_str().to_owned()))
        .bind(&attempt.error_message)
        .bind(attempt.http_status.map(i32::from))
        .bind(&attempt.response_excerpt)
        .bind(attempt.occurred_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    pub async fn get_delivery_status(
        &self,
        callback_id: &str,
    ) -> Result<Option<DeliveryStatus>, CallbackCoreError> {
        let row = sqlx::query(
            r#"
            SELECT
                callback_id,
                tenant_id,
                intent_id,
                job_id,
                state,
                attempts,
                last_http_status,
                last_error_class,
                last_error_message,
                next_attempt_at_ms,
                delivered_at_ms,
                first_seen_at_ms,
                updated_at_ms
            FROM callback_core_deliveries
            WHERE callback_id = $1
            "#,
        )
        .bind(callback_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let state_raw: String = row.get("state");
        let state = DeliveryState::parse(&state_raw).ok_or_else(|| {
            CallbackCoreError::Store(format!("unknown delivery state `{state_raw}` in storage"))
        })?;

        let last_error_class_raw: Option<String> = row.get("last_error_class");
        let last_error_class =
            parse_optional_failure_class(last_error_class_raw.as_deref(), "delivery status")?;

        Ok(Some(DeliveryStatus {
            callback_id: row.get("callback_id"),
            tenant_id: row.get("tenant_id"),
            intent_id: row.get("intent_id"),
            job_id: row.get("job_id"),
            state,
            attempts: row.get::<i32, _>("attempts").max(0) as u32,
            last_http_status: row
                .get::<Option<i32>, _>("last_http_status")
                .map(|v| v.max(0) as u16),
            last_error_class,
            last_error_message: row.get("last_error_message"),
            next_attempt_at_ms: row
                .get::<Option<i64>, _>("next_attempt_at_ms")
                .map(|v| v.max(0) as u64),
            delivered_at_ms: row
                .get::<Option<i64>, _>("delivered_at_ms")
                .map(|v| v.max(0) as u64),
            first_seen_at_ms: row.get::<i64, _>("first_seen_at_ms").max(0) as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        }))
    }

    pub async fn list_delivery_attempts(
        &self,
        callback_id: &str,
        limit: i64,
    ) -> Result<Vec<DeliveryAttempt>, CallbackCoreError> {
        let limit = limit.clamp(1, 512);
        let rows = sqlx::query(
            r#"
            SELECT
                attempt_id,
                callback_id,
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
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let outcome_raw: String = row.get("outcome");
            let outcome = DeliveryAttemptOutcome::parse(&outcome_raw).ok_or_else(|| {
                CallbackCoreError::Store(format!(
                    "unknown delivery attempt outcome `{outcome_raw}` in storage"
                ))
            })?;

            let class_raw: Option<String> = row.get("failure_class");
            let failure_class =
                parse_optional_failure_class(class_raw.as_deref(), "delivery attempt")?;

            out.push(DeliveryAttempt {
                attempt_id: row.get("attempt_id"),
                callback_id: row.get("callback_id"),
                attempt_no: row.get::<i32, _>("attempt_no").max(0) as u32,
                outcome,
                failure_class,
                error_message: row.get("error_message"),
                http_status: row
                    .get::<Option<i32>, _>("http_status")
                    .map(|v| v.max(0) as u16),
                response_excerpt: row.get("response_excerpt"),
                occurred_at_ms: row.get::<i64, _>("occurred_at_ms").max(0) as u64,
            });
        }

        Ok(out)
    }

    pub async fn get_tenant_destination(
        &self,
        tenant_id: &str,
    ) -> Result<Option<TenantCallbackDestination>, CallbackCoreError> {
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
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(row) => row,
            Err(err) if is_undefined_table(&err) => return Ok(None),
            Err(err) => return Err(sqlx_err_to_store(err)),
        };

        Ok(row.map(parse_tenant_destination_row))
    }

    pub async fn upsert_tenant_destination(
        &self,
        destination: &TenantCallbackDestination,
    ) -> Result<(), CallbackCoreError> {
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
        .bind(&destination.tenant_id)
        .bind(&destination.delivery_url)
        .bind(&destination.bearer_token)
        .bind(&destination.signature_secret)
        .bind(&destination.signature_key_id)
        .bind(destination.timeout_ms as i64)
        .bind(destination.allow_private_destinations)
        .bind(&destination.allowed_hosts)
        .bind(destination.enabled)
        .bind(&destination.updated_by_principal_id)
        .bind(destination.created_at_ms as i64)
        .bind(destination.updated_at_ms as i64)
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        Ok(())
    }

    pub async fn delete_tenant_destination(
        &self,
        tenant_id: &str,
    ) -> Result<bool, CallbackCoreError> {
        let result = match sqlx::query(
            r#"
            DELETE FROM callback_core_tenant_destinations
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .execute(&self.pool)
        .await
        {
            Ok(result) => result,
            Err(err) if is_undefined_table(&err) => return Ok(false),
            Err(err) => return Err(sqlx_err_to_store(err)),
        };

        Ok(result.rows_affected() > 0)
    }

    async fn claim_for_delivery(
        &self,
        callback_id: &str,
    ) -> Result<DeliveryClaim, CallbackCoreError> {
        let now = now_ms();
        let row = sqlx::query(
            r#"
            UPDATE callback_core_deliveries
            SET state = 'delivering',
                updated_at_ms = $2
            WHERE callback_id = $1
              AND state NOT IN ('delivered', 'delivering')
            RETURNING attempts
            "#,
        )
        .bind(callback_id)
        .bind(now as i64)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        if let Some(row) = row {
            let attempts: i32 = row.get("attempts");
            return Ok(DeliveryClaim::Claimed {
                attempt_no: attempts.saturating_add(1).max(1) as u32,
            });
        }

        let row = sqlx::query(
            r#"
            SELECT state
            FROM callback_core_deliveries
            WHERE callback_id = $1
            "#,
        )
        .bind(callback_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;

        match row.map(|row| row.get::<String, _>("state")) {
            Some(state) if state == "delivered" => Ok(DeliveryClaim::AlreadyDelivered),
            Some(state) if state == "delivering" => Ok(DeliveryClaim::InFlight),
            Some(_) => Ok(DeliveryClaim::InFlight),
            None => Err(CallbackCoreError::Store(format!(
                "callback delivery row `{callback_id}` is missing"
            ))),
        }
    }
}

pub struct TenantRoutedCallbackDispatcher {
    delivery_store: Arc<PostgresQDeliveryStore>,
    fallback: Arc<dyn CallbackDispatcher>,
}

impl TenantRoutedCallbackDispatcher {
    pub fn new(
        delivery_store: Arc<PostgresQDeliveryStore>,
        fallback: Arc<dyn CallbackDispatcher>,
    ) -> Self {
        Self {
            delivery_store,
            fallback,
        }
    }
}

#[async_trait]
impl CallbackDispatcher for TenantRoutedCallbackDispatcher {
    async fn dispatch(&self, callback: &CallbackJob) -> Result<DispatchOutcome, DispatchFailure> {
        let tenant_id = callback.summary.tenant_id.as_str();
        let destination = self
            .delivery_store
            .get_tenant_destination(tenant_id)
            .await
            .map_err(|err| DispatchFailure {
                class: DeliveryFailureClass::Internal,
                code: "callback.destination_lookup_failed".to_owned(),
                message: format!("failed to load callback destination for tenant `{tenant_id}`: {err}"),
                retryable: true,
                http_status: None,
                retry_after_secs: None,
                response_excerpt: None,
            })?;

        let Some(destination) = destination else {
            return self.fallback.dispatch(callback).await;
        };

        if !destination.enabled {
            return Err(DispatchFailure {
                class: DeliveryFailureClass::DestinationBlocked,
                code: "callback.destination_disabled".to_owned(),
                message: format!("callback destination is disabled for tenant `{tenant_id}`"),
                retryable: false,
                http_status: None,
                retry_after_secs: None,
                response_excerpt: None,
            });
        }

        let mut cfg = HttpCallbackDispatcherConfig::new(destination.delivery_url);
        cfg.bearer_token = destination.bearer_token;
        cfg.signature_secret = destination.signature_secret;
        cfg.signature_key_id = destination.signature_key_id;
        cfg.timeout_ms = destination.timeout_ms.max(1);
        cfg.allow_private_destinations = destination.allow_private_destinations;
        cfg.allowed_hosts = parse_allowed_hosts(destination.allowed_hosts.as_deref());

        let dispatcher = HttpCallbackDispatcher::new(cfg).map_err(|err| DispatchFailure {
            class: DeliveryFailureClass::InvalidDestination,
            code: "callback.destination_invalid".to_owned(),
            message: format!("invalid callback destination config for tenant `{tenant_id}`: {err}"),
            retryable: false,
            http_status: None,
            retry_after_secs: None,
            response_excerpt: None,
        })?;

        dispatcher.dispatch(callback).await
    }
}

pub struct PostgresQCallbackWorker {
    pool: PgPool,
    delivery_store: Arc<PostgresQDeliveryStore>,
    dispatcher: Arc<dyn CallbackDispatcher>,
    cfg: PostgresQCallbackWorkerConfig,
}

impl PostgresQCallbackWorker {
    pub fn new(
        pool: PgPool,
        dispatcher: Arc<dyn CallbackDispatcher>,
        cfg: PostgresQCallbackWorkerConfig,
    ) -> Self {
        let delivery_store = Arc::new(PostgresQDeliveryStore::new(pool.clone()));
        Self {
            pool,
            delivery_store,
            dispatcher,
            cfg,
        }
    }

    pub async fn ensure_schema(&self) -> Result<(), CallbackCoreError> {
        self.delivery_store.ensure_schema().await
    }

    pub fn delivery_store(&self) -> Arc<PostgresQDeliveryStore> {
        self.delivery_store.clone()
    }

    pub async fn run_once(&self) -> Result<usize, CallbackCoreError> {
        let batch = self.lease_callback_jobs().await?;
        if batch.is_empty() {
            tokio::time::sleep(Duration::from_millis(self.cfg.idle_sleep_ms)).await;
            return Ok(0);
        }

        for queued in &batch {
            self.process_one(queued).await?;
        }
        Ok(batch.len())
    }

    pub async fn run_forever(&self) -> Result<(), CallbackCoreError> {
        let mut last_reap = Instant::now() - Duration::from_millis(self.cfg.reap_interval_ms);
        loop {
            if last_reap.elapsed().as_millis() >= self.cfg.reap_interval_ms as u128 {
                self.reap_expired_locks().await?;
                last_reap = Instant::now();
            }

            let _ = self.run_once().await?;
        }
    }

    async fn process_one(&self, queued: &LeasedQueueJob) -> Result<(), CallbackCoreError> {
        let attempt = self.start_attempt(queued).await?;
        let started = Instant::now();
        let exec_res = self.execute_callback_job(queued).await;
        let latency_ms = started.elapsed().as_millis().min(i32::MAX as u128) as i32;

        match exec_res {
            Ok(DeliveryDecision::Delivered) | Ok(DeliveryDecision::SkipDuplicateDelivered) => {
                self.finish_attempt_succeeded(attempt.id, latency_ms)
                    .await?;
                self.mark_queue_job_succeeded(queued.id).await?;
                Ok(())
            }
            Ok(DeliveryDecision::Retry {
                code,
                message,
                failure_class,
                callback_id,
                preferred_delay_secs,
            }) => {
                self.finish_attempt_failed(attempt.id, latency_ms, &code, &message)
                    .await?;
                let can_retry = attempt.attempt_no < queued.max_attempts;
                if can_retry {
                    let delay_secs = preferred_delay_secs
                        .unwrap_or_else(|| next_retry_delay_secs(attempt.attempt_no));
                    let delay_secs = delay_secs.clamp(1, 900);
                    let next_run_at = Utc::now() + chrono::Duration::seconds(delay_secs);
                    self.reschedule_queue_job(queued.id, next_run_at, &code, &message)
                        .await?;

                    if let Some(callback_id) = callback_id {
                        self.delivery_store
                            .retry_callback(
                                &callback_id,
                                failure_class,
                                &message,
                                datetime_to_ms(next_run_at),
                            )
                            .await?;
                    }
                } else {
                    self.mark_queue_job_dlq(queued.id, "CALLBACK_RETRY_EXHAUSTED", &code, &message)
                        .await?;
                    if let Some(callback_id) = callback_id {
                        self.delivery_store
                            .mark_terminal_failure(
                                &callback_id,
                                DeliveryFailureClass::Internal,
                                "callback retry budget exhausted",
                            )
                            .await?;
                    }
                }
                Ok(())
            }
            Ok(DeliveryDecision::Terminal {
                code,
                message,
                failure_class,
                callback_id,
            }) => {
                self.finish_attempt_failed(attempt.id, latency_ms, &code, &message)
                    .await?;
                self.mark_queue_job_dlq(queued.id, "CALLBACK_TERMINAL", &code, &message)
                    .await?;

                if let Some(callback_id) = callback_id {
                    self.delivery_store
                        .mark_terminal_failure(&callback_id, failure_class, &message)
                        .await?;
                }
                Ok(())
            }
            Err(err) => {
                let (code, message, retryable) = classify_infrastructure_error(&err);
                self.finish_attempt_failed(attempt.id, latency_ms, code, &message)
                    .await?;
                let can_retry = retryable && attempt.attempt_no < queued.max_attempts;
                if can_retry {
                    let delay_secs = next_retry_delay_secs(attempt.attempt_no);
                    let next_run_at = Utc::now() + chrono::Duration::seconds(delay_secs);
                    self.reschedule_queue_job(queued.id, next_run_at, code, &message)
                        .await?;
                } else {
                    self.mark_queue_job_dlq(queued.id, "CALLBACK_WORKER_ERROR", code, &message)
                        .await?;
                }
                Ok(())
            }
        }
    }

    async fn execute_callback_job(
        &self,
        queued: &LeasedQueueJob,
    ) -> Result<DeliveryDecision, CallbackCoreError> {
        let callback: CallbackJob = match serde_json::from_value(queued.payload_json.clone()) {
            Ok(value) => value,
            Err(err) => {
                return Ok(DeliveryDecision::Terminal {
                    code: "callback.invalid_payload".to_owned(),
                    message: format!("invalid callback queue payload: {err}"),
                    failure_class: DeliveryFailureClass::Serialization,
                    callback_id: None,
                });
            }
        };

        let callback_id = callback.callback_id.as_str().to_owned();
        self.delivery_store.publish_callback(&callback).await?;
        let claim = self.delivery_store.claim_for_delivery(&callback_id).await?;

        match claim {
            DeliveryClaim::AlreadyDelivered => {
                let attempt = DeliveryAttempt {
                    attempt_id: Uuid::new_v4(),
                    callback_id,
                    attempt_no: 0,
                    outcome: DeliveryAttemptOutcome::SkippedDuplicate,
                    failure_class: None,
                    error_message: Some(
                        "duplicate callback skipped because it was already delivered".to_owned(),
                    ),
                    http_status: None,
                    response_excerpt: None,
                    occurred_at_ms: now_ms(),
                };
                self.delivery_store
                    .record_delivery_attempt(&attempt)
                    .await?;
                Ok(DeliveryDecision::SkipDuplicateDelivered)
            }
            DeliveryClaim::InFlight => Ok(DeliveryDecision::Retry {
                code: "callback.delivery_in_flight".to_owned(),
                message: "callback is already being delivered by another worker".to_owned(),
                failure_class: DeliveryFailureClass::Internal,
                callback_id: None,
                preferred_delay_secs: Some(self.cfg.duplicate_retry_delay_secs),
            }),
            DeliveryClaim::Claimed { attempt_no } => {
                let dispatch_result = self.dispatcher.dispatch(&callback).await;
                match dispatch_result {
                    Ok(outcome) => {
                        let attempt = DeliveryAttempt {
                            attempt_id: Uuid::new_v4(),
                            callback_id: callback.callback_id.as_str().to_owned(),
                            attempt_no,
                            outcome: DeliveryAttemptOutcome::Succeeded,
                            failure_class: None,
                            error_message: None,
                            http_status: Some(outcome.http_status),
                            response_excerpt: outcome.response_excerpt,
                            occurred_at_ms: now_ms(),
                        };
                        self.delivery_store
                            .record_delivery_attempt(&attempt)
                            .await?;
                        self.delivery_store
                            .mark_delivered(callback.callback_id.as_str(), outcome.http_status)
                            .await?;
                        Ok(DeliveryDecision::Delivered)
                    }
                    Err(failure) => {
                        self.record_dispatch_failure(&callback, attempt_no, &failure)
                            .await?;
                        if failure.retryable {
                            Ok(DeliveryDecision::Retry {
                                code: failure.code,
                                message: failure.message,
                                failure_class: failure.class,
                                callback_id: Some(callback.callback_id.as_str().to_owned()),
                                preferred_delay_secs: failure.retry_after_secs,
                            })
                        } else {
                            Ok(DeliveryDecision::Terminal {
                                code: failure.code,
                                message: failure.message,
                                failure_class: failure.class,
                                callback_id: Some(callback.callback_id.as_str().to_owned()),
                            })
                        }
                    }
                }
            }
        }
    }

    async fn record_dispatch_failure(
        &self,
        callback: &CallbackJob,
        attempt_no: u32,
        failure: &DispatchFailure,
    ) -> Result<(), CallbackCoreError> {
        let attempt = DeliveryAttempt {
            attempt_id: Uuid::new_v4(),
            callback_id: callback.callback_id.as_str().to_owned(),
            attempt_no,
            outcome: if failure.retryable {
                DeliveryAttemptOutcome::FailedRetryable
            } else {
                DeliveryAttemptOutcome::FailedTerminal
            },
            failure_class: Some(failure.class),
            error_message: Some(failure.message.clone()),
            http_status: failure.http_status,
            response_excerpt: failure.response_excerpt.clone(),
            occurred_at_ms: now_ms(),
        };
        self.delivery_store.record_delivery_attempt(&attempt).await
    }

    async fn lease_callback_jobs(&self) -> Result<Vec<LeasedQueueJob>, CallbackCoreError> {
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
        .bind(&self.cfg.job_type)
        .bind(batch_size)
        .bind(&self.cfg.worker_id)
        .bind(self.cfg.lease_seconds)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(rows)
    }

    async fn start_attempt(
        &self,
        queued: &LeasedQueueJob,
    ) -> Result<StartedAttempt, CallbackCoreError> {
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
        .fetch_one(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(row)
    }

    async fn finish_attempt_succeeded(
        &self,
        attempt_id: Uuid,
        latency_ms: i32,
    ) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn finish_attempt_failed(
        &self,
        attempt_id: Uuid,
        latency_ms: i32,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn mark_queue_job_succeeded(&self, queue_job_id: Uuid) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn reschedule_queue_job(
        &self,
        queue_job_id: Uuid,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn mark_queue_job_dlq(
        &self,
        queue_job_id: Uuid,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
    ) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }

    async fn reap_expired_locks(&self) -> Result<(), CallbackCoreError> {
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
        .execute(&self.pool)
        .await
        .map_err(sqlx_err_to_store)?;
        Ok(())
    }
}

fn parse_optional_failure_class(
    value: Option<&str>,
    context: &str,
) -> Result<Option<DeliveryFailureClass>, CallbackCoreError> {
    match value {
        Some(v) => DeliveryFailureClass::parse(v).map(Some).ok_or_else(|| {
            CallbackCoreError::Store(format!(
                "unknown failure class `{v}` found in {context} storage row"
            ))
        }),
        None => Ok(None),
    }
}

fn parse_allowed_hosts(raw: Option<&str>) -> Option<HashSet<String>> {
    let Some(raw) = raw else {
        return None;
    };

    let hosts: HashSet<String> = raw
        .split(|ch| ch == ',' || ch == ';' || ch == '|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect();

    if hosts.is_empty() {
        None
    } else {
        Some(hosts)
    }
}

fn parse_tenant_destination_row(row: sqlx::postgres::PgRow) -> TenantCallbackDestination {
    TenantCallbackDestination {
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

fn is_undefined_table(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => db_err.code().as_deref() == Some("42P01"),
        _ => false,
    }
}

fn classify_infrastructure_error(err: &CallbackCoreError) -> (&'static str, String, bool) {
    match err {
        CallbackCoreError::Store(message) => ("CALLBACK_STORE_ERROR", message.clone(), true),
        CallbackCoreError::InvalidPayload(message) => {
            ("CALLBACK_INVALID_PAYLOAD", message.clone(), false)
        }
        CallbackCoreError::Transport(message) => ("CALLBACK_TRANSPORT", message.clone(), true),
        CallbackCoreError::Security(message) => ("CALLBACK_SECURITY", message.clone(), false),
        CallbackCoreError::Configuration(message) => {
            ("CALLBACK_CONFIGURATION", message.clone(), false)
        }
    }
}

fn next_retry_delay_secs(attempt_no: i32) -> i64 {
    let exp = attempt_no.saturating_sub(1).clamp(0, 8) as u32;
    let base = 1_i64.checked_shl(exp).unwrap_or(300);
    base.clamp(1, 300)
}

fn datetime_to_ms(value: DateTime<Utc>) -> u64 {
    value.timestamp_millis().max(0) as u64
}

fn now_ms() -> u64 {
    Utc::now().timestamp_millis().max(0) as u64
}

fn _u64_to_datetime(value: u64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(value as i64)
        .single()
        .unwrap_or_else(Utc::now)
}

fn sqlx_err_to_store(err: sqlx::Error) -> CallbackCoreError {
    CallbackCoreError::Store(err.to_string())
}
