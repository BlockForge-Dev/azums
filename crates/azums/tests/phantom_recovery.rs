use azums::{quickstart, Job};
use serde_json::json;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn test_heartbeat_extends_lease_for_long_running_job() -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("w-hb")
        .with_lease_seconds(2)
        .with_queue("hb_queue");

    flow.register_handler("long_task", |_j| async move {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        Ok(())
    })
    .await;

    let job_id = flow
        .enqueue(Job::new("long_task", json!({})).queue("hb_queue"))
        .await?;

    let processed = flow.run_until_empty().await?;
    assert_eq!(processed, 1);

    let job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "succeeded");

    Ok(())
}

#[tokio::test]
async fn test_phantom_job_recovered_after_lease_expiry() -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("w-dead")
        .with_lease_seconds(1)
        .with_queue("phantom_queue");

    let job_id = flow
        .enqueue(Job::new("stranded_task", json!({})).queue("phantom_queue"))
        .await?;

    // Manually lease job simulating a worker that leased it and crashed without completing or heartbeating
    let batch = flow
        .backend()
        .lease_jobs_batch_with_ordering("phantom_queue", "w-dead", 1, 1, azums::QueueOrdering::Fifo)
        .await?;
    assert_eq!(batch.len(), 1);

    let leased_job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(leased_job.status, "running");

    // Wait for lease to expire (1s lease duration)
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // Trigger reclaimer sweeper
    let reaped = flow.backend().reap_expired_locks().await?;
    assert_eq!(reaped, 1);

    // Job should now be reset to 'queued' state
    let reclaimed_job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(reclaimed_job.status, "queued");
    assert!(reclaimed_job.locked_by.is_none());

    // Another worker picks it up
    flow.register_handler("stranded_task", |_j| async move { Ok(()) })
        .await;
    let processed = flow.run_until_empty().await?;
    assert_eq!(processed, 1);

    let final_job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(final_job.status, "succeeded");

    Ok(())
}

#[tokio::test]
async fn test_graceful_shutdown_with_cancellation_token() -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("w-shutdown")
        .with_queue("shutdown_queue");

    flow.register_handler("quick_job", |_j| async move { Ok(()) })
        .await;

    let token = CancellationToken::new();
    let token_clone = token.clone();
    let flow_clone = flow.clone();

    let worker_handle =
        tokio::spawn(async move { flow_clone.run_with_shutdown(token_clone).await });

    // Enqueue a job
    flow.enqueue(Job::new("quick_job", json!({})).queue("shutdown_queue"))
        .await?;

    // Allow worker to poll
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Trigger graceful shutdown
    token.cancel();

    let res: Result<(), anyhow::Error> = worker_handle.await?;
    assert!(res.is_ok());

    Ok(())
}
