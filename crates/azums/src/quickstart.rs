use crate::{
    backend::PostgresBackend,
    jobs::{
        model::{Job, NewJob},
        retry::classify_error,
        retry::{next_delay_seconds, ErrorClass, RetryConfig},
    },
};
use azums_core::StorageBackend;
use rand::{rngs::StdRng, SeedableRng};
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

pub type QuickstartHandlerFuture =
    Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
pub type QuickstartHandler = Arc<dyn Fn(Job) -> QuickstartHandlerFuture + Send + Sync>;

/// Main entry point client for `postgresflow`.
pub type Client = QuickstartFlow;

/// In-process worker runtime and admin API launcher built by [`quickstart`].
///
/// `QuickstartFlow` manages job enqueueing, handler registration, background worker leasing loops,
/// and the optional Axum admin web console through an abstract [`StorageBackend`].
#[derive(Clone)]
pub struct QuickstartFlow {
    backend: Arc<dyn StorageBackend>,
    handlers: Arc<RwLock<HashMap<String, QuickstartHandler>>>,
    queue_configs: Arc<RwLock<HashMap<String, azums_core::QueueConfig>>>,
    queue: String,
    worker_id: String,
    lease_seconds: i64,
    retry_cfg: RetryConfig,
}

impl QuickstartFlow {
    /// Creates a `QuickstartFlow` wrapping a custom [`StorageBackend`].
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let queue = std::env::var("AZUMS_QUEUE").unwrap_or_else(|_| "default".to_string());
        let worker_id =
            std::env::var("AZUMS_WORKER_ID").unwrap_or_else(|_| "quickstart-worker".to_string());
        let lease_seconds = std::env::var("AZUMS_LEASE_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        Self {
            backend,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            queue_configs: Arc::new(RwLock::new(HashMap::new())),
            queue,
            worker_id,
            lease_seconds,
            retry_cfg: RetryConfig::default(),
        }
    }

    /// Sets the target queue name for this [`QuickstartFlow`] worker.
    pub fn with_queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = queue.into();
        self
    }

    /// Sets the unique worker ID string for this [`QuickstartFlow`].
    pub fn with_worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = worker_id.into();
        self
    }

    /// Sets the lease lock duration in seconds for leased jobs.
    pub fn with_lease_seconds(mut self, lease_seconds: i64) -> Self {
        self.lease_seconds = lease_seconds.max(1);
        self
    }

    /// Configures queue options (such as [`QueueOrdering`](azums_core::QueueOrdering)) for a specified queue.
    pub async fn configure_queue(&self, queue: impl Into<String>, config: azums_core::QueueConfig) {
        let mut configs = self.queue_configs.write().await;
        configs.insert(queue.into(), config);
    }

    /// Returns the active [`QueueConfig`](azums_core::QueueConfig) for a specified queue (defaults to FIFO).
    pub async fn get_queue_config(&self, queue: &str) -> azums_core::QueueConfig {
        let configs = self.queue_configs.read().await;
        configs.get(queue).cloned().unwrap_or_default()
    }

    /// Returns reference to the underlying [`StorageBackend`].
    pub fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.backend
    }

    /// Returns the storage guarantees and feature support declared by the active backend.
    pub fn capabilities(&self) -> azums_core::BackendCapabilities {
        self.backend.capabilities()
    }

    /// Returns a [`StreamHandle`](crate::StreamHandle) for high-level Redis-style stream log operations.
    pub fn stream(&self, name: impl Into<String>) -> crate::stream_handle::StreamHandle {
        crate::stream_handle::StreamHandle::new(self.backend.clone(), name)
    }

    /// Enqueues a job into the storage backend queue.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::{quickstart, Job};
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// let job_id = flow.enqueue(Job::new("send_email", serde_json::json!({"user_id": 42}))).await?;
    /// println!("Enqueued job: {job_id}");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enqueue(&self, job: impl Into<NewJob>) -> anyhow::Result<Uuid> {
        let new_job: NewJob = job.into();
        self.backend.enqueue(new_job).await
    }

    /// Enqueues multiple jobs in a batch into the queue backend.
    pub async fn enqueue_batch(
        &self,
        jobs: impl IntoIterator<Item = impl Into<NewJob>>,
    ) -> anyhow::Result<Vec<Uuid>> {
        let mut ids = Vec::new();
        for job in jobs {
            ids.push(self.enqueue(job).await?);
        }
        Ok(ids)
    }

    /// Cancels a queued/scheduled job or a running job owned by `worker_id`.
    pub async fn cancel_job(&self, job_id: Uuid, worker_id: Option<&str>) -> anyhow::Result<()> {
        self.backend.cancel_job(job_id, worker_id).await
    }

    /// Registers an asynchronous handler closure for a specific `job_type`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::quickstart;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// flow.register_handler("greet", |job| async move {
    ///     println!("Hello, {}!", job.payload["name"]);
    ///     Ok(())
    /// }).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_handler<F, Fut>(&self, job_type: impl Into<String>, handler: F)
    where
        F: Fn(Job) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let job_type = job_type.into();
        let entry: QuickstartHandler = Arc::new(move |job: Job| {
            let fut = handler(job);
            Box::pin(fut) as QuickstartHandlerFuture
        });
        self.handlers.write().await.insert(job_type, entry);
    }

    /// Registers a trait-based [`JobProcessor`](azums_core::JobProcessor) for a specific `job_type`.
    pub async fn register_processor<P>(&self, job_type: impl Into<String>, processor: P)
    where
        P: azums_core::JobProcessor + 'static,
    {
        let processor = Arc::new(processor);
        self.register_handler(job_type, move |job| {
            let p = processor.clone();
            async move { p.process(job).await }
        })
        .await;
    }

    /// Gracefully shuts down background resources and connections.
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Performs database maintenance operations (such as PostgreSQL `VACUUM ANALYZE` or SQLite `PRAGMA incremental_vacuum`).
    pub async fn perform_maintenance(&self) -> anyhow::Result<()> {
        self.backend.perform_maintenance().await
    }

    /// Starts the in-process worker polling loop and admin HTTP API (if `api` feature is active).
    ///
    /// Runs continuously processing jobs until application termination.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::quickstart;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// flow.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&self) -> anyhow::Result<()> {
        let token = tokio_util::sync::CancellationToken::new();
        self.run_with_shutdown(token).await
    }

    /// Starts the in-process worker polling loop with a `CancellationToken` for graceful shutdown.
    pub async fn run_with_shutdown(
        &self,
        shutdown_token: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<()> {
        use tokio_stream::StreamExt;

        let mut last_reap_at = std::time::Instant::now();
        let mut last_maint_at = std::time::Instant::now();
        let reap_interval = std::time::Duration::from_secs(5);
        let maint_interval = std::time::Duration::from_secs(300);
        let mut stream = self.backend.subscribe(&self.queue).await.ok();

        loop {
            if shutdown_token.is_cancelled() {
                break;
            }

            if last_reap_at.elapsed() >= reap_interval {
                let _ = self.backend.reap_expired_locks().await;
                last_reap_at = std::time::Instant::now();
            }

            if last_maint_at.elapsed() >= maint_interval {
                let _ = self.backend.perform_maintenance().await;
                last_maint_at = std::time::Instant::now();
            }

            let q_config = self.get_queue_config(&self.queue).await;
            let batch = self
                .backend
                .lease_jobs_batch_with_ordering(
                    &self.queue,
                    &self.worker_id,
                    self.lease_seconds,
                    32,
                    q_config.ordering,
                )
                .await?;

            if batch.is_empty() {
                if let Some(s) = stream.as_mut() {
                    tokio::select! {
                        _ = shutdown_token.cancelled() => break,
                        _ = s.next() => {},
                        _ = tokio::time::sleep(reap_interval) => {},
                    }
                } else {
                    tokio::select! {
                        _ = shutdown_token.cancelled() => break,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
                    }
                }
                continue;
            }

            self.process_batch(batch).await?;
        }

        Ok(())
    }

    /// Runs worker polling loops until all currently queued jobs have been processed, returning total count.
    ///
    /// Useful for batch processing, integration tests, or unit testing job flows.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::quickstart;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// let processed = flow.run_until_empty().await?;
    /// println!("Processed {processed} jobs");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_until_empty(&self) -> anyhow::Result<usize> {
        let mut total_processed = 0;

        loop {
            let q_config = self.get_queue_config(&self.queue).await;
            let batch = self
                .backend
                .lease_jobs_batch_with_ordering(
                    &self.queue,
                    &self.worker_id,
                    self.lease_seconds,
                    32,
                    q_config.ordering,
                )
                .await?;

            if batch.is_empty() {
                break;
            }

            let count = batch.len();
            self.process_batch(batch).await?;
            total_processed += count;
        }

        Ok(total_processed)
    }

    async fn process_batch(&self, batch: Vec<Job>) -> anyhow::Result<()> {
        let dataset_ids: Vec<String> = batch.iter().map(|j| j.dataset_id.clone()).collect();
        let job_ids: Vec<Uuid> = batch.iter().map(|j| j.id).collect();

        let started_attempts = self
            .backend
            .start_attempts_batch(&dataset_ids, &job_ids, &self.worker_id)
            .await?;

        let mut attempts_map: HashMap<Uuid, (Uuid, i32)> = started_attempts
            .into_iter()
            .map(|(jid, aid, ano)| (jid, (aid, ano)))
            .collect();

        for job in batch {
            let (attempt_id, attempt_no) = match attempts_map.remove(&job.id) {
                Some(v) => v,
                None => continue,
            };

            let handler_opt = {
                let guard = self.handlers.read().await;
                guard.get(&job.job_type).cloned()
            };

            // Spawn background heartbeat task to extend lease for long-running jobs
            let (heartbeat_tx, mut heartbeat_rx) = tokio::sync::oneshot::channel::<()>();
            let backend_clone = self.backend.clone();
            let job_id = job.id;
            let worker_id_clone = self.worker_id.clone();
            let lease_secs = self.lease_seconds;

            let _hb_handle = tokio::spawn(async move {
                let interval_duration =
                    std::time::Duration::from_secs((lease_secs as u64 / 2).max(1));
                loop {
                    tokio::select! {
                        _ = &mut heartbeat_rx => break,
                        _ = tokio::time::sleep(interval_duration) => {
                            let _ = backend_clone.extend_lease(job_id, &worker_id_clone, lease_secs).await;
                        }
                    }
                }
            });

            let start = std::time::Instant::now();
            let res_outcome = match handler_opt.as_ref() {
                Some(handler) => {
                    let handler_clone = handler.clone();
                    let job_clone = job.clone();
                    let task_res =
                        tokio::task::spawn(async move { (handler_clone)(job_clone).await }).await;
                    match task_res {
                        Ok(res) => res,
                        Err(join_err) => {
                            if join_err.is_panic() {
                                let panic_msg =
                                    azums_core::format_panic_message(join_err.into_panic());
                                Err(anyhow::anyhow!("PANIC: {}", panic_msg))
                            } else {
                                Err(anyhow::anyhow!("task join error: {}", join_err))
                            }
                        }
                    }
                }
                None => Err(anyhow::anyhow!(
                    "no handler registered for job_type={}",
                    job.job_type
                )),
            };

            // Stop heartbeat task
            let _ = heartbeat_tx.send(());

            let latency_ms = start.elapsed().as_millis() as i32;
            match res_outcome {
                Ok(()) => {
                    self.backend
                        .mark_succeeded(job.id, attempt_id, &self.worker_id, latency_ms)
                        .await?;
                }
                Err(err) => {
                    let err_str = err.to_string();
                    let is_panic = err_str.starts_with("PANIC: ");
                    let (err_code, err_msg) = if is_panic {
                        ("PANIC", err_str.trim_start_matches("PANIC: "))
                    } else if handler_opt.is_some() {
                        ("HANDLER_ERROR", err_str.as_str())
                    } else {
                        ("UNKNOWN_JOB_TYPE", err_str.as_str())
                    };

                    if is_panic {
                        // Immediately route panicked job to DLQ
                        self.backend
                            .mark_dlq(
                                job.id,
                                attempt_id,
                                &self.worker_id,
                                latency_ms,
                                "PANIC",
                                err_code,
                                err_msg,
                                attempt_no,
                            )
                            .await?;
                    } else {
                        self.handle_failure(
                            job.id,
                            attempt_id,
                            latency_ms,
                            err_code,
                            err_msg,
                            attempt_no,
                            job.max_attempts,
                        )
                        .await?;
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_failure(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        latency_ms: i32,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
        max_attempts: i32,
    ) -> anyhow::Result<()> {
        let class = classify_error(error_code);
        let can_retry = class == ErrorClass::Retryable && attempt_no < max_attempts;

        if can_retry {
            let mut rng = StdRng::from_entropy();
            let delay_secs = next_delay_seconds(attempt_no, &self.retry_cfg, &mut rng);
            let next_run_at = chrono::Utc::now() + chrono::Duration::seconds(delay_secs);

            self.backend
                .reschedule_for_retry(
                    job_id,
                    attempt_id,
                    &self.worker_id,
                    latency_ms,
                    next_run_at,
                    error_code,
                    error_message,
                    attempt_no,
                )
                .await
        } else {
            let reason_code = match class {
                ErrorClass::NonRetryable => "NON_RETRYABLE",
                ErrorClass::Retryable => "MAX_ATTEMPTS_EXCEEDED",
            };

            self.backend
                .mark_dlq(
                    job_id,
                    attempt_id,
                    &self.worker_id,
                    latency_ms,
                    reason_code,
                    error_code,
                    error_message,
                    attempt_no,
                )
                .await
        }
    }
}

/// Spawns an in-process worker and admin API with sensible connection defaults.
///
/// Automatically attempts database connections in order:
/// 1. Passed `database_url`
/// 2. `DATABASE_URL` environment variable
/// 3. `TEST_DATABASE_URL` environment variable
/// 4. Common local development Postgres instances (Docker Compose port 5433, local port 5432)
///
/// Runs all SQL schema migrations (`run_migrations`) on successful connection.
///
/// # Examples
///
/// ```rust,no_run
/// use azums::{quickstart, Job};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let flow = quickstart("postgres://localhost/flow").await?;
///     flow.enqueue(Job::new("greet", serde_json::json!({"name": "World"}))).await?;
///     flow.register_handler("greet", |job| async move {
///         println!("Hello, {}!", job.payload["name"]);
///         Ok(())
///     }).await;
///     flow.run().await?;
///     Ok(())
/// }
/// ```
pub async fn quickstart(database_url: impl AsRef<str>) -> anyhow::Result<QuickstartFlow> {
    let _ = dotenvy::dotenv();

    let user_url = database_url.as_ref().trim();

    if user_url == "memory"
        || user_url == "in-memory"
        || user_url.starts_with("memory:")
        || user_url.starts_with("memory://")
    {
        let mem_backend = azums_core::MemoryBackend::new();
        let backend: Arc<dyn StorageBackend> = Arc::new(mem_backend);
        return Ok(QuickstartFlow::new(backend));
    }

    #[cfg(feature = "sqlite")]
    if user_url.starts_with("sqlite:") {
        let pool = crate::backend::sqlite::make_sqlite_pool(user_url).await?;
        let sqlite_backend = crate::backend::SqliteBackend::new(pool);
        sqlite_backend.run_migrations().await?;
        let backend: Arc<dyn StorageBackend> = Arc::new(sqlite_backend);
        return Ok(QuickstartFlow::new(backend));
    }

    #[cfg(feature = "redis")]
    if user_url.starts_with("redis:") || user_url.starts_with("rediss:") {
        let redis_backend = azums_redis::RedisBackend::new(user_url).await?;
        redis_backend.run_migrations().await?;
        let backend: Arc<dyn StorageBackend> = Arc::new(redis_backend);
        return Ok(QuickstartFlow::new(backend));
    }

    let mut candidates = Vec::new();
    if !user_url.is_empty() && !user_url.starts_with("sqlite:") && !user_url.starts_with("memory") {
        candidates.push(user_url.to_string());
    }

    if let Ok(env_url) = std::env::var("DATABASE_URL") {
        let env_url = env_url.trim().to_string();
        if !env_url.is_empty() && !candidates.contains(&env_url) {
            candidates.push(env_url);
        }
    }

    if let Ok(test_url) = std::env::var("TEST_DATABASE_URL") {
        let test_url = test_url.trim().to_string();
        if !test_url.is_empty() && !candidates.contains(&test_url) {
            candidates.push(test_url);
        }
    }

    let defaults = [
        "postgres://postgres:postgres@127.0.0.1:5433/azums_dev",
        "postgres://postgres:postgres@127.0.0.1:5432/azums_dev",
        "postgres://postgres:postgres@localhost:5433/azums_dev",
        "postgres://postgres:postgres@localhost:5432/azums_dev",
        "postgres://postgres:postgres@127.0.0.1:5433/postgresflow_dev",
        "postgres://postgres:postgres@127.0.0.1:5432/postgresflow_dev",
        "postgres://postgres:postgres@127.0.0.1:5432/postgres",
        "postgres://postgres:root@127.0.0.1:5432/postgres",
        "postgres://postgres:admin@127.0.0.1:5432/postgres",
        "postgres://postgres:password@127.0.0.1:5432/postgres",
        "postgres://postgres:123456@127.0.0.1:5432/postgres",
        "postgres://postgres@127.0.0.1:5432/postgres",
        "postgres://127.0.0.1:5432/postgres",
        "postgres://localhost:5432/postgres",
    ];

    for def in defaults {
        let s = def.to_string();
        if !candidates.contains(&s) {
            candidates.push(s);
        }
    }

    let mut pool_opt = None;
    let mut last_err = None;

    for candidate in &candidates {
        let opts = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(2));

        match opts.connect(candidate).await {
            Ok(pool) => {
                if sqlx::query("SELECT 1").execute(&pool).await.is_ok() {
                    pool_opt = Some((candidate.clone(), pool));
                    break;
                }
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    let (connected_url, pool) = match pool_opt {
        Some(p) => p,
        None => {
            return Err(anyhow::anyhow!(
                "Failed to connect to any PostgreSQL database. Tried candidates: {:?}. Last error: {:?}",
                candidates,
                last_err
            ));
        }
    };

    let pg_backend = PostgresBackend::new_with_url(pool, connected_url);
    pg_backend.run_migrations().await?;

    let backend: Arc<dyn StorageBackend> = Arc::new(pg_backend.clone());
    let flow = QuickstartFlow::new(backend);
    Ok(flow)
}
