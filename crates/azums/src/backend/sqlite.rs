use async_trait::async_trait;
use azums_core::{
    backend::{NotificationStream, StorageBackend, StreamBackend},
    model::{ConsumerGroupStatus, Event, Job, JobListItem, NewEvent, NewJob},
};
use chrono::{DateTime, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Sqlite, SqlitePool, Transaction,
};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

/// Constructs a SQLite connection pool tuned for single-writer concurrency (WAL mode, 5s busy timeout).
pub async fn make_sqlite_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// SQLite implementation of [`StorageBackend`] optimized for embedded, zero-network environments.
#[derive(Clone)]
pub struct SqliteBackend {
    pool: SqlitePool,
    notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
    stream_notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
    dequeue_count: Arc<std::sync::atomic::AtomicU64>,
    incremental_vacuum_n: u64,
}

impl SqliteBackend {
    /// Creates a new `SqliteBackend` wrapping a SQLx `SqlitePool`.
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_vacuum_n(pool, 100)
    }

    /// Creates a new `SqliteBackend` with a custom incremental vacuum threshold N.
    pub fn with_vacuum_n(pool: SqlitePool, incremental_vacuum_n: u64) -> Self {
        Self {
            pool,
            notifiers: Arc::new(RwLock::new(HashMap::new())),
            stream_notifiers: Arc::new(RwLock::new(HashMap::new())),
            dequeue_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            incremental_vacuum_n: incremental_vacuum_n.max(1),
        }
    }

    /// Returns reference to the underlying `SqlitePool`.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn notify_queue(&self, queue: &str) {
        let notifiers = self.notifiers.read().unwrap();
        if let Some(tx) = notifiers.get(queue) {
            let _ = tx.send(());
        }
    }

    fn notify_stream(&self, stream: &str) {
        let notifiers = self.stream_notifiers.read().unwrap();
        if let Some(tx) = notifiers.get(stream) {
            let _ = tx.send(());
        }
    }

    /// Inserts a new job using the caller's SQLite transaction.
    ///
    /// Use this when application data and queued work live in the same SQLite database and must
    /// commit or roll back together. This method does not emit an immediate wake-up notification;
    /// SQLite workers still use interval fallback and storage state as the source of truth.
    pub async fn enqueue_in_tx(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        job: NewJob,
    ) -> anyhow::Result<Uuid> {
        let job_id = Uuid::new_v4();
        let now = Utc::now();

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO jobs (
                id, dataset_id, replay_of_job_id, idempotency_key, queue, job_type, payload_json,
                run_at, deadline_at, timeout_seconds, recurring_interval_seconds,
                status, priority, max_attempts,
                created_at, updated_at
            ) VALUES (?, 'default', NULL, ?, ?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?)
            ON CONFLICT(idempotency_key) WHERE idempotency_key IS NOT NULL
            DO UPDATE SET idempotency_key = excluded.idempotency_key
            RETURNING id
            "#,
        )
        .bind(job_id)
        .bind(&job.idempotency_key)
        .bind(&job.queue)
        .bind(&job.job_type)
        .bind(&job.payload_json)
        .bind(job.run_at)
        .bind(job.deadline_at)
        .bind(job.timeout_seconds)
        .bind(job.recurring_interval_seconds)
        .bind(job.priority)
        .bind(job.max_attempts)
        .bind(now)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;

        Ok(id)
    }
}

