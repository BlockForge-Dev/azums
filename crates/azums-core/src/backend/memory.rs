use crate::{
    backend::{NotificationStream, StorageBackend, StreamBackend},
    model::{ConsumerGroupStatus, Event, Job, JobListItem, JobStatus, NewEvent, NewJob},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use uuid::Uuid;

/// Attempt history record stored in [`MemoryBackend`].
#[derive(Debug, Clone)]
pub struct MemoryAttempt {
    pub id: Uuid,
    pub dataset_id: String,
    pub job_id: Uuid,
    pub attempt_no: i32,
    pub status: String,
    pub worker_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub latency_ms: Option<i32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Default)]
struct InnerState {
    jobs: HashMap<Uuid, Job>,
    archive: HashMap<Uuid, Job>,
    attempts: HashMap<Uuid, MemoryAttempt>,
    streams: HashMap<String, Vec<Event>>,
    stream_offsets: HashMap<(String, String), ConsumerGroupStatus>,
}

/// Thread-safe in-memory implementation of [`StorageBackend`].
///
/// Ideal for unit testing, ephemeral workloads, and local development without external database servers.
#[derive(Debug, Clone, Default)]
pub struct MemoryBackend {
    state: Arc<RwLock<InnerState>>,
    notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
    stream_notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
}

impl MemoryBackend {
    /// Creates a new, empty `MemoryBackend`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets and clears all stored jobs and attempt history.
    pub fn clear(&self) {
        let mut state = self.state.write().unwrap();
        state.jobs.clear();
        state.archive.clear();
        state.attempts.clear();
        state.streams.clear();
        state.stream_offsets.clear();
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
}

#[async_trait]
impl StorageBackend for MemoryBackend {
    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        Some(self)
    }
    async fn run_migrations(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        let job_id = Uuid::new_v4();
        let now = Utc::now();
        let queue_name = job.queue.clone();

        let job_entity = Job {
            dataset_id: "default".to_string(),
            replay_of_job_id: None,
            id: job_id,
            queue: job.queue,
            job_type: job.job_type,
            payload: job.payload_json,
            run_at: job.run_at,
            status: JobStatus::Queued.as_str().to_string(),
            priority: job.priority,
            max_attempts: job.max_attempts,
            locked_at: None,
            locked_by: None,
            lock_expires_at: None,
            dlq_reason_code: None,
            dlq_at: None,
            created_at: now,
            updated_at: now,
        };

        {
            let mut state = self.state.write().unwrap();
            state.jobs.insert(job_id, job_entity);
        }

        self.notify_queue(&queue_name);
        Ok(job_id)
    }

    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream> {
        let rx = {
            let mut notifiers = self.notifiers.write().unwrap();
            let tx = notifiers
                .entry(queue.to_string())
                .or_insert_with(|| tokio::sync::broadcast::channel(128).0);
            tx.subscribe()
        };

        let stream = BroadcastStream::new(rx).filter_map(|res| res.ok());
        Ok(Box::pin(stream))
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
            crate::model::QueueOrdering::Fifo,
        )
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
        let mut state = self.state.write().unwrap();
        let now = Utc::now();

        let mut candidates: Vec<Job> = state
            .jobs
            .values()
            .filter(|j| j.queue == queue && j.status == "queued" && j.run_at <= now)
            .cloned()
            .collect();

        match ordering {
            crate::model::QueueOrdering::Fifo => {
                candidates.sort_by(|a, b| {
                    b.priority
                        .cmp(&a.priority)
                        .then_with(|| a.run_at.cmp(&b.run_at))
                        .then_with(|| a.created_at.cmp(&b.created_at))
                        .then_with(|| a.id.cmp(&b.id))
                });
            }
            crate::model::QueueOrdering::Fastest => {
                candidates.sort_by_key(|a| std::cmp::Reverse(a.priority));
            }
        }

        let candidates: Vec<Job> = candidates.into_iter().take(batch_size as usize).collect();
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let lock_expires_at = now + chrono::Duration::seconds(lease_seconds);
        let mut leased = Vec::with_capacity(candidates.len());

        for mut candidate in candidates {
            if let Some(j) = state.jobs.get_mut(&candidate.id) {
                j.status = JobStatus::Running.as_str().to_string();
                j.locked_at = Some(now);
                j.locked_by = Some(worker_id.to_string());
                j.lock_expires_at = Some(lock_expires_at);
                j.updated_at = now;

                candidate.status = j.status.clone();
                candidate.locked_at = j.locked_at;
                candidate.locked_by = j.locked_by.clone();
                candidate.lock_expires_at = j.lock_expires_at;
                candidate.updated_at = j.updated_at;

                leased.push(candidate);
            }
        }

