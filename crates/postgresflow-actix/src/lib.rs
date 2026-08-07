//! # PostgresFlow Actix
//!
//! Native Actix Web extractor (`JobQueue`) and state service integration (`BackgroundJobs`) for `postgresflow`.

use actix_web::{dev::Payload, web, FromRequest, HttpRequest};
use chrono::{DateTime, Utc};
use futures_util::future::{ready, Ready};
use postgresflow::{quickstart, QuickstartFlow};
use postgresflow_core::{NewJob, StorageBackend};
pub use postgresflow_core::{Job, JobListItem, JobStatus};
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Actix Web request extractor for enqueueing background jobs from HTTP handlers.
///
/// # Examples
///
/// ```rust,no_run
/// use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
/// use postgresflow_actix::{BackgroundJobs, JobQueue};
/// use serde_json::json;
///
/// #[post("/orders")]
/// async fn create_order(queue: JobQueue) -> impl Responder {
///     let job_id = queue
///         .enqueue_now("default", "process_order", json!({"item": "book"}))
///         .await
///         .unwrap();
///     HttpResponse::Ok().json(json!({ "status": "queued", "id": job_id }))
/// }
/// ```
#[derive(Clone)]
pub struct JobQueue(pub Arc<dyn StorageBackend>);

impl JobQueue {
    /// Creates a `JobQueue` wrapping a [`StorageBackend`].
    pub fn new(backend: Arc<dyn StorageBackend>) -> Self {
        Self(backend)
    }

    /// Returns reference to the underlying [`StorageBackend`].
    pub fn backend(&self) -> &Arc<dyn StorageBackend> {
        &self.0
    }

    /// Enqueues a [`NewJob`] or [`Job`] into the queue.
    pub async fn enqueue(&self, job: impl Into<NewJob>) -> anyhow::Result<Uuid> {
        let new_job: NewJob = job.into();
        self.0.enqueue(new_job).await
    }

    /// Enqueues a job immediately with default queue `"default"`, priority `0`, and `25` max attempts.
    pub async fn enqueue_now(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: Value,
    ) -> anyhow::Result<Uuid> {
        self.0
            .enqueue(NewJob {
                queue: queue.to_string(),
                job_type: job_type.to_string(),
                payload_json,
                run_at: Utc::now(),
                priority: 0,
                max_attempts: 25,
            })
            .await
    }

    /// Enqueues a job delayed by `delay_secs` seconds.
    pub async fn enqueue_in(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: Value,
        delay_secs: i64,
    ) -> anyhow::Result<Uuid> {
        self.enqueue_at(
            queue,
            job_type,
            payload_json,
            Utc::now() + chrono::Duration::seconds(delay_secs),
        )
        .await
    }

    /// Enqueues a job scheduled for a specific UTC timestamp (`run_at`).
    pub async fn enqueue_at(
        &self,
        queue: &str,
        job_type: &str,
        payload_json: Value,
        run_at: DateTime<Utc>,
    ) -> anyhow::Result<Uuid> {
        self.0
            .enqueue(NewJob {
                queue: queue.to_string(),
                job_type: job_type.to_string(),
                payload_json,
                run_at,
                priority: 0,
                max_attempts: 25,
            })
            .await
    }
}

impl FromRequest for JobQueue {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        if let Some(jobs) = req.app_data::<web::Data<BackgroundJobs>>() {
            ready(Ok(jobs.queue()))
        } else if let Some(queue) = req.app_data::<web::Data<JobQueue>>() {
            ready(Ok(queue.get_ref().clone()))
        } else {
            ready(Err(actix_web::error::ErrorInternalServerError(
                "BackgroundJobs or JobQueue app_data missing in Actix Web request",
            )))
        }
    }
}

/// Central background job service attaching runtime workers and storage backends to Actix Web app data.
#[derive(Clone)]
pub struct BackgroundJobs {
    backend: Arc<dyn StorageBackend>,
    flow: Arc<QuickstartFlow>,
}

impl BackgroundJobs {
    /// Connects to database URL (PostgreSQL, SQLite, or In-Memory) and initializes `BackgroundJobs`.
    pub async fn from_url(url: impl AsRef<str>) -> anyhow::Result<Self> {
        let flow = quickstart(url).await?;
        let flow = Arc::new(flow);
        Ok(Self {
            backend: flow.backend().clone(),
            flow,
        })
    }

    /// Creates `BackgroundJobs` from a pre-configured [`QuickstartFlow`].
    pub fn from_flow(flow: QuickstartFlow) -> Self {
        let flow = Arc::new(flow);
        Self {
            backend: flow.backend().clone(),
            flow,
        }
    }

    /// Creates `BackgroundJobs` from a custom [`StorageBackend`].
    pub fn from_backend(backend: Arc<dyn StorageBackend>) -> Self {
        let flow = QuickstartFlow::new(backend.clone());
        let flow = Arc::new(flow);
        Self { backend, flow }
    }

    /// Returns reference to the underlying [`StorageBackend`].
    pub fn backend(&self) -> Arc<dyn StorageBackend> {
        self.backend.clone()
    }

    /// Returns a [`JobQueue`] extractor instance.
    pub fn queue(&self) -> JobQueue {
        JobQueue(self.backend.clone())
    }

    /// Registers an async handler closure for `job_type`.
    pub async fn register_handler<F, Fut>(&self, job_type: impl Into<String>, handler: F)
    where
        F: Fn(postgresflow_core::Job) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.flow.register_handler(job_type, handler).await;
    }

    /// Registers a trait-based [`JobProcessor`](postgresflow_core::JobProcessor) for `job_type`.
    pub async fn register_processor<P>(&self, job_type: impl Into<String>, processor: P)
    where
        P: postgresflow_core::JobProcessor + 'static,
    {
        self.flow.register_processor(job_type, processor).await;
    }

    /// Spawns the background worker polling loop as an asynchronous Tokio task.
    pub fn spawn_worker(&self) -> JoinHandle<anyhow::Result<()>> {
        let flow = self.flow.clone();
        tokio::spawn(async move { flow.run().await })
    }
}
