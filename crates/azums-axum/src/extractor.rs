use axum_core::extract::{FromRef, FromRequestParts};
use azums_core::{NewJob, StorageBackend};
use chrono::{DateTime, Utc};
use http::request::Parts;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Axum extractor for enqueueing background jobs from HTTP handlers.
///
/// # Examples
///
/// ```rust,no_run
/// use axum::{routing::post, Json, Router};
/// use azums_axum::{BackgroundJobs, JobQueue};
/// use serde_json::json;
///
/// async fn create_order(
///     queue: JobQueue,
///     Json(payload): Json<serde_json::Value>,
/// ) -> Result<Json<serde_json::Value>, (http::StatusCode, String)> {
///     let job_id = queue
///         .enqueue_now("default", "process_order", payload)
///         .await
///         .map_err(|e| (http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
///
///     Ok(Json(json!({ "status": "queued", "job_id": job_id })))
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

    /// Enqueues a [`NewJob`] or [`Job`](azums_core::Job) into the queue.
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
                idempotency_key: None,
                run_at: Utc::now(),
                deadline_at: None,
                timeout_seconds: None,
                recurring_interval_seconds: None,
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
                idempotency_key: None,
                run_at,
                deadline_at: None,
                timeout_seconds: None,
                recurring_interval_seconds: None,
                priority: 0,
                max_attempts: 25,
            })
            .await
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for JobQueue
where
    JobQueue: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (http::StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Ok(JobQueue::from_ref(state))
    }
}
