use crate::{
    backend::{
        memory::MemoryBackend, JobExplanation, NotificationStream, ObservabilityBackend,
        QueueMetrics, StorageBackend, StreamBackend,
    },
    model::{ConsumerGroupStatus, Event, Job, JobListItem, NewEvent, NewJob},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Log record representing a single call executed against a [`MockBackend`].
#[derive(Debug, Clone)]
pub enum CallRecord {
    RunMigrations,
    HealthCheck,
    Enqueue(NewJob),
    Subscribe(String),
    PublishStream {
        stream: String,
        event: NewEvent,
    },
    SubscribeStream {
        stream: String,
        consumer_group: String,
        last_seq: Option<i64>,
    },
    AckStream {
        stream: String,
        consumer_group: String,
        seq: i64,
    },
    ReadEventsStream {
        stream: String,
        after_seq: i64,
        limit: i64,
    },
    PruneEventsStream {
        stream: String,
        through_seq: i64,
    },
    ConsumerGroupInfoStream(String),
    LeaseJobsBatch {
        queue: String,
        worker_id: String,
        lease_seconds: i64,
        batch_size: i64,
    },
    LeaseJobsBatchWithOrdering {
        queue: String,
        worker_id: String,
        lease_seconds: i64,
        batch_size: i64,
        ordering: crate::model::QueueOrdering,
    },
    ReapExpiredLocks,
    StartAttemptsBatch {
        job_ids: Vec<Uuid>,
        worker_id: String,
    },
    MarkSucceeded {
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: String,
        latency_ms: i32,
    },
    MarkSucceededBatch {
        dataset_id: String,
        updates: Vec<(Uuid, Uuid, i32)>,
        worker_id: String,
    },
    RescheduleForRetry {
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: String,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: String,
        error_message: String,
        attempt_no: i32,
    },
    MarkDlq {
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: String,
        latency_ms: i32,
        reason_code: String,
        error_code: String,
        error_message: String,
        attempt_no: i32,
    },
    ArchiveSucceededOlderThan {
        cutoff: DateTime<Utc>,
        limit: i64,
    },
    DeleteHistoryForSucceededOlderThan {
        cutoff: DateTime<Utc>,
        limit: i64,
    },
    PerformMaintenance,
    ExtendLease {
        job_id: Uuid,
        worker_id: String,
        lease_seconds: i64,
    },
    CancelJob {
        job_id: Uuid,
        worker_id: Option<String>,
    },
    GetJob(Uuid),
    ListJobs {
        queue: Option<String>,
        status: Option<String>,
        limit: i64,
    },
    ReplayJob {
        job_id: Uuid,
        override_queue: Option<String>,
        override_run_at: Option<DateTime<Utc>>,
    },
}

/// Recording mock storage backend wrapper for assertion-driven integration testing.
#[derive(Clone)]
pub struct MockBackend {
    inner: Arc<dyn StorageBackend>,
    calls: Arc<Mutex<Vec<CallRecord>>>,
}

impl MockBackend {
    /// Creates a new `MockBackend` wrapping a target inner [`StorageBackend`].
    pub fn new(inner: Arc<dyn StorageBackend>) -> Self {
        Self {
            inner,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a `MockBackend` backed by an in-memory storage engine ([`MemoryBackend`]).
    pub fn with_memory() -> Self {
        Self::new(Arc::new(MemoryBackend::new()))
    }

    /// Returns a copy of all recorded backend call invocations.
    pub fn calls(&self) -> Vec<CallRecord> {
        self.calls.lock().unwrap().clone()
    }

    /// Clears recorded call history.
    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    /// Asserts that a job of `job_type` was enqueued through this backend.
    pub fn assert_enqueued_job_type(&self, job_type: &str) {
        let calls = self.calls();
        let found = calls.iter().any(|c| match c {
            CallRecord::Enqueue(job) => job.job_type == job_type,
            _ => false,
        });
        assert!(
            found,
            "Expected job of type '{job_type}' to be enqueued in MockBackend, but calls were: {calls:?}"
        );
    }

    /// Asserts that a job was completed successfully via `mark_succeeded`.
    pub fn assert_marked_succeeded(&self, target_job_id: Uuid) {
        let calls = self.calls();
        let found = calls.iter().any(|c| match c {
            CallRecord::MarkSucceeded { job_id, .. } => *job_id == target_job_id,
            CallRecord::MarkSucceededBatch { updates, .. } => {
                updates.iter().any(|(jid, _, _)| *jid == target_job_id)
            }
            _ => false,
        });
        assert!(
            found,
            "Expected job {target_job_id} to be marked succeeded in MockBackend"
        );
    }

    /// Asserts that a job was moved to Dead-Letter Queue (DLQ) via `mark_dlq`.
    pub fn assert_marked_dlq(&self, target_job_id: Uuid) {
        let calls = self.calls();
        let found = calls.iter().any(|c| match c {
            CallRecord::MarkDlq { job_id, .. } => *job_id == target_job_id,
            _ => false,
        });
        assert!(
            found,
            "Expected job {target_job_id} to be marked DLQ in MockBackend"
        );
    }
}

#[async_trait]
impl StorageBackend for MockBackend {
    fn capabilities(&self) -> crate::model::BackendCapabilities {
        self.inner.capabilities()
    }

    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        Some(self)
    }

    fn as_observability(&self) -> Option<&dyn ObservabilityBackend> {
        Some(self)
    }

    async fn run_migrations(&self) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(CallRecord::RunMigrations);
        self.inner.run_migrations().await
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(CallRecord::HealthCheck);
        self.inner.health_check().await
    }

    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::Enqueue(job.clone()));
        self.inner.enqueue(job).await
    }

    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::Subscribe(queue.to_string()));
        self.inner.subscribe(queue).await
    }

    async fn lease_jobs_batch(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>> {
        self.calls.lock().unwrap().push(CallRecord::LeaseJobsBatch {
            queue: queue.to_string(),
            worker_id: worker_id.to_string(),
            lease_seconds,
            batch_size,
        });
        self.inner
            .lease_jobs_batch(queue, worker_id, lease_seconds, batch_size)
            .await
    }

    async fn lease_jobs_batch_with_ordering(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
        ordering: crate::model::QueueOrdering,
    ) -> anyhow::Result<Vec<Job>> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::LeaseJobsBatchWithOrdering {
                queue: queue.to_string(),
                worker_id: worker_id.to_string(),
                lease_seconds,
                batch_size,
                ordering,
            });
        self.inner
            .lease_jobs_batch_with_ordering(queue, worker_id, lease_seconds, batch_size, ordering)
            .await
    }

    async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::ReapExpiredLocks);
        self.inner.reap_expired_locks().await
    }

    async fn start_attempts_batch(
        &self,
        dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::StartAttemptsBatch {
                job_ids: job_ids.to_vec(),
                worker_id: worker_id.to_string(),
            });
        self.inner
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
        self.calls.lock().unwrap().push(CallRecord::MarkSucceeded {
            job_id,
            attempt_id,
            worker_id: worker_id.to_string(),
            latency_ms,
        });
        self.inner
            .mark_succeeded(job_id, attempt_id, worker_id, latency_ms)
            .await
    }

    async fn mark_succeeded_batch(
        &self,
        dataset_id: &str,
        updates: &[(Uuid, Uuid, i32)],
        worker_id: &str,
    ) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::MarkSucceededBatch {
                dataset_id: dataset_id.to_string(),
                updates: updates.to_vec(),
                worker_id: worker_id.to_string(),
            });
        self.inner
            .mark_succeeded_batch(dataset_id, updates, worker_id)
            .await
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
        attempt_no: i32,
    ) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::RescheduleForRetry {
                job_id,
                attempt_id,
                worker_id: worker_id.to_string(),
                latency_ms,
                next_run_at,
                error_code: error_code.to_string(),
                error_message: error_message.to_string(),
                attempt_no,
            });
        self.inner
            .reschedule_for_retry(
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
    ) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(CallRecord::MarkDlq {
            job_id,
            attempt_id,
            worker_id: worker_id.to_string(),
            latency_ms,
            reason_code: reason_code.to_string(),
            error_code: error_code.to_string(),
            error_message: error_message.to_string(),
            attempt_no,
        });
        self.inner
            .mark_dlq(
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

    async fn archive_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<u64> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::ArchiveSucceededOlderThan { cutoff, limit });
        self.inner.archive_succeeded_older_than(cutoff, limit).await
    }

    async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<(u64, u64)> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::DeleteHistoryForSucceededOlderThan { cutoff, limit });
        self.inner
            .delete_history_for_succeeded_older_than(cutoff, limit)
            .await
    }

    async fn perform_maintenance(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::PerformMaintenance);
        self.inner.perform_maintenance().await
    }

    async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        self.calls.lock().unwrap().push(CallRecord::ExtendLease {
            job_id,
            worker_id: worker_id.to_string(),
            lease_seconds,
        });
        self.inner
            .extend_lease(job_id, worker_id, lease_seconds)
            .await
    }

    async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(CallRecord::CancelJob {
            job_id,
            worker_id: worker_id.map(|id| id.to_string()),
        });
        self.inner.cancel_job(job_id, worker_id).await
    }

    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        self.calls.lock().unwrap().push(CallRecord::GetJob(job_id));
        self.inner.get_job(job_id).await
    }

    async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        cursor_created_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        self.calls.lock().unwrap().push(CallRecord::ListJobs {
            queue: queue.map(|s| s.to_string()),
            status: status.map(|s| s.to_string()),
            limit,
        });
        self.inner
            .list_jobs(queue, status, limit, cursor_created_at, cursor_id)
            .await
    }

    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        self.calls.lock().unwrap().push(CallRecord::ReplayJob {
            job_id,
            override_queue: override_queue.map(|s| s.to_string()),
            override_run_at,
        });
        self.inner
            .replay_job(job_id, override_queue, override_run_at)
            .await
    }
}

