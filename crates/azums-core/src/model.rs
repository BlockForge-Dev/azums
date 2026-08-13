use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Per-queue job execution ordering policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum QueueOrdering {
    /// Process jobs in exact First-In, First-Out order by creation time (`created_at ASC`).
    #[default]
    Fifo,
    /// Process jobs as fast as possible without strict creation order guarantees.
    Fastest,
}

/// Configuration options for a job queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueConfig {
    pub ordering: QueueOrdering,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            ordering: QueueOrdering::Fifo,
        }
    }
}

impl QueueConfig {
    pub fn new(ordering: QueueOrdering) -> Self {
        Self { ordering }
    }
}

/// Named queue definition plus its execution policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub name: String,
    pub config: QueueConfig,
}

impl Queue {
    pub fn new(name: impl Into<String>, config: QueueConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }
}

/// Worker identity used for leases, attempts, and execution ownership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Worker {
    pub id: String,
}

impl Worker {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

/// Ordering strength exposed by a storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderingCapability {
    /// No meaningful ordering contract beyond at-least-once execution.
    None,
    /// Runnable jobs are leased in priority/schedule/FIFO order where the backend supports it.
    FifoLeasing,
    /// Backend supports both FIFO leasing and fastest-throughput leasing modes.
    FifoAndFastestLeasing,
}

/// Backpressure behavior exposed by a storage backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackpressureCapability {
    /// The backend accepts committed jobs and represents overload as queued backlog.
    BacklogOnly,
    /// The backend can throttle worker leasing through queue policies without dropping jobs.
    ExecutionRateLimit,
}

/// Storage backend feature and guarantee declaration.
///
/// Capabilities describe what a backend can honestly provide. They are not a marketing matrix:
/// application code can inspect this value when it needs a specific storage guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub transactional_enqueue: bool,
    pub durable_jobs: bool,
    pub notifications: bool,
    pub streams: bool,
    pub consumer_groups: bool,
    pub distributed_workers: bool,
    pub ordering: OrderingCapability,
    pub backpressure: BackpressureCapability,
}

