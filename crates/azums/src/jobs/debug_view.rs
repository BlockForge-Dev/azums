use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::jobs::{AttemptsRepo, JobsRepo, PolicyDecisionsRepo};

#[derive(Debug, Serialize)]
/// Operator-oriented interpretation of what can happen to a job next.
pub enum NextAction {
    /// Job is waiting for a retry at the included time.
    RetryAt(DateTime<Utc>),
    /// Job is terminal in the dead-letter queue.
    Dlq {
        /// Machine-readable DLQ reason when available.
        reason: Option<String>,
    },
    /// Historical job may be replayed as new work.
    Replayable,
    /// Job currently has a running lease.
    Running,
    /// Job is queued or scheduled.
    Queued,
    /// Job completed successfully.
    Succeeded,
    /// Persisted state is not recognized by this unstable view.
    Unknown,
}

#[derive(Debug, Serialize)]
/// Unstable aggregate for debugging job state, history, and policy decisions.
pub struct DebugView {
    /// Job being inspected.
    pub job_id: Uuid,
    /// Current persisted job status.
    pub status: String,
    /// Serialized attempt timeline.
    pub attempts: serde_json::Value,
    /// Serialized policy-decision narrative.
    pub decisions: serde_json::Value,
    /// Operator-oriented next action inferred from current state.
    pub next_action: NextAction,
}

/// Builds an unstable debug aggregate for `job_id`, or `None` when absent.
pub async fn build_debug_view(
    jobs: &JobsRepo,
    attempts: &AttemptsRepo,
    decisions: &PolicyDecisionsRepo,
    job_id: Uuid,
) -> anyhow::Result<Option<DebugView>> {
    let Some(job) = jobs.get_job(job_id).await? else {
        return Ok(None);
    };

    let tl = crate::jobs::timeline::build_timeline(jobs, attempts, decisions, job_id).await?;
    let attempts_json = serde_json::to_value(tl.as_ref().map(|t| &t.attempts)).unwrap_or_default();
    let decisions_json = serde_json::to_value(tl.as_ref().map(|t| &t.story)).unwrap_or_default();

    let next_action = match job.status.as_str() {
        "queued" => NextAction::Queued,
        "running" => NextAction::Running,
        "succeeded" => NextAction::Succeeded,
        "dlq" => NextAction::Dlq {
            reason: job.dlq_reason_code.clone(),
        },
        "failed" => NextAction::Replayable,
        _ => NextAction::Unknown,
    };

    Ok(Some(DebugView {
        job_id,
        status: job.status.clone(),
        attempts: attempts_json,
        decisions: decisions_json,
        next_action,
    }))
}
