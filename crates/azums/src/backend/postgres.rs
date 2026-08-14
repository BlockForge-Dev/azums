use async_trait::async_trait;
use azums_core::{
    backend::{NotificationStream, StorageBackend, StreamBackend},
    model::{ConsumerGroupStatus, Event, Job, JobListItem, NewEvent, NewJob},
};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    db::run_migrations,
    jobs::{
        attempts::AttemptsRepo, maintenance::MaintenanceRepo, repo::JobsRepo,
        stream_repo::StreamRepo,
    },
};

/// PostgreSQL implementation of [`StorageBackend`] using SQLx.
#[derive(Clone)]
pub struct PostgresBackend {
    pool: PgPool,
    jobs_repo: JobsRepo,
    attempts_repo: AttemptsRepo,
    maintenance_repo: MaintenanceRepo,
    stream_repo: StreamRepo,
}

impl PostgresBackend {
    /// Creates a new `PostgresBackend` from a SQLx connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            jobs_repo: JobsRepo::new(pool.clone()),
            attempts_repo: AttemptsRepo::new(pool.clone()),
            maintenance_repo: MaintenanceRepo::new(pool.clone()),
            stream_repo: StreamRepo::new(pool.clone()),
            pool,
        }
    }

    /// Creates a new `PostgresBackend` with a pool and database connection URL for dedicated `LISTEN` sockets.
    pub fn new_with_url(pool: PgPool, database_url: impl Into<String>) -> Self {
        let url_str = database_url.into();
        Self {
            jobs_repo: JobsRepo::new_with_url(pool.clone(), url_str),
            attempts_repo: AttemptsRepo::new(pool.clone()),
            maintenance_repo: MaintenanceRepo::new(pool.clone()),
            stream_repo: StreamRepo::new(pool.clone()),
            pool,
        }
    }

    /// Returns reference to the underlying SQLx `PgPool`.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Returns reference to the underlying `JobsRepo`.
    pub fn jobs_repo(&self) -> &JobsRepo {
        &self.jobs_repo
    }

    /// Returns reference to the underlying `AttemptsRepo`.
    pub fn attempts_repo(&self) -> &AttemptsRepo {
        &self.attempts_repo
    }

    /// Returns reference to the underlying `StreamRepo`.
    pub fn stream_repo(&self) -> &StreamRepo {
        &self.stream_repo
    }

    /// Returns reference to the underlying `MaintenanceRepo`.
    pub fn maintenance_repo(&self) -> &MaintenanceRepo {
        &self.maintenance_repo
    }

    /// Inserts a job using the caller's PostgreSQL transaction.
    pub async fn enqueue_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: NewJob,
    ) -> anyhow::Result<Uuid> {
        self.jobs_repo.enqueue_in_tx(tx, job).await
    }
}

