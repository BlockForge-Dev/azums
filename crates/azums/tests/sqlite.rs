use azums::{make_sqlite_pool, Job, SqliteBackend, StorageBackend};
use std::sync::Arc;

#[tokio::test]
async fn test_sqlite_backend_lifecycle() -> anyhow::Result<()> {
    let pool = make_sqlite_pool("sqlite::memory:").await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;
    backend.health_check().await?;

    let new_job = Job::new("sync_task", serde_json::json!({"item": 123}));
    let job_id = backend.enqueue(new_job.into()).await?;

    let leased = backend
        .lease_jobs_batch("default", "sqlite-worker-1", 10, 10)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);

    let attempts = backend
        .start_attempts_batch(&["default".into()], &[job_id], "sqlite-worker-1")
        .await?;
    assert_eq!(attempts.len(), 1);
    let (_jid, attempt_id, attempt_no) = attempts[0];
    assert_eq!(attempt_no, 1);

    backend
        .mark_succeeded(job_id, attempt_id, "sqlite-worker-1", 15)
        .await?;

    let fetched = backend.get_job(job_id).await?.unwrap();
    assert_eq!(fetched.status, "succeeded");

    Ok(())
}

#[tokio::test]
async fn test_sqlite_quickstart_flow() -> anyhow::Result<()> {
    let flow = azums::quickstart("sqlite::memory:").await?;

    let _id = flow
        .enqueue(Job::new("test_job", serde_json::json!({"ok": true})))
        .await?;

    let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executed_clone = executed.clone();

    flow.register_handler("test_job", move |_job| {
        let ex = executed_clone.clone();
        async move {
            ex.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let processed = flow.run_until_empty().await?;
    assert_eq!(processed, 1);
    assert!(executed.load(std::sync::atomic::Ordering::SeqCst));

    let job_opt = flow.enqueue(Job::new("dummy", serde_json::json!({}))).await;
    assert!(job_opt.is_ok());

    Ok(())
}
