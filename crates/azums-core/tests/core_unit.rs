use azums_core::{Error, Job, JobLifecycleState, JobStatus, MemoryBackend, NewJob, StorageBackend};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, Debug, PartialEq)]
struct CustomPayload {
    count: u32,
    name: String,
}

#[tokio::test]
async fn test_core_job_creation_and_typed_payload() -> anyhow::Result<()> {
    let job = Job::new("metrics", json!({"count": 100, "name": "cpu"}));
    assert_eq!(job.job_type, "metrics");
    assert_eq!(job.queue, "default");
    assert_eq!(job.priority, 0);
    assert_eq!(job.max_attempts, 25);

    let parsed: CustomPayload = job.payload_typed()?;
    assert_eq!(parsed.count, 100);
    assert_eq!(parsed.name, "cpu");

    let invalid_job = Job::new("bad_json", json!("string_instead_of_obj"));
    let err = invalid_job.payload_typed::<CustomPayload>();
    assert!(matches!(err, Err(Error::PayloadDeserialization(_))));

    Ok(())
}

#[test]
fn canonical_state_machine_accepts_only_legal_transitions() {
    use JobLifecycleState::*;

    let legal = [
        (Scheduled, Queued),
        (Queued, Running),
        (Running, Completed),
        (Running, RetryWait),
        (RetryWait, Queued),
        (Running, Cancelled),
        (Running, Dlq),
    ];

    for &(from, to) in &legal {
        assert!(
            from.can_transition_to(to),
            "{:?} should legally transition to {:?}",
            from,
            to
        );
    }

    let states = [
        Scheduled, Queued, Running, Completed, RetryWait, Cancelled, Dlq,
    ];
    for &from in &states {
        for &to in &states {
            if !legal.contains(&(from, to)) {
                assert!(
                    !from.can_transition_to(to),
                    "{:?} -> {:?} must be illegal",
                    from,
                    to
                );
            }
        }
    }

    assert!(Completed.is_terminal());
    assert!(Cancelled.is_terminal());
    assert!(Dlq.is_terminal());
    assert!(Completed.legal_successors().is_empty());
}

#[test]
fn canonical_state_is_reconstructed_from_persisted_job_and_attempts() {
    let now = Utc::now();

    assert_eq!(
        JobLifecycleState::from_persisted(
            JobStatus::Queued,
            now + ChronoDuration::seconds(30),
            now,
            0,
        )
        .unwrap(),
        JobLifecycleState::Scheduled
    );

    assert_eq!(
        JobLifecycleState::from_persisted(
            JobStatus::Queued,
            now + ChronoDuration::seconds(30),
            now,
            1,
        )
        .unwrap(),
        JobLifecycleState::RetryWait
    );

    assert_eq!(
        JobLifecycleState::from_persisted(JobStatus::Queued, now, now, 0).unwrap(),
        JobLifecycleState::Queued
    );

    assert_eq!(
        JobLifecycleState::from_persisted(JobStatus::Completed, now, now, 0).unwrap(),
        JobLifecycleState::Completed
    );
}

#[tokio::test]
async fn test_in_memory_backend_edge_cases() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;
    backend.health_check().await?;

    // 1. Enqueue job with custom queue and priority
    let new_job = NewJob {
        queue: "high_priority".to_string(),
        job_type: "urgent_task".to_string(),
        payload_json: json!({"data": 123}),
        idempotency_key: None,
        run_at: Utc::now(),
        deadline_at: None,
        timeout_seconds: None,
        recurring_interval_seconds: None,
        priority: 100,
        max_attempts: 2,
    };
    let job_id = backend.enqueue(new_job).await?;

    // 2. Fetch single job
    let retrieved = backend.get_job(job_id).await?.expect("Job should exist");
    assert_eq!(retrieved.priority, 100);
    assert_eq!(retrieved.queue, "high_priority");
    assert_eq!(retrieved.status, "queued");

    // 3. Lease from high_priority queue
    let leased = backend
        .dequeue_and_lease("high_priority", "worker-1", 30, 10)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);

    // 4. Start attempt and fail to trigger retry
    let attempts = backend
        .start_attempts_batch(&["high_priority".to_string()], &[job_id], "worker-1")
        .await?;
    let (_, attempt_id, attempt_no) = attempts[0];
    assert_eq!(attempt_no, 1);

    backend
        .retry_job(
            job_id,
            attempt_id,
            "worker-1",
            10,
            Utc::now(),
            "ERR_TIMEOUT",
            "Simulated timeout",
            1,
        )
        .await?;

    let retried_job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(retried_job.status, "queued");

    // 5. Lease attempt #2 and fail to trigger DLQ (since max_attempts=2)
    let leased2 = backend
        .dequeue_and_lease("high_priority", "worker-1", 30, 10)
        .await?;
    assert_eq!(leased2.len(), 1);

    let attempts2 = backend
        .start_attempts_batch(&["high_priority".to_string()], &[job_id], "worker-1")
        .await?;
    let (_, attempt_id2, attempt_no2) = attempts2[0];
    assert_eq!(attempt_no2, 2);

    backend
        .fail_job(
            job_id,
            attempt_id2,
            "worker-1",
            15,
            "EXCEEDED_MAX_ATTEMPTS",
            "ERR_TIMEOUT",
            "Simulated timeout second failure",
            2,
        )
        .await?;

    let dlq_job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(dlq_job.status, "dlq");
    assert_eq!(
        dlq_job.dlq_reason_code.as_deref(),
        Some("EXCEEDED_MAX_ATTEMPTS")
    );

    Ok(())
}

