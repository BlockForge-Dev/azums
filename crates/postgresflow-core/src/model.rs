use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Lightweight job summary model returned when listing jobs in Admin UI or APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct JobListItem {
    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    pub status: String,

    pub run_at: DateTime<Utc>,
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
/// use postgresflow_core::Job;
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

    pub id: Uuid,
    pub queue: String,
    pub job_type: String,
    #[cfg_attr(feature = "sqlx", sqlx(rename = "payload_json"))]
    pub payload: Value,
    pub run_at: DateTime<Utc>,
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
    /// use postgresflow_core::Job;
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
            id: Uuid::new_v4(),
            queue: "default".to_string(),
            job_type: job_type.into(),
            payload,
            run_at: now,
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

    /// Sets scheduled execution timestamp (`run_at`).
    pub fn run_at(mut self, run_at: DateTime<Utc>) -> Self {
        self.run_at = run_at;
        self
    }

    /// Returns reference to job JSON payload.
    pub fn payload_json(&self) -> &Value {
        &self.payload
    }
}

/// Specification for enqueueing a new job into a storage backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub queue: String,
    pub job_type: String,
    pub payload_json: Value,
    pub run_at: DateTime<Utc>,
    pub priority: i32,
    pub max_attempts: i32,
}

impl From<Job> for NewJob {
    fn from(job: Job) -> Self {
        NewJob {
            queue: job.queue,
            job_type: job.job_type,
            payload_json: job.payload,
            run_at: job.run_at,
            priority: job.priority,
            max_attempts: job.max_attempts,
        }
    }
}

/// Enumeration of possible job lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Dlq,
    Canceled,
}

impl JobStatus {
    /// Returns static string representation of job status.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use postgresflow_core::JobStatus;
    /// assert_eq!(JobStatus::Queued.as_str(), "queued");
    /// assert_eq!(JobStatus::Dlq.as_str(), "dlq");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Dlq => "dlq",
            JobStatus::Canceled => "canceled",
        }
    }
}