#[async_trait]
impl ObservabilityBackend for MockBackend {
    async fn explain_job(&self, job_id: Uuid) -> anyhow::Result<Option<JobExplanation>> {
        if let Some(observability) = self.inner.as_observability() {
            observability.explain_job(job_id).await
        } else {
            Ok(None)
        }
    }

    async fn queue_metrics(&self, queue: Option<&str>) -> anyhow::Result<Vec<QueueMetrics>> {
        if let Some(observability) = self.inner.as_observability() {
            observability.queue_metrics(queue).await
        } else {
            Ok(Vec::new())
        }
    }
}

#[async_trait]
impl StreamBackend for MockBackend {
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        self.calls.lock().unwrap().push(CallRecord::PublishStream {
            stream: stream.to_string(),
            event: event.clone(),
        });
        if let Some(sb) = self.inner.as_stream() {
            sb.publish(stream, event).await
        } else {
            Ok(1)
        }
    }

    async fn subscribe_stream(
        &self,
        stream: &str,
        consumer_group: &str,
        last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::SubscribeStream {
                stream: stream.to_string(),
                consumer_group: consumer_group.to_string(),
                last_seq,
            });
        if let Some(sb) = self.inner.as_stream() {
            sb.subscribe_stream(stream, consumer_group, last_seq).await
        } else {
            anyhow::bail!("inner backend does not support StreamBackend")
        }
    }

    async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(CallRecord::AckStream {
            stream: stream.to_string(),
            consumer_group: consumer_group.to_string(),
            seq,
        });
        if let Some(sb) = self.inner.as_stream() {
            sb.ack(stream, consumer_group, seq).await
        } else {
            Ok(())
        }
    }

    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::ReadEventsStream {
                stream: stream.to_string(),
                after_seq,
                limit,
            });
        if let Some(sb) = self.inner.as_stream() {
            sb.read_events(stream, after_seq, limit).await
        } else {
            Ok(Vec::new())
        }
    }

    async fn prune_events(&self, stream: &str, through_seq: i64) -> anyhow::Result<u64> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::PruneEventsStream {
                stream: stream.to_string(),
                through_seq,
            });
        if let Some(sb) = self.inner.as_stream() {
            sb.prune_events(stream, through_seq).await
        } else {
            Ok(0)
        }
    }

    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        self.calls
            .lock()
            .unwrap()
            .push(CallRecord::ConsumerGroupInfoStream(stream.to_string()));
        if let Some(sb) = self.inner.as_stream() {
            sb.consumer_group_info(stream).await
        } else {
            Ok(Vec::new())
        }
    }
}