impl BackendCapabilities {
    pub const fn memory() -> Self {
        Self {
            transactional_enqueue: false,
            durable_jobs: false,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: false,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    }

    pub const fn sqlite() -> Self {
        Self {
            transactional_enqueue: true,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: false,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    }

    pub const fn postgres() -> Self {
        Self {
            transactional_enqueue: true,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: true,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::ExecutionRateLimit,
        }
    }

    pub const fn redis() -> Self {
        Self {
            transactional_enqueue: false,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: true,
            ordering: OrderingCapability::FifoLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    }

    pub fn supports_portable_job_api(&self) -> bool {
        self.durable_jobs || !self.distributed_workers
    }
}

/// Lightweight job summary model returned when listing jobs in Admin UI or APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JobListItem {
    pub id: Uuid,
    pub idempotency_key: Option<String>,
    pub queue: String,
    pub job_type: String,
    pub status: String,

    pub run_at: DateTime<Utc>,
    #[serde(default)]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
    #[serde(default)]
    pub recurring_interval_seconds: Option<i64>,
    pub priority: i32,
    pub max_attempts: i32,

    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,

    pub dlq_reason_code: Option<String>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Primary job entity representing a unit of work stored in a storage backend.
///
/// # Examples
///
/// ```rust
/// use azums_core::Job;
///
/// let job = Job::new("email_send", serde_json::json!({"to": "user@example.com"}))
///     .queue("emails")
///     .priority(10)
///     .max_attempts(5);
///
/// assert_eq!(job.queue, "emails");
/// assert_eq!(job.priority, 10);
/// assert_eq!(job.max_attempts, 5);
/// assert_eq!(job.payload["to"], "user@example.com");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Job {
    pub dataset_id: String,
    pub replay_of_job_id: Option<Uuid>,
    pub idempotency_key: Option<String>,

    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    #[cfg_attr(feature = "sqlx", sqlx(rename = "payload_json"))]
    pub payload: Value,
    pub run_at: DateTime<Utc>,
    #[serde(default)]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
    #[serde(default)]
    pub recurring_interval_seconds: Option<i64>,
    pub status: String,
    pub priority: i32,
    pub max_attempts: i32,

    pub locked_at: Option<DateTime<Utc>>,
    pub locked_by: Option<String>,
    pub lock_expires_at: Option<DateTime<Utc>>,

    pub dlq_reason_code: Option<String>,
    pub dlq_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    /// Creates a new `Job` with default queue `"default"`, priority `0`, and max attempts `25`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use azums_core::Job;
    ///
    /// let job = Job::new("greet", serde_json::json!({"name": "World"}));
    /// assert_eq!(job.job_type, "greet");
    /// assert_eq!(job.payload["name"], "World");
    /// ```
    pub fn new(job_type: impl Into<String>, payload: Value) -> Self {
        let now = Utc::now();
        Self {
            dataset_id: "default".to_string(),
            replay_of_job_id: None,
            idempotency_key: None,
            id: Uuid::new_v4(),
            queue: "default".to_string(),
            job_type: job_type.into(),
            payload,
            run_at: now,
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            status: JobStatus::Queued.as_str().to_string(),
            priority: 0,
            max_attempts: 25,
            locked_at: None,
            locked_by: None,
            lock_expires_at: None,
            dlq_reason_code: None,
            dlq_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Sets target queue name for this job.
    pub fn queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = queue.into();
        self
    }

    /// Sets job execution priority (higher numbers are leased first).
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Sets maximum retry attempts before moving job to Dead-Letter Queue (DLQ).
    pub fn max_attempts(mut self, max_attempts: i32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Sets an application-provided enqueue idempotency key.
    ///
    /// Backends that support idempotent enqueue return the existing logical job ID when another
    /// enqueue uses the same key.
    pub fn idempotency_key(mut self, idempotency_key: impl Into<String>) -> Self {
        self.idempotency_key = Some(idempotency_key.into());
        self
    }

    /// Sets scheduled execution timestamp (`run_at`).
    pub fn run_at(mut self, run_at: DateTime<Utc>) -> Self {
        self.run_at = run_at;
        self
    }

    /// Sets the latest timestamp at which this job may start execution.
    ///
    /// If the backend clock is already past this value when workers try to lease the job, Azums
    /// moves the job to DLQ with `DEADLINE_EXCEEDED` instead of executing it late.
    pub fn deadline_at(mut self, deadline_at: DateTime<Utc>) -> Self {
        self.deadline_at = Some(deadline_at);
        self
    }

    /// Sets a per-attempt handler timeout in seconds.
    ///
    /// Worker runtimes that execute handlers enforce this as a handler execution timeout and route
    /// timeout failures through normal retry/DLQ classification.
    pub fn timeout_seconds(mut self, timeout_seconds: i64) -> Self {
        self.timeout_seconds = Some(timeout_seconds.max(0));
        self
    }

    /// Sets fixed-interval recurring execution in seconds.
    ///
    /// After a successful occurrence, Azums enqueues the next occurrence as a new logical job with
    /// `run_at = previous_run_at + recurring_interval_seconds`.
    pub fn recurring_interval_seconds(mut self, interval_seconds: i64) -> Self {
        self.recurring_interval_seconds = Some(interval_seconds.max(1));
        self
    }

    /// Returns reference to job JSON payload.
    pub fn payload_json(&self) -> &Value {
        &self.payload
    }

    /// Derives the canonical lifecycle state from this persisted job and attempt history.
    ///
    /// `failed_attempts` is the number of durable failed `JobAttempt` rows for this job.
    pub fn lifecycle_state_at(
        &self,
        now: DateTime<Utc>,
        failed_attempts: usize,
    ) -> Result<JobLifecycleState, crate::error::Error> {
        JobLifecycleState::from_persisted(
            JobStatus::parse(&self.status)?,
            self.run_at,
            now,
            failed_attempts,
        )
    }

    /// Deserializes the JSON payload into a concrete type `T`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use azums_core::{Job, Error};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize, Debug, PartialEq)]
    /// struct EmailPayload {
    ///     to: String,
    /// }
    ///
    /// let job = Job::new("email", serde_json::json!({"to": "a@b.com"}));
    /// let payload: EmailPayload = job.payload_typed().unwrap();
    /// assert_eq!(payload.to, "a@b.com");
    /// ```
    pub fn payload_typed<T: serde::de::DeserializeOwned>(&self) -> Result<T, crate::error::Error> {
        serde_json::from_value(self.payload.clone())
            .map_err(crate::error::Error::PayloadDeserialization)
    }
}

/// Trait-based job processor interface for structured background workers.
#[async_trait::async_trait]
pub trait JobProcessor: Send + Sync {
    /// Processes a single background job execution attempt.
    async fn process(&self, job: Job) -> anyhow::Result<()>;
}

/// Specification for enqueueing a new job into a storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub queue: String,
    pub job_type: String,
    pub payload_json: Value,
    pub idempotency_key: Option<String>,
    pub run_at: DateTime<Utc>,
    #[serde(default)]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
    #[serde(default)]
    pub recurring_interval_seconds: Option<i64>,
    pub priority: i32,
    pub max_attempts: i32,
}

/// Runtime execution claim tying a job, durable attempt, worker, and lease together.
///
/// `JobExecution` is the in-flight view of work. The durable record of the handler run is
/// `JobAttempt`; the durable record of the work item is `Job`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobExecution {
    pub job_id: Uuid,
    pub attempt_id: Uuid,
    pub attempt_no: i32,
    pub worker_id: String,
    pub lease_expires_at: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
}

impl From<Job> for NewJob {
    fn from(job: Job) -> Self {
        NewJob {
            queue: job.queue,
            job_type: job.job_type,
            payload_json: job.payload,
            idempotency_key: job.idempotency_key,
            run_at: job.run_at,
            deadline_at: job.deadline_at,
            timeout_seconds: job.timeout_seconds,
            recurring_interval_seconds: job.recurring_interval_seconds,
            priority: job.priority,
            max_attempts: job.max_attempts,
        }
    }
}

/// Stored job status values.
///
/// The canonical execution model is expressed by [`JobLifecycleState`]. Storage backends
/// continue to persist compact lowercase strings for compatibility with existing schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    /// Canonical completed terminal state.
    Completed,
    /// Backward-compatible alias for [`JobStatus::Completed`].
    Succeeded,
    /// Legacy job-level failure status. New executions should record failures on
    /// `JobAttempt` and move the job to retry wait or DLQ instead.
    Failed,
    Dlq,
    /// Canonical cancelled terminal state.
    Cancelled,
    /// Backward-compatible alias for [`JobStatus::Cancelled`].
    Canceled,
}

impl JobStatus {
    /// Returns static string representation of job status.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use azums_core::JobStatus;
    /// assert_eq!(JobStatus::Queued.as_str(), "queued");
    /// assert_eq!(JobStatus::Dlq.as_str(), "dlq");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Completed | JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Dlq => "dlq",
            JobStatus::Cancelled | JobStatus::Canceled => "canceled",
        }
    }

    /// Parses a persisted status string.
    pub fn parse(status: &str) -> Result<Self, crate::error::Error> {
        match status {
            "queued" => Ok(JobStatus::Queued),
            "running" => Ok(JobStatus::Running),
            "succeeded" | "completed" => Ok(JobStatus::Completed),
            "failed" => Ok(JobStatus::Failed),
            "dlq" => Ok(JobStatus::Dlq),
            "canceled" | "cancelled" => Ok(JobStatus::Cancelled),
            other => Err(crate::error::Error::InvalidState(format!(
                "unknown job status '{other}'"
            ))),
        }
    }

    /// Returns true when this persisted status represents a terminal job state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed
                | JobStatus::Succeeded
                | JobStatus::Dlq
                | JobStatus::Cancelled
                | JobStatus::Canceled
        )
    }
}

