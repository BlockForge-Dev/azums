use chrono::Utc;
use postgresflow_core::{Error, Job, MemoryBackend, NewJob, StorageBackend};
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
        run_at: Utc::now(),
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
    assert_eq!(dlq_job.dlq_reason_code.as_deref(), Some("EXCEEDED_MAX_ATTEMPTS"));

    Ok(())
}
