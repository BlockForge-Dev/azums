// crates/azums/src/jobs/repo.rs

use crate::jobs::model::{Job, JobListItem, JobStatus, NewJob};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Repository providing atomic database operations for job queue management.
///
/// Handles enqueueing, transactional batch leasing (`SKIP LOCKED`), execution completion,
/// re-scheduling retries, moving jobs to DLQ, and replay.
#[derive(Clone)]
pub struct JobsRepo {
    pool: PgPool,
    database_url: Option<String>,
}

impl JobsRepo {
    /// Creates a new `JobsRepo` wrapping a SQLx PostgreSQL connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            database_url: None,
        }
    }

    /// Creates a new `JobsRepo` with a dedicated `database_url` for unpooled `LISTEN` connections.
    pub fn new_with_url(pool: PgPool, database_url: impl Into<String>) -> Self {
        Self {
            pool,
            database_url: Some(database_url.into()),
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
            "default".to_string()
        } else {
            trimmed.chars().take(32).collect()
        }
    }

    pub(crate) fn dataset_id_for(queue: &str, at: DateTime<Utc>) -> String {
        format!(
            "{}_{}",
            Self::sanitize_dataset_queue(queue),
            at.format("%Y%m%d_%H")
        )
    }

    pub(crate) async fn ensure_dataset_partition(&self, dataset_id: &str) -> anyhow::Result<()> {
        match sqlx::query("SELECT public.ensure_jobs_dataset_partition($1)")
            .bind(dataset_id)
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(()),
            // During startup races migrations may still be applying; DEFAULT partition still accepts inserts.
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42883") => {
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }

    // ----------------------------
    // Enqueue helpers
    // ----------------------------

    fn notify_channel_name(queue: &str) -> String {
        let sanitized = Self::sanitize_dataset_queue(queue);
        format!("azums_job_enqueued_{sanitized}")
    }

    /// Inserts a new job into the queue database.
    ///
    /// Automatically routes the job to the appropriate dataset partition based on `run_at`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::{Job, JobsRepo, make_pool};
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let pool = make_pool("postgres://localhost/flow").await?;
    /// let repo = JobsRepo::new(pool);
    /// let job_id = repo
    ///     .enqueue(Job::new("email_send", serde_json::json!({"to": "user@example.com"})).into())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        let dataset_id = Self::dataset_id_for(&job.queue, job.run_at);
        self.ensure_dataset_partition(&dataset_id).await?;

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO jobs (
                dataset_id, idempotency_key,
                queue, job_type, payload_json, run_at,
                deadline_at, timeout_seconds, recurring_interval_seconds,
                status, priority, max_attempts
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7::timestamptz, $8::integer, $9::integer,
                $10, $11, $12
            )
            ON CONFLICT (dataset_id, idempotency_key) WHERE idempotency_key IS NOT NULL
            DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key
            RETURNING id
            "#,
        )
        .bind(dataset_id)
        .bind(&job.idempotency_key)
        .bind(&job.queue)
        .bind(job.job_type)
        .bind(job.payload_json)
        .bind(job.run_at)
        .bind(job.deadline_at)
        .bind(job.timeout_seconds)
        .bind(job.recurring_interval_seconds)
        .bind(JobStatus::Queued.as_str())
        .bind(job.priority)
        .bind(job.max_attempts)
        .fetch_one(&self.pool)
        .await?;

        let channel = Self::notify_channel_name(&job.queue);
        let _ = sqlx::query("SELECT pg_notify($1, '')")
            .bind(&channel)
            .execute(&self.pool)
            .await;

        Ok(id)
    }

    /// Inserts a new job using the caller's PostgreSQL transaction.
    ///
    /// Use this when an application state mutation and job enqueue must commit or roll back
    /// together. The `pg_notify` call is executed inside the same transaction, so PostgreSQL only
    /// delivers the wake-up notification if the transaction commits.
    pub async fn enqueue_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: NewJob,
    ) -> anyhow::Result<Uuid> {
        let dataset_id = Self::dataset_id_for(&job.queue, job.run_at);
        self.ensure_dataset_partition(&dataset_id).await?;
        let channel = Self::notify_channel_name(&job.queue);

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO jobs (
                dataset_id, idempotency_key,
                queue, job_type, payload_json, run_at,
                deadline_at, timeout_seconds, recurring_interval_seconds,
                status, priority, max_attempts
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7::timestamptz, $8::integer, $9::integer,
                $10, $11, $12
            )
            ON CONFLICT (dataset_id, idempotency_key) WHERE idempotency_key IS NOT NULL
            DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key
            RETURNING id
            "#,
        )
        .bind(dataset_id)
        .bind(&job.idempotency_key)
        .bind(&job.queue)
        .bind(job.job_type)
        .bind(job.payload_json)
        .bind(job.run_at)
        .bind(job.deadline_at)
        .bind(job.timeout_seconds)
        .bind(job.recurring_interval_seconds)
        .bind(JobStatus::Queued.as_str())
        .bind(job.priority)
        .bind(job.max_attempts)
        .fetch_one(&mut **tx)
        .await?;

        let _ = sqlx::query("SELECT pg_notify($1, '')")
            .bind(&channel)
            .execute(&mut **tx)
            .await?;

        Ok(id)
    }

    /// Enqueues a job for immediate execution (`run_at = Utc::now()`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::{JobsRepo, make_pool};
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let pool = make_pool("postgres://localhost/flow").await?;
    /// let repo = JobsRepo::new(pool);
    /// let id = repo.enqueue_now("default", "send_welcome", serde_json::json!({"id": 123})).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enqueue_now(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            idempotency_key: None,
            run_at: Utc::now(),
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    /// Schedules a job for future execution delayed by `delay_secs` seconds.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::{JobsRepo, make_pool};
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let pool = make_pool("postgres://localhost/flow").await?;
    /// let repo = JobsRepo::new(pool);
    /// let id = repo.enqueue_in("default", "send_reminder", serde_json::json!({}), 300).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enqueue_in(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
        delay_secs: i64,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            idempotency_key: None,
            run_at: Utc::now() + chrono::Duration::seconds(delay_secs),
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    /// Schedules a job to run at a specific UTC timestamp (`run_at`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::{JobsRepo, make_pool};
    /// use chrono::Utc;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let pool = make_pool("postgres://localhost/flow").await?;
    /// let repo = JobsRepo::new(pool);
    /// let run_at = Utc::now() + chrono::Duration::hours(1);
    /// let id = repo.enqueue_at("default", "scheduled_report", serde_json::json!({}), run_at).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enqueue_at(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: serde_json::Value,
        run_at: DateTime<Utc>,
    ) -> anyhow::Result<Uuid> {
        self.enqueue(NewJob {
            queue: queue.to_string(),
            job_type: job_type.to_string(),
            payload_json,
            idempotency_key: None,
            run_at,
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 25,
        })
        .await
    }

    // ----------------------------
    // Reads
    // ----------------------------

    /// Fetches a single [`Job`] record by primary key `job_id`.
    pub async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>("SELECT * FROM jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(job)
    }

    /// Extends the lock expiration timestamp for a running job.
    pub async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET lock_expires_at = now() + ($3::int * interval '1 second'),
                updated_at = now()
            WHERE id = $1 AND locked_by = $2 AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(lease_seconds)
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected() > 0)
    }

    // ----------------------------
    // List / DLQ views (Admin API support)
    // ----------------------------

    /// Cursor-paginated list of jobs.
    /// Cursor is (created_at, id) ordered DESC.
    ///
    /// - queue/status are optional filters
    /// - limit is clamped to [1, 500]
    pub async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let limit = limit.clamp(1, 500);

        let rows = match (queue, status, cursor_created_at, cursor_id) {
            (Some(q), Some(st), Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1 AND status = $2
                      AND (created_at, id) < ($3, $4)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $5
                    "#,
                )
                .bind(q)
                .bind(st)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), Some(st), _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1 AND status = $2
                    ORDER BY created_at DESC, id DESC
                    LIMIT $3
                    "#,
                )
                .bind(q)
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), None, Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(q)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), None, _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE queue = $1
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(q)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(st), Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE status = $1
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(st)
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(st), _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE status = $1
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None, Some(ca), Some(cid)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    WHERE (created_at, id) < ($1, $2)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $3
                    "#,
                )
                .bind(ca)
                .bind(cid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None, _, _) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        last_error_code, last_error_message,
                        dlq_reason_code,
                        created_at, updated_at
                    FROM jobs
                    ORDER BY created_at DESC, id DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows)
    }

    // ----------------------------
    // Metrics snapshot (for /metrics)
    // ----------------------------

    /// Returns: (queued, running, succeeded_last_60s, failed_or_dlq_last_60s)
    pub async fn metrics_snapshot(&self) -> anyhow::Result<(i64, i64, i64, i64)> {
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'queued'")
            .fetch_one(&self.pool)
            .await?;

        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'running'")
            .fetch_one(&self.pool)
            .await?;

        let succeeded_last_60s: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM jobs
            WHERE status = 'succeeded'
              AND updated_at >= now() - interval '60 seconds'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let failed_last_60s: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM jobs
            WHERE status IN ('failed', 'dlq')
              AND updated_at >= now() - interval '60 seconds'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok((queued, running, succeeded_last_60s, failed_last_60s))
    }

    // ----------------------------
    // Leasing + Storm Control + Policy Decisions Log (Milestone 11)
    // ----------------------------

    /// Lease up to `batch_size` runnable jobs for this worker.
    ///
    /// Correctness: SELECT ... FOR UPDATE SKIP LOCKED
    ///
    /// Storm-control gates (per queue):
    /// - max_in_flight (jobs.status='running')
    /// - max_attempts_per_minute (attempts started in last 60s)
    ///
    /// If exceeded:
    /// - write a row into policy_decisions
    /// - reschedule one candidate slightly (throttle_delay_ms)
    /// - return an empty batch
    pub async fn lease_jobs_batch(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>> {
        self.lease_jobs_batch_with_ordering(
            queue,
            worker_id,
            lease_seconds,
            batch_size,
            azums_core::QueueOrdering::Fifo,
        )
        .await
    }

    /// Lease up to `batch_size` runnable jobs for this worker using specified [`azums_core::QueueOrdering`].
    pub async fn lease_jobs_batch_with_ordering(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
        ordering: azums_core::QueueOrdering,
    ) -> anyhow::Result<Vec<Job>> {
        let batch_size = batch_size.clamp(1, 4096);
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'dlq',
                dlq_reason_code = 'DEADLINE_EXCEEDED',
                dlq_at = now(),
                updated_at = now()
            WHERE queue = $1
              AND status = 'queued'
              AND run_at <= now()
              AND deadline_at IS NOT NULL
              AND deadline_at < now()
            "#,
        )
        .bind(queue)
        .execute(&mut *tx)
        .await?;

        // 0) Load queue policy (defaults: basically unlimited)
        let policy = sqlx::query_as::<_, (i32, i32, i32)>(
            r#"
            SELECT max_attempts_per_minute, max_in_flight, throttle_delay_ms
            FROM queue_policies
            WHERE queue = $1
            "#,
        )
        .bind(queue)
        .fetch_optional(&mut *tx)
        .await?;

        let mut max_attempts_per_minute = i32::MAX / 4;
        let mut max_in_flight = i32::MAX / 4;
        let mut throttle_delay_ms = 250;
        let mut in_flight = 0_i64;
        let mut attempts_last_min = 0_i64;

        let dataset_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT dataset_id
            FROM jobs
            WHERE queue = $1
              AND status = 'queued'
              AND run_at <= now()
            ORDER BY run_at ASC, created_at ASC
            LIMIT 1
            "#,
        )
        .bind(queue)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(dataset_id) = dataset_id else {
            tx.commit().await?;
            return Ok(Vec::new());
        };

        let throttle_reason =
            if let Some((p_max_attempts, p_max_in_flight, p_throttle_delay_ms)) = policy {
                max_attempts_per_minute = p_max_attempts;
                max_in_flight = p_max_in_flight;
                throttle_delay_ms = p_throttle_delay_ms;

                in_flight = sqlx::query_scalar(
                    r#"
                SELECT COUNT(*)
                FROM jobs
                WHERE queue = $1 AND status = 'running'
                "#,
                )
                .bind(queue)
                .fetch_one(&mut *tx)
                .await?;

                attempts_last_min = sqlx::query_scalar(
                    r#"
                SELECT COUNT(*)
                FROM job_attempts a
                JOIN jobs j ON j.id = a.job_id AND j.dataset_id = a.dataset_id
                WHERE j.queue = $1
                  AND a.started_at >= now() - interval '60 seconds'
                "#,
                )
                .bind(queue)
                .fetch_one(&mut *tx)
                .await?;

                if in_flight >= max_in_flight as i64 {
                    Some("IN_FLIGHT_EXCEEDED")
                } else if attempts_last_min >= max_attempts_per_minute as i64 {
                    Some("RETRY_RATE_EXCEEDED")
                } else {
                    None
                }
            } else {
                None
            };

        if let Some(reason_code) = throttle_reason {
            let candidate_id_query = match ordering {
                azums_core::QueueOrdering::Fifo => {
                    r#"
                    SELECT id
                    FROM jobs
                    WHERE dataset_id = $1
                      AND queue = $2
                      AND status = 'queued'
                      AND run_at <= now()
                    ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#
                }
                azums_core::QueueOrdering::Fastest => {
                    r#"
                    SELECT id
                    FROM jobs
                    WHERE dataset_id = $1
                      AND queue = $2
                      AND status = 'queued'
                      AND run_at <= now()
                    ORDER BY priority DESC, run_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                    "#
                }
            };

            let candidate_id = sqlx::query_scalar::<_, Uuid>(candidate_id_query)
                .bind(&dataset_id)
                .bind(queue)
                .fetch_optional(&mut *tx)
                .await?;

            if let Some(job_id) = candidate_id {
                let details = match reason_code {
                    "IN_FLIGHT_EXCEEDED" => json!({
                        "dataset_id": dataset_id,
                        "queue": queue,
                        "in_flight": in_flight,
                        "max_in_flight": max_in_flight,
                        "throttle_delay_ms": throttle_delay_ms
                    }),
                    _ => json!({
                        "dataset_id": dataset_id,
                        "queue": queue,
                        "attempts_last_minute": attempts_last_min,
                        "max_attempts_per_minute": max_attempts_per_minute,
                        "throttle_delay_ms": throttle_delay_ms
                    }),
                };

                sqlx::query(
                    r#"
                    INSERT INTO policy_decisions (
                      id, dataset_id, job_id, decision, reason_code, details_json
                    )
                    VALUES ($1, $2, $3, 'THROTTLED', $4, $5)
                    "#,
                )
                .bind(Uuid::new_v4())
                .bind(&dataset_id)
                .bind(job_id)
                .bind(reason_code)
                .bind(details)
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    r#"
                    UPDATE jobs
                    SET run_at = now() + ($2::int * interval '1 millisecond'),
                        updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(job_id)
                .bind(throttle_delay_ms)
                .execute(&mut *tx)
                .await?;
            }

            tx.commit().await?;
            return Ok(Vec::new());
        }

        // 3) Lease a batch in one round-trip according to QueueOrdering.
        let leased = match ordering {
            azums_core::QueueOrdering::Fifo => {
                sqlx::query_as::<_, Job>(
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM jobs
                        WHERE dataset_id = $1
                          AND queue = $2
                          AND status = 'queued'
                          AND run_at <= now()
                        ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC
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
                        RETURNING j.*
                    )
                    SELECT *
                    FROM leased
                    ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC
                    "#,
                )
                .bind(&dataset_id)
                .bind(queue)
                .bind(batch_size)
                .bind(worker_id)
                .bind(lease_seconds)
                .fetch_all(&mut *tx)
                .await?
            }
            azums_core::QueueOrdering::Fastest => {
                sqlx::query_as::<_, Job>(
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM jobs
                        WHERE dataset_id = $1
                          AND queue = $2
                          AND status = 'queued'
                          AND run_at <= now()
                        ORDER BY priority DESC, run_at ASC
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
                        RETURNING j.*
                    )
                    SELECT *
                    FROM leased
                    ORDER BY priority DESC, run_at ASC
                    "#,
                )
                .bind(&dataset_id)
                .bind(queue)
                .bind(batch_size)
                .bind(worker_id)
                .bind(lease_seconds)
                .fetch_all(&mut *tx)
                .await?
            }
        };

        tx.commit().await?;
        Ok(leased)
    }

    /// Compatibility helper for call sites/tests that still lease one-by-one.
    pub async fn lease_one_job(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<Option<Job>> {
        let mut jobs = self
            .lease_jobs_batch(queue, worker_id, lease_seconds, 1)
            .await?;
        Ok(jobs.pop())
    }

    // ----------------------------
    // Maintenance
    // ----------------------------

    pub async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;

        let expired: Vec<(String, Uuid)> = sqlx::query_as(
            r#"
            SELECT dataset_id, id
            FROM jobs
            WHERE status = 'running'
              AND lock_expires_at IS NOT NULL
              AND lock_expires_at < now()
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .fetch_all(&mut *tx)
        .await?;

        if expired.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }

        let dataset_ids: Vec<String> = expired
            .iter()
            .map(|(dataset_id, _)| dataset_id.clone())
            .collect();
        let job_ids: Vec<Uuid> = expired.iter().map(|(_, job_id)| *job_id).collect();

        sqlx::query(
            r#"
            UPDATE job_attempts a
            SET status = 'failed',
                finished_at = now(),
                latency_ms = COALESCE(
                  latency_ms,
                  GREATEST(0, (EXTRACT(EPOCH FROM (now() - started_at)) * 1000)::int)
                ),
                error_code = 'LEASE_EXPIRED',
                error_message = 'worker lease expired before ACK'
            WHERE a.dataset_id = ANY($1)
              AND a.job_id = ANY($2)
              AND a.status = 'running'
            "#,
        )
        .bind(&dataset_ids)
        .bind(&job_ids)
        .execute(&mut *tx)
        .await?;

        let res = sqlx::query(
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
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(res.rows_affected())
    }

    // ----------------------------
    // State transitions
    // ----------------------------

    /// Fast-path for successful batch execution: transitions many jobs in one statement.
    pub async fn mark_succeeded_batch(
        &self,
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<u64> {
        if job_ids.is_empty() {
            return Ok(0);
        }

        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE id = ANY($1)
              AND locked_by = $2
              AND status = 'running'
            "#,
        )
        .bind(job_ids)
        .bind(worker_id)
        .execute(&self.pool)
        .await?;

        let changed = res.rows_affected();
        if changed != job_ids.len() as u64 {
            anyhow::bail!(
                "illegal job state transition to completed: expected {} running jobs leased by {worker_id}, updated {changed}",
                job_ids.len()
            );
        }

        Ok(changed)
    }

    /// Dataset-aware fast-path for partition-pruned successful batch updates.
    pub async fn mark_succeeded_batch_for_dataset(
        &self,
        dataset_id: &str,
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<u64> {
        if job_ids.is_empty() {
            return Ok(0);
        }

        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE dataset_id = $1
              AND id = ANY($2)
              AND locked_by = $3
              AND status = 'running'
            "#,
        )
        .bind(dataset_id)
        .bind(job_ids)
        .bind(worker_id)
        .execute(&self.pool)
        .await?;

        let changed = res.rows_affected();
        if changed != job_ids.len() as u64 {
            anyhow::bail!(
                "illegal job state transition to completed: expected {} running jobs in dataset {dataset_id} leased by {worker_id}, updated {changed}",
                job_ids.len()
            );
        }

        Ok(changed)
    }

    pub async fn mark_succeeded(&self, job_id: Uuid, worker_id: &str) -> anyhow::Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND locked_by = $2
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to completed for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        Ok(())
    }

    pub async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        next_run_at: DateTime<Utc>,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let res = sqlx::query(
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
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(next_run_at)
        .bind(last_error_code)
        .bind(last_error_message)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to retry_wait for job {job_id}: expected running job"
            );
        }

        Ok(())
    }

    pub async fn mark_failed(
        &self,
        job_id: Uuid,
        worker_id: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'failed',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now(),
                last_error_code = $3,
                last_error_message = $4
            WHERE id = $1
              AND locked_by = $2
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(last_error_code)
        .bind(last_error_message)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to failed for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        Ok(())
    }

    pub async fn mark_dlq(
        &self,
        job_id: Uuid,
        worker_id: &str,
        reason_code: &str,
        last_error_code: Option<&str>,
        last_error_message: Option<&str>,
    ) -> anyhow::Result<()> {
        let res = sqlx::query(
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
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(worker_id)
        .bind(reason_code)
        .bind(last_error_code)
        .bind(last_error_message)
        .execute(&self.pool)
        .await?;
        if res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to dlq for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        Ok(())
    }

    pub async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        let current: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT status, locked_by FROM jobs WHERE id = $1")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await?;

        let Some((status, locked_by)) = current else {
            anyhow::bail!("job {job_id} not found");
        };

        match status.as_str() {
            "queued" => {}
            "running" => {
                let Some(worker_id) = worker_id else {
                    anyhow::bail!(
                        "cannot cancel running job {job_id}: worker identity is required"
                    );
                };
                if locked_by.as_deref() != Some(worker_id) {
                    anyhow::bail!(
                        "illegal job state transition to cancelled for job {job_id}: expected running lease held by {worker_id}"
                    );
                }
            }
            "succeeded" | "dlq" | "canceled" => {
                anyhow::bail!("cannot cancel terminal job {job_id}: status={status}");
            }
            other => anyhow::bail!("cannot cancel job {job_id}: invalid status={other}"),
        }

        if status == "running" {
            sqlx::query(
                r#"
                UPDATE job_attempts
                SET status = 'failed',
                    finished_at = now(),
                    latency_ms = COALESCE(latency_ms, 0),
                    error_code = 'CANCELLED',
                    error_message = 'job cancelled'
                WHERE id = (
                    SELECT id
                    FROM job_attempts
                    WHERE job_id = $1
                      AND status = 'running'
                    ORDER BY attempt_no DESC
                    LIMIT 1
                )
                "#,
            )
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'canceled',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    // ----------------------------
    // Replay
    // ----------------------------

    pub async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        let src = match self.get_job(job_id).await? {
            Some(j) => j,
            None => return Err(anyhow::anyhow!("Job with id {} not found", job_id)),
        };

        let new_queue = override_queue.unwrap_or(src.queue.as_str()).to_string();
        let new_run_at = override_run_at.unwrap_or_else(Utc::now);
        let new_dataset_id = Self::dataset_id_for(&new_queue, new_run_at);

        self.ensure_dataset_partition(&new_dataset_id).await?;

        let mut tx = self.pool.begin().await?;

        let new_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO jobs (
                dataset_id,
                queue, job_type, payload_json, run_at,
                deadline_at, timeout_seconds, recurring_interval_seconds,
                status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at,
                dlq_reason_code, dlq_at,
                replay_of_job_id
            )
            VALUES (
                $1,
                $2, $3, $4, $5,
                $6::timestamptz, $7::integer, $8::integer,
                'queued', $9, $10,
                NULL, NULL, NULL,
                NULL, NULL,
                $11
            )
            RETURNING id
            "#,
        )
        .bind(new_dataset_id)
        .bind(&new_queue)
        .bind(src.job_type)
        .bind(src.payload)
        .bind(new_run_at)
        .bind(src.deadline_at)
        .bind(src.timeout_seconds)
        .bind(src.recurring_interval_seconds)
        .bind(src.priority)
        .bind(src.max_attempts)
        .bind(src.id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let channel = Self::notify_channel_name(&new_queue);
        let _ = sqlx::query("SELECT pg_notify($1, '')")
            .bind(&channel)
            .execute(&self.pool)
            .await;

        Ok(new_id)
    }

    /// Subscribes to PostgreSQL `LISTEN` events for job enqueueing on a channel named `azums_job_enqueued_<queue>`.
    pub async fn subscribe(&self, queue: &str) -> anyhow::Result<azums_core::NotificationStream> {
        use sqlx::postgres::PgListener;
        use tokio_stream::StreamExt;

        let channel = Self::notify_channel_name(queue);
        let mut listener = if let Some(url) = &self.database_url {
            PgListener::connect(url).await?
        } else {
            PgListener::connect_with(&self.pool).await?
        };
        listener.listen(&channel).await?;

        let stream = listener
            .into_stream()
            .filter_map(|res| res.ok().map(|_| ()));
        Ok(Box::pin(stream))
    }
}
