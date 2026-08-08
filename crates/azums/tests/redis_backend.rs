use azums::{quickstart, Job};
use serde_json::json;

#[tokio::test]
async fn test_redis_backend_job_lifecycle_and_stream() -> anyhow::Result<()> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let client = match quickstart(&redis_url).await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Skipping live Redis test: No Redis server reachable at {redis_url}");
            return Ok(());
        }
    };

    // 1. Enqueue job
    let job_id = client
        .enqueue(Job::new("test_job", json!({"hello": "redis"})))
        .await?;
    assert!(!job_id.is_nil());

    // 2. Fetch job
    let fetched = client.backend().get_job(job_id).await?;
    assert!(fetched.is_some());
    let job = fetched.unwrap();
    assert_eq!(job.job_type, "test_job");
    assert_eq!(job.status, "queued");

    // 3. Lease job
    let leased = client
        .backend()
        .lease_jobs_batch("default", "worker_1", 30, 10)
        .await?;
    assert!(!leased.is_empty());

    let leased_job = leased.iter().find(|j| j.id == job_id);
    assert!(leased_job.is_some());

    // 4. Mark succeeded
    let attempt_ids = client
        .backend()
        .start_attempts_batch(&["default".to_string()], &[job_id], "worker_1")
        .await?;
    assert_eq!(attempt_ids.len(), 1);

    client
        .backend()
        .mark_succeeded(job_id, attempt_ids[0].1, "worker_1", 15)
        .await?;

    let final_job = client.backend().get_job(job_id).await?.unwrap();
    assert_eq!(final_job.status, "succeeded");

    // 5. Test Redis Stream pub/sub & replay
    let stream = client.stream("test_redis_stream");
    let seq1 = stream.publish("event_a", json!({"step": 1})).await?;
    let seq2 = stream.publish("event_b", json!({"step": 2})).await?;

    assert!(seq1 > 0);
    assert_eq!(seq2, seq1 + 1);

    let events = stream.read_events(0, 10).await?;
    assert!(events.len() >= 2);

    stream.ack("redis_group", seq2).await?;
    let group_info = stream.consumer_group_info().await?;
    assert!(!group_info.is_empty());

    Ok(())
}
