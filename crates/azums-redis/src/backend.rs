use async_trait::async_trait;
use azums_core::{
    backend::{NotificationStream, StorageBackend, StreamBackend},
    model::{ConsumerGroupStatus, Event, Job, JobListItem, JobStatus, NewEvent, NewJob},
};
use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use uuid::Uuid;

/// Production-grade Redis implementation of [`StorageBackend`] and [`StreamBackend`].
#[derive(Clone)]
pub struct RedisBackend {
    client: redis::Client,
    conn_mgr: ConnectionManager,
    notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
    stream_notifiers: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<()>>>>,
}

impl RedisBackend {
    /// Creates a new `RedisBackend` from a Redis connection URL (e.g., `"redis://127.0.0.1:6379"`).
    pub async fn new(redis_url: impl AsRef<str>) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url.as_ref())?;
        let conn_mgr = ConnectionManager::new(client.clone()).await?;

        Ok(Self {
            client,
            conn_mgr,
            notifiers: Arc::new(RwLock::new(HashMap::new())),
            stream_notifiers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Returns reference to the underlying Redis `Client`.
    pub fn client(&self) -> &redis::Client {
        &self.client
    }

    fn notify_queue_local(&self, queue: &str) {
        let notifiers = self.notifiers.read().unwrap();
        if let Some(tx) = notifiers.get(queue) {
            let _ = tx.send(());
        }
    }

    fn notify_stream_local(&self, stream: &str) {
        let notifiers = self.stream_notifiers.read().unwrap();
        if let Some(tx) = notifiers.get(stream) {
            let _ = tx.send(());
        }
    }
}

#[async_trait]
impl StorageBackend for RedisBackend {
    fn capabilities(&self) -> azums_core::BackendCapabilities {
        azums_core::BackendCapabilities::redis()
    }

    fn as_stream(&self) -> Option<&dyn StreamBackend> {
        Some(self)
    }

    async fn run_migrations(&self) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(())
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let res: String = redis::cmd("PING").query_async(&mut conn).await?;
        if res == "PONG" || !res.is_empty() {
            Ok(())
        } else {
            anyhow::bail!("Redis health check failed")
        }
    }

    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid> {
        let mut conn = self.conn_mgr.clone();
        let job_id = Uuid::new_v4();
        let now = Utc::now();
        let idempotency_key = job.idempotency_key.clone();

        if let Some(key) = &idempotency_key {
            let claimed: bool = conn
                .hset_nx("azums:idempotency", key, job_id.to_string())
                .await?;
            if !claimed {
                let existing: String = conn.hget("azums:idempotency", key).await?;
                return Ok(Uuid::parse_str(&existing)?);
            }
        }

        let job_entity = Job {
            dataset_id: "default".to_string(),
            replay_of_job_id: None,
            idempotency_key,
            id: job_id,
            queue: job.queue.clone(),
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

        let json_str = serde_json::to_string(&job_entity)?;

        let _: () = conn
            .hset("azums:jobs", job_id.to_string(), json_str)
            .await?;
        let queue_key = format!("azums:queue:{}", job.queue);
        let _: () = conn.rpush(queue_key, job_id.to_string()).await?;

        let notify_channel = format!("azums:notify:{}", job.queue);
        let _: () = conn.publish(notify_channel, "1").await?;

        self.notify_queue_local(&job.queue);

        Ok(job_id)
    }

    async fn subscribe(&self, queue: &str) -> anyhow::Result<NotificationStream> {
        use tokio_stream::wrappers::BroadcastStream;
        use tokio_stream::StreamExt;

        let channel = format!("azums:notify:{queue}");
        let client_clone = self.client.clone();
        let tx_clone = {
            let mut notifiers = self.notifiers.write().unwrap();
            notifiers
                .entry(queue.to_string())
                .or_insert_with(|| tokio::sync::broadcast::channel(128).0)
                .clone()
        };

        let tx_spawn = tx_clone.clone();
        // Spawn a dedicated, unpooled PubSub socket listener
        tokio::spawn(async move {
            if let Ok(mut pubsub) = client_clone.get_async_pubsub().await {
                if pubsub.subscribe(&channel).await.is_ok() {
                    let mut stream = pubsub.into_on_message();
                    while stream.next().await.is_some() {
                        let _ = tx_spawn.send(());
                    }
                }
            }
        });

        let rx = tx_clone.subscribe();
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
        let mut conn = self.conn_mgr.clone();
        let queue_key = format!("azums:queue:{}", queue);
        let processing_key = format!("azums:processing:{}:{}", queue, worker_id);
        let now = Utc::now();
        let lock_expires_at = now + chrono::Duration::seconds(lease_seconds);

        let mut leased = Vec::new();
        let batch_size = batch_size.clamp(1, 100) as usize;

        for _ in 0..batch_size {
            let job_id_str: Option<String> = redis::cmd("LMOVE")
                .arg(&queue_key)
                .arg(&processing_key)
                .arg("LEFT")
                .arg("RIGHT")
                .query_async(&mut conn)
                .await
                .ok();

            let job_id_str = match job_id_str {
                Some(id) if !id.is_empty() => id,
                _ => break,
            };

            let json_str: Option<String> = conn.hget("azums:jobs", &job_id_str).await?;
            if let Some(json) = json_str {
                if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                    if job.run_at > now {
                        // Put back at head if run_at is in the future
                        let _: () = conn.lpush(&queue_key, &job_id_str).await?;
                        let _: () = conn.lrem(&processing_key, 1, &job_id_str).await?;
                        continue;
                    }

                    job.status = JobStatus::Running.as_str().to_string();
                    job.locked_at = Some(now);
                    job.locked_by = Some(worker_id.to_string());
                    job.lock_expires_at = Some(lock_expires_at);
                    job.updated_at = now;

                    let updated_json = serde_json::to_string(&job)?;
                    let _: () = conn.hset("azums:jobs", &job_id_str, updated_json).await?;
                    leased.push(job);
                }
            }
        }

        Ok(leased)
    }

    async fn lease_jobs_batch_with_ordering(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
        ordering: azums_core::QueueOrdering,
    ) -> anyhow::Result<Vec<Job>> {
        let _ = ordering; // Redis RPUSH (enqueue) and LMOVE LEFT RIGHT (dequeue) natively preserve strict FIFO insertion order
        self.lease_jobs_batch(queue, worker_id, lease_seconds, batch_size)
            .await
    }

    async fn reap_expired_locks(&self) -> anyhow::Result<u64> {
        let mut conn = self.conn_mgr.clone();
        let keys: Vec<String> = conn.keys("azums:processing:*").await.unwrap_or_default();
        let now = Utc::now();
        let mut reaped = 0u64;

        for proc_key in keys {
            let parts: Vec<&str> = proc_key.split(':').collect();
            if parts.len() < 4 {
                continue;
            }
            let queue = parts[2];
            let queue_key = format!("azums:queue:{}", queue);

            let job_ids: Vec<String> = conn.lrange(&proc_key, 0, -1).await.unwrap_or_default();
            for jid in job_ids {
                if let Ok(Some(json)) = conn.hget::<_, _, Option<String>>("azums:jobs", &jid).await
                {
                    if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                        if let Some(exp) = job.lock_expires_at {
                            if exp <= now {
                                job.status = JobStatus::Queued.as_str().to_string();
                                job.locked_at = None;
                                job.locked_by = None;
                                job.lock_expires_at = None;
                                job.updated_at = now;

                                if let Ok(updated_json) = serde_json::to_string(&job) {
                                    let _: () = conn
                                        .hset("azums:jobs", &jid, updated_json)
                                        .await
                                        .unwrap_or(());
                                    let _: () = conn.lrem(&proc_key, 1, &jid).await.unwrap_or(());
                                    let _: () = conn.rpush(&queue_key, &jid).await.unwrap_or(());
                                    reaped += 1;
                                }
                            }
                        }
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
        _worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>> {
        let mut conn = self.conn_mgr.clone();
        let mut results = Vec::with_capacity(job_ids.len());

        for &job_id in job_ids {
            let job_id_str = job_id.to_string();
            let json_str: Option<String> = conn.hget("azums:jobs", &job_id_str).await?;
            let job = json_str
                .as_deref()
                .and_then(|json| serde_json::from_str::<Job>(json).ok())
                .ok_or_else(|| anyhow::anyhow!("job {job_id} not found"))?;
            if job.status != "running" || job.locked_by.as_deref() != Some(_worker_id) {
                anyhow::bail!(
                    "cannot start attempt for job {job_id}: expected running lease held by {_worker_id}"
                );
            }

            let attempt_id = Uuid::new_v4();
            let attempts_key = format!("azums:attempts:{}", job_id);
            let attempt_no: i32 = conn.incr(&attempts_key, 1).await?;
            results.push((job_id, attempt_id, attempt_no));
        }

        Ok(results)
    }

    async fn mark_succeeded(
        &self,
        job_id: Uuid,
        _attempt_id: Uuid,
        worker_id: &str,
        _latency_ms: i32,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let job_id_str = job_id.to_string();

        if let Ok(Some(json)) = conn
            .hget::<_, _, Option<String>>("azums:jobs", &job_id_str)
            .await
        {
            if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                if job.status != "running" || job.locked_by.as_deref() != Some(worker_id) {
                    anyhow::bail!(
                        "illegal job state transition to completed for job {job_id}: expected running lease held by {worker_id}"
                    );
                }

                job.status = JobStatus::Succeeded.as_str().to_string();
                job.locked_at = None;
                job.locked_by = None;
                job.lock_expires_at = None;
                job.updated_at = Utc::now();
                let updated_json = serde_json::to_string(&job)?;
                let _: () = conn.hset("azums:jobs", &job_id_str, updated_json).await?;

                let proc_key = format!("azums:processing:{}:{}", job.queue, worker_id);
                let _: () = conn.lrem(proc_key, 1, &job_id_str).await?;
            }
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

    async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        _attempt_id: Uuid,
        worker_id: &str,
        _latency_ms: i32,
        next_run_at: DateTime<Utc>,
        _error_code: &str,
        _error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let job_id_str = job_id.to_string();

        if let Ok(Some(json)) = conn
            .hget::<_, _, Option<String>>("azums:jobs", &job_id_str)
            .await
        {
            if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                if job.status != "running" || job.locked_by.as_deref() != Some(worker_id) {
                    anyhow::bail!(
                        "illegal job state transition to retry_wait for job {job_id}: expected running lease held by {worker_id}"
                    );
                }

                job.status = JobStatus::Queued.as_str().to_string();
                job.run_at = next_run_at;
                job.locked_at = None;
                job.locked_by = None;
                job.lock_expires_at = None;
                job.updated_at = Utc::now();

                let updated_json = serde_json::to_string(&job)?;
                let _: () = conn.hset("azums:jobs", &job_id_str, updated_json).await?;

                let proc_key = format!("azums:processing:{}:{}", job.queue, worker_id);
                let _: () = conn.lrem(proc_key, 1, &job_id_str).await?;

                let queue_key = format!("azums:queue:{}", job.queue);
                let _: () = conn.rpush(queue_key, &job_id_str).await?;
            }
        }

        Ok(())
    }

    async fn mark_dlq(
        &self,
        job_id: Uuid,
        _attempt_id: Uuid,
        worker_id: &str,
        _latency_ms: i32,
        reason_code: &str,
        _error_code: &str,
        _error_message: &str,
        _attempt_no: i32,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let job_id_str = job_id.to_string();

        if let Ok(Some(json)) = conn
            .hget::<_, _, Option<String>>("azums:jobs", &job_id_str)
            .await
        {
            if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                if job.status != "running" || job.locked_by.as_deref() != Some(worker_id) {
                    anyhow::bail!(
                        "illegal job state transition to dlq for job {job_id}: expected running lease held by {worker_id}"
                    );
                }

                job.status = JobStatus::Dlq.as_str().to_string();
                job.dlq_reason_code = Some(reason_code.to_string());
                job.dlq_at = Some(Utc::now());
                job.updated_at = Utc::now();

                let updated_json = serde_json::to_string(&job)?;
                let _: () = conn.hset("azums:jobs", &job_id_str, updated_json).await?;

                let proc_key = format!("azums:processing:{}:{}", job.queue, worker_id);
                let _: () = conn.lrem(proc_key, 1, &job_id_str).await?;
            }
        }

        Ok(())
    }

    async fn archive_succeeded_older_than(
        &self,
        _cutoff: DateTime<Utc>,
        _limit: i64,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn delete_history_for_succeeded_older_than(
        &self,
        _cutoff: DateTime<Utc>,
        _limit: i64,
    ) -> anyhow::Result<(u64, u64)> {
        Ok((0, 0))
    }

    async fn perform_maintenance(&self) -> anyhow::Result<()> {
        let _ = self.reap_expired_locks().await;
        Ok(())
    }

    async fn extend_lease(
        &self,
        job_id: Uuid,
        worker_id: &str,
        lease_seconds: i64,
    ) -> anyhow::Result<bool> {
        let mut conn = self.conn_mgr.clone();
        let job_key = job_id.to_string();
        let json_str: Option<String> = conn.hget("azums:jobs", &job_key).await?;

        if let Some(json) = json_str {
            if let Ok(mut job) = serde_json::from_str::<Job>(&json) {
                if job.status == "running" && job.locked_by.as_deref() == Some(worker_id) {
                    let now = Utc::now();
                    job.lock_expires_at = Some(now + chrono::Duration::seconds(lease_seconds));
                    job.updated_at = now;

                    let updated_json = serde_json::to_string(&job)?;
                    let _: () = conn.hset("azums:jobs", &job_key, updated_json).await?;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        let mut conn = self.conn_mgr.clone();
        let job_id_str = job_id.to_string();
        let json_str: Option<String> = conn.hget("azums:jobs", &job_id_str).await?;

        let mut job = match json_str {
            Some(json) => serde_json::from_str::<Job>(&json)?,
            None => anyhow::bail!("job {job_id} not found"),
        };

        match job.status.as_str() {
            "queued" => {}
            "running" => {
                let Some(worker_id) = worker_id else {
                    anyhow::bail!(
                        "cannot cancel running job {job_id}: worker identity is required"
                    );
                };
                if job.locked_by.as_deref() != Some(worker_id) {
                    anyhow::bail!(
                        "illegal job state transition to cancelled for job {job_id}: expected running lease held by {worker_id}"
                    );
                }

                let proc_key = format!("azums:processing:{}:{}", job.queue, worker_id);
                let _: () = conn.lrem(proc_key, 1, &job_id_str).await?;
            }
            "succeeded" | "dlq" | "canceled" => {
                anyhow::bail!("cannot cancel terminal job {job_id}: status={}", job.status);
            }
            other => anyhow::bail!("cannot cancel job {job_id}: invalid status={other}"),
        }

        job.status = JobStatus::Cancelled.as_str().to_string();
        job.locked_at = None;
        job.locked_by = None;
        job.lock_expires_at = None;
        job.updated_at = Utc::now();

        let updated_json = serde_json::to_string(&job)?;
        let _: () = conn.hset("azums:jobs", &job_id_str, updated_json).await?;

        Ok(())
    }

    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>> {
        let mut conn = self.conn_mgr.clone();
        let json_str: Option<String> = conn.hget("azums:jobs", job_id.to_string()).await?;
        match json_str {
            Some(json) => Ok(serde_json::from_str(&json).ok()),
            None => Ok(None),
        }
    }

    async fn list_jobs(
        &self,
        queue: Option<&str>,
        status: Option<&str>,
        limit: i64,
        _cursor_created_at: Option<DateTime<Utc>>,
        _cursor_id: Option<Uuid>,
    ) -> anyhow::Result<Vec<JobListItem>> {
        let mut conn = self.conn_mgr.clone();
        let map: HashMap<String, String> = conn.hgetall("azums:jobs").await.unwrap_or_default();

        let mut items = Vec::new();
        for json in map.values() {
            if let Ok(job) = serde_json::from_str::<Job>(json) {
                if let Some(q) = queue {
                    if job.queue != q {
                        continue;
                    }
                }
                if let Some(st) = status {
                    if job.status != st {
                        continue;
                    }
                }

                items.push(JobListItem {
                    id: job.id,
                    idempotency_key: job.idempotency_key,
                    queue: job.queue,
                    job_type: job.job_type,
                    status: job.status,
                    run_at: job.run_at,
                    priority: job.priority,
                    max_attempts: job.max_attempts,
                    last_error_code: None,
                    last_error_message: None,
                    dlq_reason_code: job.dlq_reason_code,
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                });
            }
        }

        items.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        items.truncate(limit.clamp(1, 500) as usize);
        Ok(items)
    }

    async fn replay_job(
        &self,
        job_id: Uuid,
        override_queue: Option<&str>,
        override_run_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<Uuid> {
        let mut conn = self.conn_mgr.clone();
        let json_str: Option<String> = conn.hget("azums:jobs", job_id.to_string()).await?;

        let src = match json_str {
            Some(json) => serde_json::from_str::<Job>(&json)?,
            None => anyhow::bail!("Job {} not found", job_id),
        };

        let new_id = Uuid::new_v4();
        let target_queue = override_queue.unwrap_or(&src.queue).to_string();
        let target_run_at = override_run_at.unwrap_or_else(Utc::now);
        let now = Utc::now();

        let new_job = Job {
            dataset_id: "default".to_string(),
            replay_of_job_id: Some(job_id),
            idempotency_key: None,
            id: new_id,
            queue: target_queue.clone(),
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

        let updated_json = serde_json::to_string(&new_job)?;
        let _: () = conn
            .hset("azums:jobs", new_id.to_string(), updated_json)
            .await?;

        let queue_key = format!("azums:queue:{}", target_queue);
        let _: () = conn.rpush(queue_key, new_id.to_string()).await?;

        self.notify_queue_local(&target_queue);
        Ok(new_id)
    }
}

#[async_trait]
impl StreamBackend for RedisBackend {
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        let mut conn = self.conn_mgr.clone();
        let seq_key = format!("azums:stream_seq:{}", stream);
        let sequence_no: i64 = conn.incr(&seq_key, 1).await?;
        let now = Utc::now();

        let event_entity = Event {
            sequence_no,
            stream_name: stream.to_string(),
            event_type: event.event_type,
            payload_json: event.payload_json,
            created_at: now,
        };

        let json_str = serde_json::to_string(&event_entity)?;
        let stream_key = format!("azums:stream_events:{}", stream);
        let _: () = conn.rpush(stream_key, json_str).await?;

        let notify_channel = format!("azums:stream_notify:{}", stream);
        let _: () = conn.publish(notify_channel, "1").await?;

        self.notify_stream_local(stream);
        Ok(sequence_no)
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
        let mut conn = self.conn_mgr.clone();
        let key = format!("azums:stream_offsets:{}", stream);
        let now = Utc::now();

        let current_offset: Option<i64> = conn.hget(&key, consumer_group).await.ok();
        let new_seq = match current_offset {
            Some(existing) => existing.max(seq),
            None => seq,
        };

        let _: () = conn.hset(&key, consumer_group, new_seq).await?;
        let timestamp_key = format!("azums:stream_offsets_time:{}", stream);
        let _: () = conn
            .hset(timestamp_key, consumer_group, now.to_rfc3339())
            .await?;

        Ok(())
    }

    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let mut conn = self.conn_mgr.clone();
        let stream_key = format!("azums:stream_events:{}", stream);
        let raw_events: Vec<String> = conn.lrange(&stream_key, 0, -1).await.unwrap_or_default();

        let limit = limit.clamp(1, 1000) as usize;
        let mut result = Vec::new();

        for raw in raw_events {
            if let Ok(event) = serde_json::from_str::<Event>(&raw) {
                if event.sequence_no > after_seq {
                    result.push(event);
                    if result.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(result)
    }

    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        let mut conn = self.conn_mgr.clone();
        let key = format!("azums:stream_offsets:{}", stream);
        let map: HashMap<String, i64> = conn.hgetall(&key).await.unwrap_or_default();

        let time_key = format!("azums:stream_offsets_time:{}", stream);
        let time_map: HashMap<String, String> = conn.hgetall(time_key).await.unwrap_or_default();

        let mut result = Vec::new();
        for (group, last_acked_seq) in map {
            let updated_at = time_map
                .get(&group)
                .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            result.push(ConsumerGroupStatus {
                consumer_group: group,
                stream_name: stream.to_string(),
                last_acked_seq,
                updated_at,
            });
        }

        Ok(result)
    }
}