/// Canonical logical job lifecycle state.
///
/// `Scheduled` and `RetryWait` are derived from persisted state: both are stored as
/// `status = "queued"` with a future `run_at`, but `RetryWait` also has prior failed
/// attempt history. This keeps storage compact while still making lifecycle reconstruction
/// deterministic from persisted job and attempt rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JobLifecycleState {
    Scheduled,
    Queued,
    Running,
    Completed,
    RetryWait,
    Cancelled,
    Dlq,
}

impl JobLifecycleState {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobLifecycleState::Scheduled => "scheduled",
            JobLifecycleState::Queued => "queued",
            JobLifecycleState::Running => "running",
            JobLifecycleState::Completed => "completed",
            JobLifecycleState::RetryWait => "retry_wait",
            JobLifecycleState::Cancelled => "cancelled",
            JobLifecycleState::Dlq => "dlq",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobLifecycleState::Completed | JobLifecycleState::Cancelled | JobLifecycleState::Dlq
        )
    }

    pub fn legal_successors(&self) -> &'static [JobLifecycleState] {
        use JobLifecycleState::*;
        match self {
            Scheduled => &[Queued],
            Queued => &[Running],
            Running => &[Completed, RetryWait, Cancelled, Dlq],
            RetryWait => &[Queued],
            Completed | Cancelled | Dlq => &[],
        }
    }

    pub fn can_transition_to(&self, next: JobLifecycleState) -> bool {
        self.legal_successors().contains(&next)
    }

    pub fn ensure_transition_to(&self, next: JobLifecycleState) -> Result<(), crate::error::Error> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(crate::error::Error::InvalidState(format!(
                "illegal job state transition: {} -> {}",
                self.as_str(),
                next.as_str()
            )))
        }
    }

    /// Derives the canonical state from persisted job state and attempt history.
    pub fn from_persisted(
        status: JobStatus,
        run_at: DateTime<Utc>,
        now: DateTime<Utc>,
        failed_attempts: usize,
    ) -> Result<Self, crate::error::Error> {
        match status {
            JobStatus::Queued if run_at > now && failed_attempts > 0 => {
                Ok(JobLifecycleState::RetryWait)
            }
            JobStatus::Queued if run_at > now => Ok(JobLifecycleState::Scheduled),
            JobStatus::Queued => Ok(JobLifecycleState::Queued),
            JobStatus::Running => Ok(JobLifecycleState::Running),
            JobStatus::Completed | JobStatus::Succeeded => Ok(JobLifecycleState::Completed),
            JobStatus::Dlq => Ok(JobLifecycleState::Dlq),
            JobStatus::Cancelled | JobStatus::Canceled => Ok(JobLifecycleState::Cancelled),
            JobStatus::Failed => Err(crate::error::Error::InvalidState(
                "job status 'failed' is legacy; failures belong to JobAttempt".to_string(),
            )),
        }
    }
}