        Ok(leased)
    }

    async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let mut state = self.state.write().unwrap();
        let now = Utc::now();
        let mut reaped = 0u64;

        for job in state.jobs.values_mut() {
            if job.status == "running" {
                if let Some(exp) = job.lock_expires_at {
                    if exp <= now {
                        job.status = JobStatus::Queued.as_str().to_string();
                        job.locked_at = None;
                        job.locked_by = None;
                        job.lock_expires_at = None;
                        job.updated_at = now;
                        reaped += 1;
                    }
                }
            }
        }

        Ok(reaped)
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

        let mut state = self.state.write().unwrap();
        let now = Utc::now();
        let mut results = Vec::with_capacity(job_ids.len());

        for &job_id in job_ids {
            let max_attempt = state
                .attempts
                .values()
                .filter(|a| a.job_id == job_id)
                .map(|a| a.attempt_no)
                .max()
                .unwrap_or(0);

            let next_attempt_no = max_attempt + 1;
            let attempt_id = Uuid::new_v4();

            let attempt = MemoryAttempt {
                id: attempt_id,
                dataset_id: "default".to_string(),
                job_id,
                attempt_no: next_attempt_no,
                status: "running".to_string(),
                worker_id: worker_id.to_string(),
                started_at: now,
                finished_at: None,
                latency_ms: None,
                error_code: None,
                error_message: None,
            };

            state.attempts.insert(attempt_id, attempt);
            results.push((job_id, attempt_id, next_attempt_no));
        }

        Ok(results)
    }

    async fn mark_succeeded(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        _worker_id: &str,
        latency_ms: i32,
    ) -> anyhow::Result<()> {
        let mut state = self.state.write().unwrap();
        let now = Utc::now();

        if let Some(att) = state.attempts.get_mut(&attempt_id) {
            att.status = "succeeded".to_string();
            att.finished_at = Some(now);
            att.latency_ms = Some(latency_ms);
        }

        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.status = JobStatus::Succeeded.as_str().to_string();
            job.locked_at = None;
            job.locked_by = None;
            job.lock_expires_at = None;
            job.updated_at = now;
        }

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
        _worker_id: &str,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut state = self.state.write().unwrap();
        let now = Utc::now();

        if let Some(att) = state.attempts.get_mut(&attempt_id) {
            att.status = "failed".to_string();
            att.finished_at = Some(now);
            att.latency_ms = Some(latency_ms);
            att.error_code = Some(error_code.to_string());
            att.error_message = Some(error_message.to_string());
        }

        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.status = JobStatus::Queued.as_str().to_string();
            job.run_at = next_run_at;
            job.locked_at = None;
            job.locked_by = None;
            job.lock_expires_at = None;
            job.updated_at = now;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn mark_dlq(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        _worker_id: &str,
        latency_ms: i32,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut state = self.state.write().unwrap();
        let now = Utc::now();

        if let Some(att) = state.attempts.get_mut(&attempt_id) {
            att.status = "failed".to_string();
            att.finished_at = Some(now);
            att.latency_ms = Some(latency_ms);
            att.error_code = Some(error_code.to_string());
            att.error_message = Some(error_message.to_string());
        }

        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.status = JobStatus::Dlq.as_str().to_string();
            job.dlq_reason_code = Some(reason_code.to_string());
            job.dlq_at = Some(now);
            job.locked_at = None;
            job.locked_by = None;
            job.lock_expires_at = None;
            job.updated_at = now;
        }

        Ok(())
    }

    async fn archive_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<u64> {
        let mut state = self.state.write().unwrap();

        let to_archive: Vec<Uuid> = state
            .jobs
            .values()
            .filter(|j| j.status == "succeeded" && j.updated_at < cutoff)
            .take(limit as usize)
            .map(|j| j.id)
            .collect();

        let count = to_archive.len() as u64;
        for id in to_archive {
            if let Some(job) = state.jobs.remove(&id) {
                state.archive.insert(id, job);
            }
        }

        Ok(count)
    }

    async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        limit: i64,
    ) -> anyhow::Result<(u64, u64)> {
        let mut state = self.state.write().unwrap();

        let archived_ids: Vec<Uuid> = state
            .archive
            .values()
            .filter(|j| j.updated_at < cutoff)
            .map(|j| j.id)
            .collect();

        let to_remove: Vec<Uuid> = state
            .attempts
            .values()
            .filter(|a| a.started_at < cutoff && archived_ids.contains(&a.job_id))
            .take(limit as usize)
            .map(|a| a.id)
            .collect();

        let count = to_remove.len() as u64;
        for aid in to_remove {
            state.attempts.remove(&aid);
        }

        Ok((count, 0))
    }

    async fn perform_maintenance(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        let mut state = self.state.write().unwrap();
        if let Some(job) = state.jobs.get_mut(&job_id) {
            if job.status == "running" && job.locked_by.as_deref() == Some(worker_id) {
                let now = Utc::now();
                job.lock_expires_at = Some(now + chrono::Duration::seconds(lease_seconds));
                job.updated_at = now;
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        let state = self.state.read().unwrap();
        let job = state
            .jobs
            .get(&job_id)
            .or_else(|| state.archive.get(&job_id))
            .cloned();
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
        let state = self.state.read().unwrap();
        let limit = limit.clamp(1, 500) as usize;

        let mut items: Vec<JobListItem> = state
            .jobs
            .values()
            .filter(|j| {
                if let Some(q) = queue {
                    if j.queue != q {
                        return false;
                    }
                }
                if let Some(st) = status {
                    if j.status != st {
                        return false;
                    }
                }
                true
            })
            .map(|j| JobListItem {
                id: j.id,
                queue: j.queue.clone(),
                job_type: j.job_type.clone(),
                status: j.status.clone(),
                run_at: j.run_at,
                priority: j.priority,
                max_attempts: j.max_attempts,
                last_error_code: None,
                last_error_message: None,
                dlq_reason_code: j.dlq_reason_code.clone(),
                created_at: j.created_at,
                updated_at: j.updated_at,
            })
            .collect();

        items.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        items.truncate(limit);
        Ok(items)
    }

    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        let mut state = self.state.write().unwrap();

        let src = state
            .jobs
            .get(&job_id)
            .or_else(|| state.archive.get(&job_id))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Job {job_id} not found"))?;

        let new_id = Uuid::new_v4();
        let now = Utc::now();
        let target_queue = override_queue.unwrap_or(&src.queue).to_string();
        let target_run_at = override_run_at.unwrap_or(now);

        let target_queue_clone = target_queue.clone();
        let new_job = Job {
            dataset_id: "default".to_string(),
            replay_of_job_id: Some(job_id),
            id: new_id,
            queue: target_queue,
            job_type: src.job_type,
            payload: src.payload,
            run_at: target_run_at,
            status: JobStatus::Queued.as_str().to_string(),
            priority: src.priority,
            max_attempts: src.max_attempts,
            locked_at: None,
            locked_by: None,
            lock_expires_at: None,
            dlq_reason_code: None,
            dlq_at: None,
            created_at: now,
            updated_at: now,
        };

        state.jobs.insert(new_id, new_job);
        drop(state);
        self.notify_queue(&target_queue_clone);
        Ok(new_id)
    }
}

