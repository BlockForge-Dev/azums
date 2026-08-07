use postgresflow::{quickstart, Job, MemoryBackend, StorageBackend};
use std::sync::Arc;

#[tokio::test]
async fn test_memory_backend_lifecycle() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;
    backend.health_check().await?;

    let job_id = backend
        .enqueue(Job::new("in_ram_task", serde_json::json!({"val": 42})).into())
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "mem-worker", 10, 5)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);

    let attempts = backend
        .start_attempts_batch(&["default".into()], &[job_id], "mem-worker")
        .await?;
    assert_eq!(attempts.len(), 1);
    let (_jid, attempt_id, attempt_no) = attempts[0];
    assert_eq!(attempt_no, 1);

    backend
        .mark_succeeded(job_id, attempt_id, "mem-worker", 5)
        .await?;

    let job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "succeeded");

    backend.clear();
    assert!(backend.get_job(job_id).await?.is_none());

    Ok(())
}

#[tokio::test]
async fn test_quickstart_in_memory() -> anyhow::Result<()> {
    // Developers can run cargo test with zero Docker and zero disk I/O!
    let flow = quickstart("memory").await?;

    let _id = flow
        .enqueue(Job::new("ephemeral_job", serde_json::json!({"data": "test"})))
        .await?;

    let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executed_clone = executed.clone();

    flow.register_handler("ephemeral_job", move |job| {
        let ex = executed_clone.clone();
        async move {
            assert_eq!(job.payload["data"], "test");
            ex.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 1);
    assert!(executed.load(std::sync::atomic::Ordering::SeqCst));

    Ok(())
}
