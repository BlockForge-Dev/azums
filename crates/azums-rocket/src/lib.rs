//! # Azums Rocket
//!
//! Native Rocket request guard (`JobQueue`) and state service integration (`BackgroundJobs`) for `azums`.

use azums::{quickstart, QuickstartFlow};
pub use azums_core::{Job, JobListItem, JobStatus};
use azums_core::{NewJob, StorageBackend};
use chrono::{DateTime, Utc};
use rocket::request::{FromRequest, Outcome, Request};
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Rocket request guard for enqueueing background jobs from HTTP handlers.
///
/// # Examples
///
/// ```rust,no_run
/// use azums_rocket::{BackgroundJobs, JobQueue};
/// use rocket::{post, routes, serde::json::Json};
/// use serde_json::json;
///
/// #[post("/orders")]
/// async fn create_order(queue: JobQueue) -> Json<serde_json::Value> {
///     let job_id = queue
///         .enqueue_now("default", "process_order", json!({"item": "phone"}))
///         .await
///         .unwrap();
///     Json(json!({ "status": "queued", "id": job_id }))
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

#[rocket::async_trait]
impl<'r> FromRequest<'r> for JobQueue {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Some(jobs) = req.rocket().state::<BackgroundJobs>() {
            Outcome::Success(jobs.queue())
        } else if let Some(queue) = req.rocket().state::<JobQueue>() {
            Outcome::Success(queue.clone())
        } else {
            Outcome::Error((rocket::http::Status::InternalServerError, ()))
        }
    }
}

/// Central background job service attaching runtime workers and storage backends to Rocket state.
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
        F: Fn(azums_core::Job) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        self.flow.register_handler(job_type, handler).await;
    }

    /// Registers a trait-based [`JobProcessor`](azums_core::JobProcessor) for `job_type`.
    pub async fn register_processor<P>(&self, job_type: impl Into<String>, processor: P)
    where
        P: azums_core::JobProcessor + 'static,
    {
        self.flow.register_processor(job_type, processor).await;
    }

    /// Spawns the background worker polling loop as an asynchronous Tokio task.
    pub fn spawn_worker(&self) -> JoinHandle<anyhow::Result<()>> {
        let flow = self.flow.clone();
        tokio::spawn(async move { flow.run().await })
    }
}