#[tokio::test]
async fn memory_backend_rejects_invalid_terminal_state_transitions() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    let job_id = backend
        .enqueue(NewJob {
            queue: "default".to_string(),
            job_type: "one_shot".to_string(),
            payload_json: json!({}),
            idempotency_key: None,
            run_at: Utc::now(),
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 1,
        })
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "worker-a", 30, 1)
        .await?;
    assert_eq!(leased[0].id, job_id);

    let attempts = backend
        .start_attempts_batch(&["default".to_string()], &[job_id], "worker-a")
        .await?;
    let (_, attempt_id, _) = attempts[0];

    backend
        .mark_succeeded(job_id, attempt_id, "worker-a", 5)
        .await?;

    let second_complete = backend
        .mark_succeeded(job_id, attempt_id, "worker-a", 5)
        .await;
    assert!(
        second_complete.is_err(),
        "completed jobs must reject repeated completion"
    );

    let retry_terminal = backend
        .reschedule_for_retry(
            job_id,
            attempt_id,
            "worker-a",
            5,
            Utc::now(),
            "TIMEOUT",
            "late retry",
            1,
        )
        .await;
    assert!(
        retry_terminal.is_err(),
        "completed jobs must reject retry transitions"
    );

    Ok(())
}

#[tokio::test]
async fn memory_backend_cancel_primitive_enforces_ownership_and_terminality() -> anyhow::Result<()>
{
    let backend = MemoryBackend::new();

    let queued_id = backend
        .enqueue(NewJob {
            queue: "default".to_string(),
            job_type: "cancel_queued".to_string(),
            payload_json: json!({}),
            idempotency_key: None,
            run_at: Utc::now(),
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 1,
        })
        .await?;
    backend.cancel_job(queued_id, None).await?;
    assert_eq!(
        backend.get_job(queued_id).await?.unwrap().status,
        JobStatus::Cancelled.as_str()
    );

    let running_id = backend
        .enqueue(NewJob {
            queue: "default".to_string(),
            job_type: "cancel_running".to_string(),
            payload_json: json!({}),
            idempotency_key: None,
            run_at: Utc::now(),
            deadline_at: None,
            timeout_seconds: None,
            recurring_interval_seconds: None,
            priority: 0,
            max_attempts: 1,
        })
        .await?;
    backend
        .lease_jobs_batch("default", "owner-worker", 30, 1)
        .await?;
    let attempts = backend
        .start_attempts_batch(&["default".to_string()], &[running_id], "owner-worker")
        .await?;
    assert_eq!(attempts[0].2, 1);

    let wrong_worker = backend.cancel_job(running_id, Some("other-worker")).await;
    assert!(
        wrong_worker.is_err(),
        "running cancellation requires the lease owner"
    );

    backend.cancel_job(running_id, Some("owner-worker")).await?;
    assert_eq!(
        backend.get_job(running_id).await?.unwrap().status,
        JobStatus::Cancelled.as_str()
    );

    let terminal_cancel = backend.cancel_job(running_id, Some("owner-worker")).await;
    assert!(
        terminal_cancel.is_err(),
        "cancelled jobs must reject repeated cancellation"
    );

    Ok(())
}