/// Asynchronous job handler closure type alias.
pub type JobHandler = std::sync::Arc<
    dyn Fn(Job) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Represents an immutable event stored within a durable stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Event {
    /// Monotonically increasing 1-based sequence number within the stream.
    pub sequence_no: i64,
    /// Name of the target stream log (e.g., "orders", "audit_logs").
    pub stream_name: String,
    /// Domain-specific identifier for the event type (e.g., "order_created").
    pub event_type: String,
    /// JSON payload content of the event.
    pub payload_json: serde_json::Value,
    /// Timestamp when the event was appended to the stream log.
    pub created_at: DateTime<Utc>,
}

/// Input model for publishing a new event into a stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEvent {
    /// Domain-specific identifier for the event type (e.g., "order_created").
    pub event_type: String,
    /// JSON payload content of the event.
    pub payload_json: serde_json::Value,
}

impl NewEvent {
    /// Creates a new `NewEvent` with the specified event type and JSON payload.
    pub fn new(event_type: impl Into<String>, payload_json: serde_json::Value) -> Self {
        Self {
            event_type: event_type.into(),
            payload_json,
        }
    }
}

/// Status and offset information for a consumer group registered on a stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ConsumerGroupStatus {
    /// Identifier of the consumer group (e.g., "analytics_processor").
    pub consumer_group: String,
    /// Name of the stream log.
    pub stream_name: String,
    /// Highest sequence number successfully acknowledged by this consumer group.
    pub last_acked_seq: i64,
    /// Timestamp when the offset was last updated.
    pub updated_at: DateTime<Utc>,
}
