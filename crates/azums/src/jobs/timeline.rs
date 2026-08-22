use crate::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
/// Reconstructed job history combining attempts and policy decisions.
/// # Examples
///
/// ```rust
/// use azums::jobs::timeline::TimelineEvent;
///
/// let public_type = std::any::type_name::<TimelineEvent>();
/// assert!(public_type.ends_with("TimelineEvent"));
/// ```
pub struct JobTimeline {
    /// Job represented by this timeline.
    pub job_id: Uuid,
    /// Current persisted job status.
    pub status: String,
    /// Queue that owns the job.
    pub queue: String,
    /// Handler dispatch key.
    pub job_type: String,
    /// Current execution eligibility timestamp.
    pub run_at: DateTime<Utc>,

    /// Next scheduled execution time when queued.
    pub next_run_at: Option<DateTime<Utc>>,
    /// Most recent worker in attempt history.
    pub last_worker_id: Option<String>,
    /// Most recent failed-attempt error.
    pub last_error: Option<LastError>,

    // keep existing attempts list (backwards compatible)
    /// Attempts ordered by attempt number.
    pub attempts: Vec<TimelineAttempt>,

    // âœ… new: unified ordered narrative (attempts + policy decisions)
    /// Unified chronological attempt and policy narrative.
    pub story: Vec<TimelineEvent>,
}

#[derive(Debug, Serialize)]
/// One execution attempt rendered for the unstable timeline API.
/// # Examples
///
/// ```rust
/// use azums::jobs::timeline::TimelineEvent;
///
/// let public_type = std::any::type_name::<TimelineEvent>();
/// assert!(public_type.ends_with("TimelineEvent"));
/// ```
pub struct TimelineAttempt {
    /// Unique attempt identifier.
    pub id: Uuid,
    /// Monotonic attempt number.
    pub attempt_no: i32,
    /// Persisted attempt status.
    pub status: String,
    /// Attempt start time.
    pub started_at: DateTime<Utc>,
    /// Attempt completion time, if finished.
    pub finished_at: Option<DateTime<Utc>>,
    /// Machine-readable error code, if failed.
    pub error_code: Option<String>,
    /// Human-readable error detail, if failed.
    pub error_message: Option<String>,
    /// Measured attempt latency in milliseconds.
    pub latency_ms: Option<i32>,
    /// Worker that owned the attempt.
    pub worker_id: String,
    /// Suggested operator response derived from the error code.
    pub suggested_action: Option<String>,
}