#[async_trait]
impl StorageBackend for PostgresBackend {
    fn capabilities(&self) -> azums_core::BackendCapabilities {
        azums_core::BackendCapabilities::postgres()
    }

    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        Some(self)
    }

    async fn run_migrations(&self) -> anyhow::Result<()> {
        run_migrations(&self.pool).await
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        self.jobs_repo.enqueue(job).await
    }

    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream> {
        self.jobs_repo.subscribe(queue).await
    }

    async fn lease_jobs_batch(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>> {
        self.jobs_repo
            .lease_jobs_batch(queue, worker_id, lease_seconds, batch_size)
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
        self.jobs_repo
            .lease_jobs_batch_with_ordering(queue, worker_id, lease_seconds, batch_size, ordering)
            .await
    }

    async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let count = self.jobs_repo.reap_expired_locks().await?;
        Ok(count as u64)
    }

    async fn start_attempts_batch(
        &self,
        dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>> {
        self.attempts_repo
            .start_attempts_batch(dataset_ids, job_ids, worker_id)
            .await
    }

    async fn mark_succeeded(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'succeeded',
                finished_at = now(),
                latency_ms = $2
            WHERE id = $1
              AND job_id = $3
              AND status = 'running'
            "#,
        )
        .bind(attempt_id)
        .bind(latency_ms)
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
            SET status = 'succeeded',
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                updated_at = now()
            WHERE id = $1
              AND locked_by = $2
              AND status = 'running'
            RETURNING *
            "#,
        )
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
            let next_run_at =
                completed_job.run_at + chrono::Duration::seconds(interval_seconds.max(1));
            let next_deadline_at = completed_job
                .deadline_at
                .map(|deadline| deadline + chrono::Duration::seconds(interval_seconds.max(1)));
            let next_dataset_id =
                crate::jobs::JobsRepo::dataset_id_for(&completed_job.queue, next_run_at);
            self.jobs_repo
                .ensure_dataset_partition(&next_dataset_id)
                .await?;

            sqlx::query(
                r#"
                INSERT INTO jobs (
                    dataset_id, replay_of_job_id, idempotency_key,
                    queue, job_type, payload_json, run_at,
                    deadline_at, timeout_seconds, recurring_interval_seconds,
                    status, priority, max_attempts
                )
                VALUES (
                    $1, $2, NULL,
                    $3, $4, $5, $6,
                    $7::timestamptz, $8::integer, $9::integer,
                    'queued', $10, $11
                )
                "#,
            )
            .bind(next_dataset_id)
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
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn mark_succeeded_batch(
        &self,
        dataset_id: &str,
        updates: &[(Uuid, Uuid, i32)],
        worker_id: &str,
    ) -> anyhow::Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for &(job_id, attempt_id, latency_ms) in updates {
            let attempt_res = sqlx::query(
                r#"
                UPDATE job_attempts
                SET status = 'succeeded',
                    finished_at = now(),
                    latency_ms = $2
                WHERE dataset_id = $1
                  AND id = $3
                  AND job_id = $4
                  AND status = 'running'
                "#,
            )
            .bind(dataset_id)
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

            let job_res = sqlx::query(
                r#"
                UPDATE jobs
                SET status = 'succeeded',
                    locked_at = NULL,
                    locked_by = NULL,
                    lock_expires_at = NULL,
                    updated_at = now()
                WHERE dataset_id = $1
                  AND id = $2
                  AND locked_by = $3
                  AND status = 'running'
                "#,
            )
            .bind(dataset_id)
            .bind(job_id)
            .bind(worker_id)
            .execute(&mut *tx)
            .await?;
            if job_res.rows_affected() != 1 {
                anyhow::bail!(
                    "illegal job state transition to completed for job {job_id}: expected running lease held by {worker_id}"
                );
            }
        }

        tx.commit().await?;
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

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'failed',
                finished_at = now(),
                latency_ms = $2,
                error_code = $3,
                error_message = $4
            WHERE id = $1
              AND job_id = $5
              AND status = 'running'
            "#,
        )
        .bind(attempt_id)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
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
            SET status = 'queued',
                run_at = $2,
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                last_error_code = $4,
                last_error_message = $5,
                updated_at = now()
            WHERE id = $1
              AND locked_by = $3
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(next_run_at)
        .bind(worker_id)
        .bind(error_code)
        .bind(error_message)
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

        let attempt_res = sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = 'failed',
                finished_at = now(),
                latency_ms = $2,
                error_code = $3,
                error_message = $4
            WHERE id = $1
              AND job_id = $5
              AND status = 'running'
            "#,
        )
        .bind(attempt_id)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
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
            SET status = 'dlq',
                dlq_reason_code = $2,
                dlq_at = now(),
                locked_at = NULL,
                locked_by = NULL,
                lock_expires_at = NULL,
                last_error_code = $4,
                last_error_message = $5,
                updated_at = now()
            WHERE id = $1
              AND locked_by = $3
              AND status = 'running'
            "#,
        )
        .bind(job_id)
        .bind(reason_code)
        .bind(worker_id)
        .bind(error_code)
        .bind(error_message)
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
        let count = self
            .maintenance_repo
            .archive_succeeded_older_than(cutoff, limit)
            .await?;
        Ok(count as u64)
    }

    async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<(u64, u64)> {
        let (attempts, decisions) = self
            .maintenance_repo
            .delete_history_for_succeeded_older_than(cutoff, limit)
            .await?;
        Ok((attempts, decisions))
    }

    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        self.jobs_repo.get_job(job_id).await
    }

    async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        self.jobs_repo
            .list_jobs(queue, status, limit, cursor_created_at, cursor_id)
            .await
    }

    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        self.jobs_repo
            .replay_job(job_id, override_queue, override_run_at)
            .await
    }

    async fn perform_maintenance(&self) -> anyhow::Result<()> {
        self.maintenance_repo.vacuum_analyze().await
    }

    async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        self.jobs_repo
            .extend_lease(job_id, worker_id, lease_seconds)
            .await
    }

    async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        self.jobs_repo.cancel_job(job_id, worker_id).await
    }
}

#[async_trait]
impl StreamBackend for PostgresBackend {
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        self.stream_repo.publish(stream, event).await
    }

    async fn subscribe_stream(
        &self,
        stream: &str,
        consumer_group: &str,
        last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        self.stream_repo
            .subscribe_stream(stream, consumer_group, last_seq)
            .await
    }

    async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        self.stream_repo.ack(stream, consumer_group, seq).await
    }

    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        self.stream_repo.read_events(stream, after_seq, limit).await
    }

    async fn prune_events(&self, stream: &str, through_seq: i64) -> anyhow::Result<u64> {
        self.stream_repo.prune_events(stream, through_seq).await
    }

    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        self.stream_repo.consumer_group_info(stream).await
    }
}
