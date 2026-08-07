use async_trait::async_trait;
use chrono::{DateTime, Utc};
use postgresflow_core::{
    backend::StorageBackend,
    model::{Job, JobListItem, NewJob},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    db::run_migrations,
    jobs::{attempts::AttemptsRepo, maintenance::MaintenanceRepo, repo::JobsRepo},
};

/// PostgreSQL implementation of [`StorageBackend`] using SQLx.
#[derive(Clone)]
pub struct PostgresBackend {
    pool: PgPool,
    jobs_repo: JobsRepo,
    attempts_repo: AttemptsRepo,
    maintenance_repo: MaintenanceRepo,
}

impl PostgresBackend {
    /// Creates a new `PostgresBackend` from a SQLx connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self {
            jobs_repo: JobsRepo::new(pool.clone()),
            attempts_repo: AttemptsRepo::new(pool.clone()),
            maintenance_repo: MaintenanceRepo::new(pool.clone()),
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
}

#[async_trait]
impl StorageBackend for PostgresBackend {
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
        self.attempts_repo
            .finish_succeeded(attempt_id, latency_ms)
            .await?;
        self.jobs_repo.mark_succeeded(job_id, worker_id).await?;
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
        let attempt_updates: Vec<(Uuid, i32)> = updates
            .iter()
            .map(|(_, attempt_id, latency_ms)| (*attempt_id, *latency_ms))
            .collect();
        let job_ids: Vec<Uuid> = updates.iter().map(|(job_id, _, _)| *job_id).collect();

        self.attempts_repo
            .finish_succeeded_batch(&attempt_updates)
            .await?;
        self.jobs_repo
            .mark_succeeded_batch_for_dataset(dataset_id, &job_ids, worker_id)
            .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        _worker_id: &str,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        self.attempts_repo
            .finish_failed(attempt_id, latency_ms, error_code, error_message)
            .await?;
        self.jobs_repo
            .reschedule_for_retry(job_id, next_run_at, Some(error_code), Some(error_message))
            .await?;
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
        self.attempts_repo
            .finish_failed(attempt_id, latency_ms, error_code, error_message)
            .await?;
        self.jobs_repo
            .mark_dlq(
                job_id,
                worker_id,
                reason_code,
                Some(error_code),
                Some(error_message),
            )
            .await?;
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
}