#[derive(Debug, Serialize)]
/// Most recent error summary in a job timeline.
/// # Examples
///
/// ```rust
/// use azums::jobs::timeline::TimelineEvent;
///
/// let public_type = std::any::type_name::<TimelineEvent>();
/// assert!(public_type.ends_with("TimelineEvent"));
/// ```
pub struct LastError {
    /// Machine-readable failure code.
    pub error_code: Option<String>,
    /// Human-readable failure detail.
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
/// Chronological event in the unstable unified job narrative.
/// # Examples
///
/// ```rust
/// use azums::jobs::timeline::TimelineEvent;
///
/// let public_type = std::any::type_name::<TimelineEvent>();
/// assert!(public_type.ends_with("TimelineEvent"));
/// ```
pub enum TimelineEvent {
    /// Handler attempt event.
    Attempt {
        /// Event timestamp.
        at: DateTime<Utc>,
        /// Attempt identifier.
        id: Uuid,
        /// Attempt number.
        attempt_no: i32,
        /// Attempt status.
        status: String,
        /// Worker that owned the attempt.
        worker_id: String,
        /// Machine-readable error code.
        error_code: Option<String>,
        /// Human-readable error detail.
        error_message: Option<String>,
        /// Suggested operator response.
        suggested_action: Option<String>,
        /// Measured attempt latency in milliseconds.
        latency_ms: Option<i32>,
    },
    /// Queue-policy decision event.
    PolicyDecision {
        /// Event timestamp.
        at: DateTime<Utc>,
        /// Decision identifier.
        id: Uuid,
        /// Decision name.
        decision: String,
        /// Machine-readable decision reason.
        reason_code: String,
        /// Structured policy context.
        details_json: serde_json::Value,
    },
}

/// Reconstructs a chronological timeline for `job_id`, or `None` when absent.
/// # Examples
///
/// ```rust
/// use azums::jobs::timeline::TimelineEvent;
///
/// let public_type = std::any::type_name::<TimelineEvent>();
/// assert!(public_type.ends_with("TimelineEvent"));
/// ```
pub async fn build_timeline(
    jobs: &JobsRepo,
    attempts: &AttemptsRepo,
    policy_decisions: &PolicyDecisionsRepo,
    job_id: Uuid,
) -> anyhow::Result<Option<JobTimeline>> {
    let job = match jobs.get_job(job_id).await? {
        Some(j) => j,
        None => return Ok(None),
    };

    let raw_attempts = attempts.list_attempts_for_job(job_id).await?;
    let policy_rows = policy_decisions.list_for_job(job_id).await?;

    let last_worker_id = raw_attempts.last().map(|a| a.worker_id.clone());
    let last_failed = raw_attempts.iter().rev().find(|a| a.status == "failed");

    let last_error = last_failed.map(|a| LastError {
        error_code: a.error_code.clone(),
        error_message: a.error_message.clone(),
    });

    let next_run_at = if job.status == "queued" {
        Some(job.run_at)
    } else {
        None
    };

    // suggested action mapping
    use crate::jobs::error_codes;

    let attempts_out: Vec<TimelineAttempt> = raw_attempts
        .iter()
        .cloned()
        .map(|a| {
            let suggested = a
                .error_code
                .as_deref()
                .map(|code| error_codes::suggested_action(code).to_string());

            TimelineAttempt {
                id: a.id,
                attempt_no: a.attempt_no,
                status: a.status,
                started_at: a.started_at,
                finished_at: a.finished_at,
                error_code: a.error_code,
                error_message: a.error_message,
                latency_ms: a.latency_ms,
                worker_id: a.worker_id,
                suggested_action: suggested,
            }
        })
        .collect();

    // âœ… build unified story
    let mut story: Vec<TimelineEvent> = Vec::new();

    for a in &attempts_out {
        story.push(TimelineEvent::Attempt {
            at: a.started_at,
            id: a.id,
            attempt_no: a.attempt_no,
            status: a.status.clone(),
            worker_id: a.worker_id.clone(),
            error_code: a.error_code.clone(),
            error_message: a.error_message.clone(),
            suggested_action: a.suggested_action.clone(),
            latency_ms: a.latency_ms,
        });
    }

    for p in policy_rows {
        story.push(TimelineEvent::PolicyDecision {
            at: p.created_at,
            id: p.id,
            decision: p.decision,
            reason_code: p.reason_code,
            details_json: p.details_json,
        });
    }

    // sort by time, stable tie-break so output is deterministic
    story.sort_by(|a, b| {
        let (ta, ka) = match a {
            TimelineEvent::Attempt { at, attempt_no, .. } => (*at, (1i32, *attempt_no)),
            TimelineEvent::PolicyDecision { at, .. } => (*at, (0i32, 0)),
        };
        let (tb, kb) = match b {
            TimelineEvent::Attempt { at, attempt_no, .. } => (*at, (1i32, *attempt_no)),
            TimelineEvent::PolicyDecision { at, .. } => (*at, (0i32, 0)),
        };
        ta.cmp(&tb).then(ka.cmp(&kb))
    });

    Ok(Some(JobTimeline {
        job_id: job.id,
        status: job.status,
        queue: job.queue,
        job_type: job.job_type,
        run_at: job.run_at,
        next_run_at,
        last_worker_id,
        last_error,
        attempts: attempts_out,
        story,
    }))
}
