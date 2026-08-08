use crate::extractor::JobQueue;
use axum_core::extract::FromRef;
use azums::{quickstart, QuickstartFlow};
use azums_core::StorageBackend;
use std::sync::Arc;
use tokio::task::JoinHandle;

/// Central background job service attaching runtime workers and storage backends to Axum router state.
///
/// # Examples
///
/// ```rust,no_run
/// use axum::{routing::post, Router};
/// use azums_axum::{BackgroundJobs, JobQueue};
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let jobs = BackgroundJobs::from_url("sqlite://jobs.db?mode=rwc").await?;
///     jobs.register_handler("send_email", |_job| async move { Ok(()) }).await;
///     jobs.spawn_worker();
///
///     async fn handler(_queue: JobQueue) -> &'static str { "ok" }
///     let app: Router = Router::new()
///         .route("/orders", post(handler))
///         .with_state(jobs);
///
///     Ok(())
/// }
/// ```
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

impl FromRef<BackgroundJobs> for JobQueue {
    fn from_ref(state: &BackgroundJobs) -> Self {
        state.queue()
    }
}
