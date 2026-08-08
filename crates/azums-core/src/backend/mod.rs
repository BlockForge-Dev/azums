pub mod memory;
pub mod mock;
pub mod stream;

pub use memory::{MemoryAttempt, MemoryBackend};
pub use mock::{CallRecord, MockBackend};
pub use stream::StreamBackend;

use crate::model::{Job, JobListItem, NewJob};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::pin::Pin;
use uuid::Uuid;

/// Type alias for asynchronous notification event streams produced by [`StorageBackend::subscribe`].
pub type NotificationStream = Pin<Box<dyn futures_core::Stream<Item = ()> + Send>>;

/// Async, backend-agnostic storage interface for job queue operations.
///
/// Implementations of `StorageBackend` manage job persistence, leasing, retry scheduling,
/// Dead-Letter Queue (DLQ) routing, maintenance archiving, and health probes.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Returns reference to StreamBackend if supported by this storage implementation.
    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        None
    }

    /// Executes backend schema migrations or setup steps.
    async fn run_migrations(&self) -> anyhow::Result<()>;

    /// Performs a health check to verify backend connectivity and readiness.
    async fn health_check(&self) -> anyhow::Result<()>;

    /// Enqueues a new job into the backend queue.
    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid>;

    /// Subscribes to job enqueue notification events for a specific queue.
    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream>;

    /// Leases up to `batch_size` runnable jobs for a specified worker ID.
    async fn lease_jobs_batch(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>>;

    /// Reaps expired locks from inactive workers, resetting their status back to queued.
    async fn reap_expired_locks(&self) -> anyhow::Result<u64>;

    /// Starts job execution attempt records, returning `(job_id, attempt_id, attempt_number)` tuples.
    async fn start_attempts_batch(
        &self,
        dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>>;

    /// Marks a single job execution attempt as succeeded.
    async fn mark_succeeded(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()>;

    /// Marks a batch of job execution attempts as succeeded.
    async fn mark_succeeded_batch(
        &self,
        dataset_id: &str,
        updates: &[(Uuid, Uuid, i32)],
        worker_id: &str,
    ) -> anyhow::Result<()>;

    /// Records a failed attempt and reschedules the job for a future retry attempt.
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
        attempt_no: i32,
    ) -> anyhow::Result<()>;

    /// Records a failed attempt and transitions the job to the Dead-Letter Queue (DLQ).
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
        attempt_no: i32,
    ) -> anyhow::Result<()>;

    /// Moves succeeded jobs older than `cutoff` into an archive table or storage location.
    async fn archive_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<u64>;

    /// Prunes attempt audit logs and decision records for succeeded jobs older than `cutoff`.
    async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<(u64, u64)>;

    /// Fetches a single job record by ID.
    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>>;

    /// Fetches a list of jobs matching filters with cursor pagination.
    async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>>;

    /// Atomically replays a job by ID into the queue.
    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid>;

    /// Dequeues and leases up to `batch_size` runnable jobs (alias for `lease_jobs_batch`).
    async fn dequeue_and_lease(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>> {
        self.lease_jobs_batch(queue, worker_id, lease_seconds, batch_size)
            .await
    }

    /// Marks a job attempt as completed (alias for `mark_succeeded`).
    async fn complete_job(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()> {
        self.mark_succeeded(job_id, attempt_id, worker_id, latency_ms)
            .await
    }

    /// Reschedules a job for retry (alias for `reschedule_for_retry`).
    #[allow(clippy::too_many_arguments)]
    async fn retry_job(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
    ) -> anyhow::Result<()> {
        self.reschedule_for_retry(
            job_id,
            attempt_id,
            worker_id,
            latency_ms,
            next_run_at,
            error_code,
            error_message,
            attempt_no,
        )
        .await
    }

    /// Moves a job to DLQ on failure (alias for `mark_dlq`).
    #[allow(clippy::too_many_arguments)]
    async fn fail_job(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
    ) -> anyhow::Result<()> {
        self.mark_dlq(
            job_id,
            attempt_id,
            worker_id,
            latency_ms,
            reason_code,
            error_code,
            error_message,
            attempt_no,
        )
        .await
    }
}