#[async_trait]
impl StreamBackend for MemoryBackend {
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        let mut state = self.state.write().unwrap();
        let log = state.streams.entry(stream.to_string()).or_default();
        let sequence_no = (log.len() + 1) as i64;
        let now = Utc::now();

        let event_entity = Event {
            sequence_no,
            stream_name: stream.to_string(),
            event_type: event.event_type,
            payload_json: event.payload_json,
            created_at: now,
        };

        log.push(event_entity);
        drop(state);

        self.notify_stream(stream);
        Ok(sequence_no)
    }

    async fn subscribe_stream(
        &self,
        stream: &str,
        _consumer_group: &str,
        _last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        let rx = {
            let mut notifiers = self.stream_notifiers.write().unwrap();
            let tx = notifiers
                .entry(stream.to_string())
                .or_insert_with(|| tokio::sync::broadcast::channel(128).0);
            tx.subscribe()
        };

        let stream = BroadcastStream::new(rx).filter_map(|res| res.ok());
        Ok(Box::pin(stream))
    }

    async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        let mut state = self.state.write().unwrap();
        let key = (stream.to_string(), consumer_group.to_string());
        let now = Utc::now();

        let entry = state
            .stream_offsets
            .entry(key)
            .or_insert_with(|| ConsumerGroupStatus {
                consumer_group: consumer_group.to_string(),
                stream_name: stream.to_string(),
                last_acked_seq: 0,
                updated_at: now,
            });

        if seq > entry.last_acked_seq {
            entry.last_acked_seq = seq;
            entry.updated_at = now;
        }

        Ok(())
    }

    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let state = self.state.read().unwrap();
        let limit = limit.clamp(1, 1000) as usize;

        if let Some(log) = state.streams.get(stream) {
            let events: Vec<Event> = log
                .iter()
                .filter(|e| e.sequence_no > after_seq)
                .take(limit)
                .cloned()
                .collect();
            Ok(events)
        } else {
            Ok(Vec::new())
        }
    }

    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        let state = self.state.read().unwrap();
        let info: Vec<ConsumerGroupStatus> = state
            .stream_offsets
            .values()
            .filter(|cg| cg.stream_name == stream)
            .cloned()
            .collect();
        Ok(info)
    }
}
