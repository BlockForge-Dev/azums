use crate::model::Job;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobObservationEvent {
    pub at: DateTime<Utc>,
    pub job_id: Uuid,
    pub attempt: Option<i32>,
    pub worker_id: Option<String>,
    pub queue: String,
    pub duration_ms: Option<i32>,
    pub status: String,
    pub retry_count: i32,
    pub error: Option<String>,
    pub trace_id: Option<String>,
}

impl JobObservationEvent {
    /// Returns OpenTelemetry-compatible span attributes for this job lifecycle observation.
    pub fn span_attributes(&self) -> BTreeMap<&'static str, String> {
        let mut attrs = BTreeMap::new();
        attrs.insert("azums.job_id", self.job_id.to_string());
        attrs.insert("azums.queue", self.queue.clone());
        attrs.insert("azums.status", self.status.clone());
        attrs.insert("azums.retry_count", self.retry_count.to_string());

        if let Some(attempt) = self.attempt {
            attrs.insert("azums.attempt", attempt.to_string());
        }
        if let Some(worker_id) = &self.worker_id {
            attrs.insert("azums.worker_id", worker_id.clone());
        }
        if let Some(duration_ms) = self.duration_ms {
            attrs.insert("azums.duration_ms", duration_ms.to_string());
        }
        if let Some(error) = &self.error {
            attrs.insert("azums.error", error.clone());
        }
        if let Some(trace_id) = &self.trace_id {
            attrs.insert("trace_id", trace_id.clone());
        }

        attrs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExplanation {
    pub job_id: Uuid,
    pub job_type: String,
    pub queue: String,
    pub status: String,
    pub retry_count: i32,
    pub last_worker_id: Option<String>,
    pub last_error: Option<String>,
    pub trace_id: Option<String>,
    pub events: Vec<JobObservationEvent>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMetrics {
    pub at: DateTime<Utc>,
    pub queue: String,
    pub jobs_total: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_retried: u64,
    pub jobs_dlq: u64,
    pub queue_depth: u64,
    pub execution_latency_ms_avg: f64,
    pub claim_latency_ms_avg: f64,
    pub retry_latency_ms_avg: f64,
    pub worker_count: u64,
}

#[async_trait]
pub trait ObservabilityBackend: Send + Sync {
    async fn explain_job(&self, job_id: Uuid) -> anyhow::Result<Option<JobExplanation>>;

    async fn queue_metrics(&self, queue: Option<&str>) -> anyhow::Result<Vec<QueueMetrics>>;
}

pub fn trace_id_from_job(job: &Job) -> Option<String> {
    job.payload
        .get("trace_id")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            job.payload
                .get("metadata")
                .and_then(|value| value.get("trace_id"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
}
