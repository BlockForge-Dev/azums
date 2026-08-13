use azums::{quickstart, Job};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn m16_install_enqueue_process_retry_inspect_path_is_one_client() -> anyhow::Result<()> {
    let queue = quickstart("memory")
        .await?
        .with_queue("m16-onboarding")
        .with_worker_id("m16-worker");

    assert_eq!(queue.queue(), "m16-onboarding");
    assert_eq!(queue.worker_id(), "m16-worker");
    assert!(queue.capabilities().supports_portable_job_api());

    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    queue
        .register_handler("welcome_email", move |job| {
            let handler_calls = handler_calls.clone();
            async move {
                assert_eq!(job.payload["email"], "new@example.com");
                let call_no = handler_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if call_no == 1 {
                    anyhow::bail!("SYSTEM_FAILURE: transient smtp outage");
                }
                Ok(())
            }
        })
        .await;

    let job_id = queue
        .enqueue(
            Job::new("welcome_email", json!({ "email": "new@example.com" }))
                .queue(queue.queue())
                .max_attempts(3),
        )
        .await?;

    assert_eq!(queue.run_until_empty().await?, 1);
    let retrying = queue
        .get_job(job_id)
        .await?
        .expect("job remains inspectable");
    assert_eq!(retrying.status, "queued");
    assert!(
        retrying.run_at > chrono::Utc::now(),
        "retry should schedule the job for later"
    );

    tokio::time::sleep(std::time::Duration::from_millis(2_300)).await;
    assert_eq!(queue.run_until_empty().await?, 1);

    let completed = queue
        .get_job(job_id)
        .await?
        .expect("completed job remains inspectable");
    assert_eq!(completed.status, "succeeded");
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let replayed_id = queue.replay_job(job_id).await?;
    let replayed = queue
        .get_job(replayed_id)
        .await?
        .expect("replayed job is inspectable");
    assert_eq!(replayed.status, "queued");
    assert_eq!(replayed.replay_of_job_id, Some(job_id));

    let stream = queue.stream("m16-events");
    let seq = stream
        .publish("job_replayed", json!({ "job_id": replayed_id }))
        .await?;
    let next = stream.read_next("docs-reader", 10).await?;
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].sequence_no, seq);
    stream.ack("docs-reader", seq).await?;
    assert!(stream.read_next("docs-reader", 10).await?.is_empty());

    Ok(())
}