#[async_trait]
impl StorageBackend for SqliteBackend {
    fn capabilities(&self) -> azums_core::BackendCapabilities {
        azums_core::BackendCapabilities::sqlite()
    }

    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        Some(self)
    }

    async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::query("PRAGMA auto_vacuum = INCREMENTAL;")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id BLOB PRIMARY KEY,
                dataset_id TEXT NOT NULL DEFAULT 'default',
                replay_of_job_id BLOB,
                idempotency_key TEXT,
                queue TEXT NOT NULL DEFAULT 'default',
                job_type TEXT NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}',
                run_at TEXT NOT NULL,
                deadline_at TEXT,
                timeout_seconds INTEGER,
                recurring_interval_seconds INTEGER,
                status TEXT NOT NULL DEFAULT 'queued',
                priority INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 25,
                locked_at TEXT,
                locked_by TEXT,
                lock_expires_at TEXT,
                dlq_reason_code TEXT,
                dlq_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_runnable
                ON jobs (queue, status, run_at, priority DESC, created_at);

            CREATE INDEX IF NOT EXISTS idx_jobs_fifo
                ON jobs (queue, status, run_at, priority DESC, created_at ASC, id ASC);

            CREATE INDEX IF NOT EXISTS idx_jobs_deadline
                ON jobs (queue, status, deadline_at)
                WHERE deadline_at IS NOT NULL;

            CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key
                ON jobs (idempotency_key)
                WHERE idempotency_key IS NOT NULL;

            CREATE TABLE IF NOT EXISTS job_attempts (
                id BLOB PRIMARY KEY,
                dataset_id TEXT NOT NULL DEFAULT 'default',
                job_id BLOB NOT NULL,
                attempt_no INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                worker_id TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                latency_ms INTEGER,
                error_code TEXT,
                error_message TEXT,
                FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS jobs_archive (
                id BLOB PRIMARY KEY,
                dataset_id TEXT NOT NULL DEFAULT 'default',
                replay_of_job_id BLOB,
                queue TEXT NOT NULL DEFAULT 'default',
                job_type TEXT NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}',
                run_at TEXT NOT NULL,
                deadline_at TEXT,
                timeout_seconds INTEGER,
                recurring_interval_seconds INTEGER,
                status TEXT NOT NULL,
                priority INTEGER NOT NULL,
                max_attempts INTEGER NOT NULL,
                dlq_reason_code TEXT,
                dlq_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS stream_events (
                sequence_no INTEGER PRIMARY KEY AUTOINCREMENT,
                stream_name TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_stream_events_lookup
                ON stream_events (stream_name, sequence_no ASC);

            CREATE TABLE IF NOT EXISTS stream_offsets (
                consumer_group TEXT NOT NULL,
                stream_name TEXT NOT NULL,
                last_acked_seq INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (consumer_group, stream_name)
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        let _ = sqlx::query("ALTER TABLE jobs ADD COLUMN idempotency_key TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE jobs ADD COLUMN deadline_at TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE jobs ADD COLUMN timeout_seconds INTEGER")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE jobs ADD COLUMN recurring_interval_seconds INTEGER")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE jobs_archive ADD COLUMN deadline_at TEXT")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE jobs_archive ADD COLUMN timeout_seconds INTEGER")
            .execute(&self.pool)
            .await;
        let _ =
            sqlx::query("ALTER TABLE jobs_archive ADD COLUMN recurring_interval_seconds INTEGER")
                .execute(&self.pool)
                .await;
        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_idempotency_key
                ON jobs (idempotency_key)
                WHERE idempotency_key IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_jobs_deadline
                ON jobs (queue, status, deadline_at)
                WHERE deadline_at IS NOT NULL
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        let job_id = Uuid::new_v4();
        let now = Utc::now();
        let queue_name = job.queue.clone();

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO jobs (
                id, dataset_id, replay_of_job_id, idempotency_key, queue, job_type, payload_json,
                run_at, status, priority, max_attempts,
                created_at, updated_at
            ) VALUES (?, 'default', NULL, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?)
            ON CONFLICT(idempotency_key) WHERE idempotency_key IS NOT NULL
            DO UPDATE SET idempotency_key = excluded.idempotency_key
            RETURNING id
            "#,
        )
        .bind(job_id)
        .bind(&job.idempotency_key)
        .bind(&job.queue)
        .bind(&job.job_type)
        .bind(&job.payload_json)
        .bind(job.run_at)
        .bind(job.priority)
        .bind(job.max_attempts)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;

        self.notify_queue(&queue_name);
        Ok(id)
    }

    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream> {
        use tokio_stream::wrappers::BroadcastStream;
        use tokio_stream::StreamExt;

        let rx = {
            let mut notifiers = self.notifiers.write().unwrap();
            let tx = notifiers
                .entry(queue.to_string())
                .or_insert_with(|| tokio::sync::broadcast::channel(128).0);
            tx.subscribe()
        };

        let bcast_stream = BroadcastStream::new(rx).filter_map(|res| res.ok());
        let interval_stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
            std::time::Duration::from_millis(100),
        ))
        .map(|_| ());

        let merged = bcast_stream.merge(interval_stream);
        Ok(Box::pin(merged))
    }

    async fn lease_jobs_batch(
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

    async fn lease_jobs_batch_with_ordering(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
        ordering: azums_core::QueueOrdering,
    ) -> anyhow::Result<Vec<Job>> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'dlq',
                dlq_reason_code = 'DEADLINE_EXCEEDED',
                dlq_at = ?,
                updated_at = ?
            WHERE queue = ?
              AND status = 'queued'
              AND run_at <= ?
              AND deadline_at IS NOT NULL
              AND deadline_at < ?
            "#,
        )
        .bind(now)
        .bind(now)
        .bind(queue)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let order_sql = match ordering {
            azums_core::QueueOrdering::Fifo => {
                "ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC"
            }
            azums_core::QueueOrdering::Fastest => "ORDER BY priority DESC, run_at ASC",
        };

        let sql = format!(
            r#"
            SELECT
                dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
                created_at, updated_at
            FROM jobs
            WHERE queue = ? AND status = 'queued' AND run_at <= ?
            {order_sql}
            LIMIT ?
            "#
        );

        let candidates = sqlx::query_as::<_, Job>(&sql)
            .bind(queue)
            .bind(now)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await?;

        if candidates.is_empty() {
            tx.commit().await?;
            return Ok(Vec::new());
        }

        let lock_expires_at = now + chrono::Duration::seconds(lease_seconds);

        let mut leased_jobs = Vec::with_capacity(candidates.len());
        for mut job in candidates {
            sqlx::query(
                r#"
                UPDATE jobs
                SET status = 'running', locked_at = ?, locked_by = ?, lock_expires_at = ?, updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(now)
            .bind(worker_id)
            .bind(lock_expires_at)
            .bind(now)
            .bind(job.id)
            .execute(&mut *tx)
            .await?;

            job.status = "running".to_string();
            job.locked_at = Some(now);
            job.locked_by = Some(worker_id.to_string());
            job.lock_expires_at = Some(lock_expires_at);
            job.updated_at = now;

            leased_jobs.push(job);
        }

        tx.commit().await?;

        if !leased_jobs.is_empty() {
            let prev = self
                .dequeue_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if (prev + 1) % self.incremental_vacuum_n == 0 {
                let pool = self.pool.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("PRAGMA incremental_vacuum")
                        .execute(&pool)
                        .await;
                });
            }
        }

        Ok(leased_jobs)
    }

    async fn perform_maintenance(&self) -> anyhow::Result<()> {
        let _ = sqlx::query("PRAGMA incremental_vacuum")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("PRAGMA optimize").execute(&self.pool).await;
        Ok(())
    }

    async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        let now = Utc::now();
        let lock_expires_at = now + chrono::Duration::seconds(lease_seconds);
        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET lock_expires_at = ?, updated_at = ?
            WHERE id = ? AND locked_by = ? AND status = 'running'
            "#,
        )
        .bind(lock_expires_at)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected() > 0)
    }

    async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let current: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT status, locked_by FROM jobs WHERE id = ?")
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
                    finished_at = ?,
                    latency_ms = COALESCE(latency_ms, 0),
                    error_code = 'CANCELLED',
                    error_message = 'job cancelled'
                WHERE id = (
                    SELECT id
                    FROM job_attempts
                    WHERE job_id = ?
                      AND status = 'running'
                    ORDER BY attempt_no DESC
                    LIMIT 1
                )
                "#,
            )
            .bind(now)
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
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(now)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let expired: Vec<(String, Uuid)> = sqlx::query_as(
            r#"
            SELECT dataset_id, id
            FROM jobs
            WHERE status = 'running'
              AND lock_expires_at IS NOT NULL
              AND lock_expires_at <= ?
            "#,
        )
        .bind(now)
        .fetch_all(&mut *tx)
        .await?;

        for (dataset_id, job_id) in &expired {
            sqlx::query(
                r#"
                UPDATE job_attempts
                SET status = 'failed',
                    finished_at = ?,
                    latency_ms = COALESCE(latency_ms, 0),
                    error_code = 'LEASE_EXPIRED',
                    error_message = 'worker lease expired before ACK'
                WHERE dataset_id = ?
                  AND job_id = ?
                  AND status = 'running'
                "#,
            )
            .bind(now)
            .bind(dataset_id)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        }

        let res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'queued', locked_at = NULL, locked_by = NULL, lock_expires_at = NULL, updated_at = ?
            WHERE status = 'running' AND lock_expires_at IS NOT NULL AND lock_expires_at <= ?
            "#,
        )
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(res.rows_affected())
    }

    async fn start_attempts_batch(
        &self,
        _dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>> {
        if job_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut tx = self.pool.begin().await?;
        let now = Utc::now();
        let mut results = Vec::with_capacity(job_ids.len());

        for &job_id in job_ids {
            let claim_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE id = ? AND status = 'running' AND locked_by = ?",
            )
            .bind(job_id)
            .bind(worker_id)
            .fetch_one(&mut *tx)
            .await?;
            if claim_count != 1 {
                anyhow::bail!(
                    "cannot start attempt for job {job_id}: expected running lease held by {worker_id}"
                );
            }

            let max_attempt: Option<i32> =
                sqlx::query_scalar("SELECT MAX(attempt_no) FROM job_attempts WHERE job_id = ?")
                    .bind(job_id)
                    .fetch_one(&mut *tx)
                    .await?;

            let next_attempt_no = max_attempt.unwrap_or(0) + 1;
            let attempt_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO job_attempts (
                    id, dataset_id, job_id, attempt_no, status, worker_id, started_at
                ) VALUES (?, 'default', ?, ?, 'running', ?, ?)
                "#,
            )
            .bind(attempt_id)
            .bind(job_id)
            .bind(next_attempt_no)
            .bind(worker_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            results.push((job_id, attempt_id, next_attempt_no));
        }

        tx.commit().await?;
        Ok(results)
    }

    async fn mark_succeeded(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'succeeded', finished_at = ?, latency_ms = ?
            WHERE id = ? AND job_id = ? AND status = 'running'
            "#,
        )
        .bind(now)
        .bind(latency_ms)
        .bind(attempt_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        if attempt_res.rows_affected() != 1 {
            anyhow::bail!(
                "cannot complete attempt {attempt_id}: expected running attempt for job {job_id}"
            );
        }

        let completed_job = sqlx::query_as::<_, Job>(
            r#"
            UPDATE jobs
            SET status = 'succeeded', locked_at = NULL, locked_by = NULL, lock_expires_at = NULL, updated_at = ?
            WHERE id = ? AND status = 'running' AND locked_by = ?
            RETURNING
                dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds,
                status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
                created_at, updated_at
            "#,
        )
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(completed_job) = completed_job else {
            anyhow::bail!(
                "illegal job state transition to completed for job {job_id}: expected running lease held by {worker_id}"
            );
        };

        if let Some(interval_seconds) = completed_job.recurring_interval_seconds {
            let interval_seconds = interval_seconds.max(1);
            let next_run_at = completed_job.run_at + chrono::Duration::seconds(interval_seconds);
            let next_deadline_at = completed_job
                .deadline_at
                .map(|deadline| deadline + chrono::Duration::seconds(interval_seconds));
            let next_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO jobs (
                    id, dataset_id, replay_of_job_id, idempotency_key, queue, job_type, payload_json,
                    run_at, deadline_at, timeout_seconds, recurring_interval_seconds,
                    status, priority, max_attempts, created_at, updated_at
                )
                VALUES (?, 'default', ?, NULL, ?, ?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?)
                "#,
            )
            .bind(next_id)
            .bind(completed_job.id)
            .bind(&completed_job.queue)
            .bind(&completed_job.job_type)
            .bind(&completed_job.payload)
            .bind(next_run_at)
            .bind(next_deadline_at)
            .bind(completed_job.timeout_seconds)
            .bind(completed_job.recurring_interval_seconds)
            .bind(completed_job.priority)
            .bind(completed_job.max_attempts)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn mark_succeeded_batch(
        &self,
        _dataset_id: &str,
        updates: &[(Uuid, Uuid, i32)],
        worker_id: &str,
    ) -> anyhow::Result<()> {
        for &(job_id, attempt_id, latency_ms) in updates {
            self.mark_succeeded(job_id, attempt_id, worker_id, latency_ms)
                .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'failed', finished_at = ?, latency_ms = ?, error_code = ?, error_message = ?
            WHERE id = ? AND job_id = ? AND status = 'running'
            "#,
        )
        .bind(now)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
        .bind(attempt_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        if attempt_res.rows_affected() != 1 {
            anyhow::bail!(
                "cannot fail attempt {attempt_id}: expected running attempt for job {job_id}"
            );
        }

        let job_res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'queued', run_at = ?, locked_at = NULL, locked_by = NULL, lock_expires_at = NULL, updated_at = ?
            WHERE id = ? AND status = 'running' AND locked_by = ?
            "#,
        )
        .bind(next_run_at)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        if job_res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to retry_wait for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        tx.commit().await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn mark_dlq(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        let now = Utc::now();

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'failed', finished_at = ?, latency_ms = ?, error_code = ?, error_message = ?
            WHERE id = ? AND job_id = ? AND status = 'running'
            "#,
        )
        .bind(now)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
        .bind(attempt_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        if attempt_res.rows_affected() != 1 {
            anyhow::bail!(
                "cannot fail attempt {attempt_id}: expected running attempt for job {job_id}"
            );
        }

        let job_res = sqlx::query(
            r#"
            UPDATE jobs
            SET status = 'dlq', dlq_reason_code = ?, dlq_at = ?, locked_at = NULL, locked_by = NULL, lock_expires_at = NULL, updated_at = ?
            WHERE id = ? AND status = 'running' AND locked_by = ?
            "#,
        )
        .bind(reason_code)
        .bind(now)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(&mut *tx)
        .await?;
        if job_res.rows_affected() != 1 {
            anyhow::bail!(
                "illegal job state transition to dlq for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        tx.commit().await?;
        Ok(())
    }

    async fn archive_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;

        let jobs = sqlx::query_as::<_, Job>(
            r#"
            SELECT
                dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
                created_at, updated_at
            FROM jobs
            WHERE status = 'succeeded' AND updated_at < ?
            LIMIT ?
            "#,
        )
        .bind(cutoff)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;

        if jobs.is_empty() {
            tx.commit().await?;
            return Ok(0);
        }

        for j in &jobs {
            sqlx::query(
                r#"
                INSERT INTO jobs_archive (
                    id, dataset_id, replay_of_job_id, queue, job_type,
                    payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                    dlq_reason_code, dlq_at, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(j.id)
            .bind(&j.dataset_id)
            .bind(j.replay_of_job_id)
            .bind(&j.queue)
            .bind(&j.job_type)
            .bind(&j.payload)
            .bind(j.run_at)
            .bind(j.deadline_at)
            .bind(j.timeout_seconds)
            .bind(j.recurring_interval_seconds)
            .bind(&j.status)
            .bind(j.priority)
            .bind(j.max_attempts)
            .bind(&j.dlq_reason_code)
            .bind(j.dlq_at)
            .bind(j.created_at)
            .bind(j.updated_at)
            .execute(&mut *tx)
            .await?;

            sqlx::query("DELETE FROM jobs WHERE id = ?")
                .bind(j.id)
                .execute(&mut *tx)
                .await?;
        }

        let count = jobs.len() as u64;
        tx.commit().await?;
        Ok(count)
    }

    async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<(u64, u64)> {
        let res = sqlx::query(
            r#"
            DELETE FROM job_attempts
            WHERE started_at < ? AND job_id IN (
                SELECT id FROM jobs_archive WHERE updated_at < ? LIMIT ?
            )
            "#,
        )
        .bind(cutoff)
        .bind(cutoff)
        .bind(limit)
        .execute(&self.pool)
        .await?;

        Ok((res.rows_affected(), 0))
    }

    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        let job = sqlx::query_as::<_, Job>(
            r#"
            SELECT
                dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
                created_at, updated_at
            FROM jobs WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        _cursor_created_at: Option<DateTime<Utc>>,
        _cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let limit = limit.clamp(1, 500);

        let rows = match (queue, status) {
            (Some(q), Some(st)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        NULL AS last_error_code, NULL AS last_error_message,
                        dlq_reason_code, created_at, updated_at
                    FROM jobs
                    WHERE queue = ? AND status = ?
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?
                    "#,
                )
                .bind(q)
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (Some(q), None) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        NULL AS last_error_code, NULL AS last_error_message,
                        dlq_reason_code, created_at, updated_at
                    FROM jobs
                    WHERE queue = ?
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?
                    "#,
                )
                .bind(q)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, Some(st)) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        NULL AS last_error_code, NULL AS last_error_message,
                        dlq_reason_code, created_at, updated_at
                    FROM jobs
                    WHERE status = ?
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?
                    "#,
                )
                .bind(st)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            (None, None) => {
                sqlx::query_as::<_, JobListItem>(
                    r#"
                    SELECT
                        id, idempotency_key, queue, job_type, status,
                        run_at, deadline_at, timeout_seconds, recurring_interval_seconds, priority, max_attempts,
                        NULL AS last_error_code, NULL AS last_error_message,
                        dlq_reason_code, created_at, updated_at
                    FROM jobs
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows)
    }

    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        let mut tx = self.pool.begin().await?;

        let src = sqlx::query_as::<_, Job>(
            r#"
            SELECT
                dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
                created_at, updated_at
            FROM jobs WHERE id = ?
            "#,
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;

        let new_id = Uuid::new_v4();
        let target_queue = override_queue.unwrap_or(&src.queue);
        let target_run_at = override_run_at.unwrap_or_else(Utc::now);
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO jobs (
                id, dataset_id, replay_of_job_id, queue, job_type,
                payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds, status, priority, max_attempts,
                created_at, updated_at
            ) VALUES (?, 'default', ?, ?, ?, ?, ?, 'queued', ?, ?, ?, ?)
            "#,
        )
        .bind(new_id)
        .bind(job_id)
        .bind(target_queue)
        .bind(&src.job_type)
        .bind(&src.payload)
        .bind(target_run_at)
        .bind(src.priority)
        .bind(src.max_attempts)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        self.notify_queue(target_queue);
        Ok(new_id)
    }
}

#[async_trait]
impl StreamBackend for SqliteBackend {
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        let now = Utc::now();
        let res = sqlx::query(
            r#"
            INSERT INTO stream_events (stream_name, event_type, payload_json, created_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(stream)
        .bind(event.event_type)
        .bind(event.payload_json)
        .bind(now)
        .execute(&self.pool)
        .await?;

        let seq = res.last_insert_rowid();
        self.notify_stream(stream);
        Ok(seq)
    }

    async fn subscribe_stream(
        &self,
        stream: &str,
        _consumer_group: &str,
        _last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        use tokio_stream::wrappers::BroadcastStream;
        use tokio_stream::StreamExt;

        let rx = {
            let mut notifiers = self.stream_notifiers.write().unwrap();
            let tx = notifiers
                .entry(stream.to_string())
                .or_insert_with(|| tokio::sync::broadcast::channel(128).0);
            tx.subscribe()
        };

        let bcast_stream = BroadcastStream::new(rx).filter_map(|res| res.ok());
        let interval_stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
            std::time::Duration::from_millis(100),
        ))
        .map(|_| ());

        let merged = bcast_stream.merge(interval_stream);
        Ok(Box::pin(merged))
    }

    async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO stream_offsets (consumer_group, stream_name, last_acked_seq, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (consumer_group, stream_name)
            DO UPDATE SET last_acked_seq = MAX(stream_offsets.last_acked_seq, EXCLUDED.last_acked_seq),
                          updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(consumer_group)
        .bind(stream)
        .bind(seq)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"
            SELECT sequence_no, stream_name, event_type, payload_json, created_at
            FROM stream_events
            WHERE stream_name = ? AND sequence_no > ?
            ORDER BY sequence_no ASC
            LIMIT ?
            "#,
        )
        .bind(stream)
        .bind(after_seq)
        .bind(limit.clamp(1, 1000))
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        let info = sqlx::query_as::<_, ConsumerGroupStatus>(
            r#"
            SELECT consumer_group, stream_name, last_acked_seq, updated_at
            FROM stream_offsets
            WHERE stream_name = ?
            "#,
        )
        .bind(stream)
        .fetch_all(&self.pool)
        .await?;

        Ok(info)
    }
}
