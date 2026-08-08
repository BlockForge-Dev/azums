use azums::{quickstart, Job};
use serde_json::json;

#[tokio::test]
async fn test_panic_isolation_routes_to_dlq_and_worker_survives() -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("w-panic")
        .with_queue("panic_queue");

    // Register a handler that panics
    flow.register_handler("panicking_job", |_j| async move {
        panic!("something terrible happened in handler!");
    })
    .await;

    // Register a normal handler
    flow.register_handler("normal_job", |_j| async move { Ok(()) })
        .await;

    // Enqueue panicking job first, then normal job
    let panic_job_id = flow
        .enqueue(Job::new("panicking_job", json!({})).queue("panic_queue"))
        .await?;

    let normal_job_id = flow
        .enqueue(Job::new("normal_job", json!({})).queue("panic_queue"))
        .await?;

    // Process both jobs
    let count = flow.run_until_empty().await?;
    assert_eq!(count, 2);

    // 1. Verify panicked job went to DLQ
    let panic_job = flow.backend().get_job(panic_job_id).await?.unwrap();
    assert_eq!(panic_job.status, "dlq");
    assert_eq!(panic_job.dlq_reason_code.as_deref(), Some("PANIC"));

    // 2. Verify normal job succeeded
    let normal_job = flow.backend().get_job(normal_job_id).await?.unwrap();
    assert_eq!(normal_job.status, "succeeded");

    Ok(())
}
