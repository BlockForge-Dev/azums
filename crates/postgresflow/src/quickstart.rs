use crate::{
    backend::PostgresBackend,
    jobs::{
        enqueue_guard::{EnqueueGuard, EnqueueGuardConfig},
        ingest_decisions::IngestDecisionsRepo,
        metrics::MetricsRepo,
        model::{Job, NewJob},
        policy_decisions::PolicyDecisionsRepo,
        retry::classify_error,
        retry::{next_delay_seconds, ErrorClass, RetryConfig},
    },
};
use postgresflow_core::StorageBackend;
use rand::{rngs::StdRng, SeedableRng};
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

pub type QuickstartHandlerFuture =
    Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>;
pub type QuickstartHandler = Arc<dyn Fn(Job) -> QuickstartHandlerFuture + Send + Sync>;

/// In-process worker runtime and admin API launcher built by [`quickstart`].
///
/// `QuickstartFlow` manages job enqueueing, handler registration, background worker leasing loops,
/// and the optional Axum admin web console through an abstract [`StorageBackend`].
pub struct QuickstartFlow {
    backend: Arc<dyn StorageBackend>,
    #[cfg(feature = "api")]
    postgres_backend: Option<PostgresBackend>,
    handlers: Arc<RwLock<HashMap<String, QuickstartHandler>>>,
    queue: String,
    worker_id: String,
    admin_addr: Option<String>,
    admin_started: std::sync::atomic::AtomicBool,
    retry_cfg: RetryConfig,
}

impl QuickstartFlow {
    /// Creates a `QuickstartFlow` wrapping a custom [`StorageBackend`].
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        let queue = std::env::var("PGFLOW_QUEUE").unwrap_or_else(|_| "default".to_string());
        let worker_id =
            std::env::var("PGFLOW_WORKER_ID").unwrap_or_else(|_| "quickstart-worker".to_string());
        let admin_addr = std::env::var("PGFLOW_ADMIN_ADDR")
            .ok()
            .or_else(|| Some("127.0.0.1:3003".to_string()));

        Self {
            backend,
            #[cfg(feature = "api")]
            postgres_backend: None,
            handlers: Arc::new(RwLock::new(HashMap::new())),
            queue,
            worker_id,
            admin_addr,
            admin_started: std::sync::atomic::AtomicBool::new(false),
            retry_cfg: RetryConfig::default(),
        }
    }

    /// Enqueues a job into the storage backend queue.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use postgresflow::{quickstart, Job};
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

    /// Registers an asynchronous handler closure for a specific `job_type`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use postgresflow::quickstart;
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

    /// Starts the in-process worker polling loop and admin HTTP API (if `api` feature is active).
    ///
    /// Runs continuously processing jobs until application termination.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use postgresflow::quickstart;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// flow.run().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run(&self) -> anyhow::Result<()> {
        self.ensure_admin_api();

        let mut last_reap_at = std::time::Instant::now();
        let reap_interval = std::time::Duration::from_secs(5);

        loop {
            if last_reap_at.elapsed() >= reap_interval {
                let _ = self.backend.reap_expired_locks().await;
                last_reap_at = std::time::Instant::now();
            }

            let batch = self
                .backend
                .lease_jobs_batch(&self.queue, &self.worker_id, 10, 32)
                .await?;

            if batch.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }

            self.process_batch(batch).await?;
        }
    }

    /// Runs worker polling loops until all currently queued jobs have been processed, returning total count.
    ///
    /// Useful for batch processing, integration tests, or unit testing job flows.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use postgresflow::quickstart;
    ///
    /// # async fn doc_test() -> anyhow::Result<()> {
    /// let flow = quickstart("postgres://localhost/flow").await?;
    /// let processed = flow.run_until_empty().await?;
    /// println!("Processed {processed} jobs");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_until_empty(&self) -> anyhow::Result<usize> {
        self.ensure_admin_api();
        let mut total_processed = 0;

        loop {
            let batch = self
                .backend
                .lease_jobs_batch(&self.queue, &self.worker_id, 10, 32)
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

            let start = std::time::Instant::now();
            match handler_opt {
                Some(handler) => {
                    let res = (handler)(job.clone()).await;
                    let latency_ms = start.elapsed().as_millis() as i32;

                    match res {
                        Ok(()) => {
                            self.backend
                                .mark_succeeded(job.id, attempt_id, &self.worker_id, latency_ms)
                                .await?;
                        }
                        Err(err) => {
                            self.handle_failure(
                                job.id,
                                attempt_id,
                                latency_ms,
                                "HANDLER_ERROR",
                                &err.to_string(),
                                attempt_no,
                                job.max_attempts,
                            )
                            .await?;
                        }
                    }
                }
                None => {
                    let latency_ms = start.elapsed().as_millis() as i32;
                    self.handle_failure(
                        job.id,
                        attempt_id,
                        latency_ms,
                        "UNKNOWN_JOB_TYPE",
                        &format!("no handler registered for job_type={}", job.job_type),
                        attempt_no,
                        job.max_attempts,
                    )
                    .await?;
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

    fn ensure_admin_api(&self) {
        if self
            .admin_started
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_err()
        {
            return;
        }

        #[cfg(feature = "api")]
        {
            if let (Some(addr), Some(pg)) = (&self.admin_addr, &self.postgres_backend) {
                let pool = pg.pool().clone();
                let policy_decisions_repo = PolicyDecisionsRepo::new(pool.clone());
                let ingest_decisions_repo = IngestDecisionsRepo::new(pool.clone());
                let metrics_repo = MetricsRepo::new(pool.clone());
                let enqueue_guard = EnqueueGuard::new(
                    pool.clone(),
                    ingest_decisions_repo.clone(),
                    EnqueueGuardConfig {
                        max_payload_bytes: 262144,
                        max_enqueues_per_minute_per_queue: 10000,
                    },
                );

                let api_state = crate::api::ApiState {
                    jobs: pg.jobs_repo().clone(),
                    attempts: pg.attempts_repo().clone(),
                    policy_decisions: policy_decisions_repo,
                    ingest_decisions: ingest_decisions_repo,
                    metrics: metrics_repo,
                    enqueue_guard,
                    api_token: None,
                };
                let app = crate::api::router(api_state);
                let addr = addr.clone();

                tokio::spawn(async move {
                    if let Ok(listener) = tokio::net::TcpListener::bind(&addr).await {
                        let _ = axum::serve(listener, app).await;
                    }
                });
            }
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
/// use postgresflow::{quickstart, Job};
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

    let mut candidates = Vec::new();
    let user_url = database_url.as_ref().trim();
    if !user_url.is_empty() {
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
        "postgres://postgres:postgres@127.0.0.1:5433/postgresflow_dev",
        "postgres://postgres:postgres@127.0.0.1:5432/postgresflow_dev",
        "postgres://postgres:postgres@localhost:5433/postgresflow_dev",
        "postgres://postgres:postgres@localhost:5432/postgresflow_dev",
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
                    pool_opt = Some(pool);
                    break;
                }
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    let pool = match pool_opt {
        Some(p) => p,
        None => {
            return Err(anyhow::anyhow!(
                "Failed to connect to any PostgreSQL database. Tried candidates: {:?}. Last error: {:?}",
                candidates,
                last_err
            ));
        }
    };

    let pg_backend = PostgresBackend::new(pool);
    pg_backend.run_migrations().await?;

    let backend: Arc<dyn StorageBackend> = Arc::new(pg_backend.clone());
    let mut flow = QuickstartFlow::new(backend);
    #[cfg(feature = "api")]
    {
        flow.postgres_backend = Some(pg_backend);
    }
    Ok(flow)
}
