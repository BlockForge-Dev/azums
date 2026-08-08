use azums::{quickstart, Job};
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_instant_wakeup_event_driven() -> anyhow::Result<()> {
    let flow = Arc::new(quickstart("memory").await?);
    let (tx, rx) = tokio::sync::oneshot::channel::<Instant>();

    flow.register_handler("instant_job", move |_job| async move { Ok(()) })
        .await;

    let worker_flow = flow.clone();
    let worker_handle = tokio::spawn(async move {
        let mut stream = worker_flow.backend().subscribe("default").await.unwrap();
        loop {
            let batch = worker_flow
                .backend()
                .lease_jobs_batch("default", "test-worker", 10, 10)
                .await
                .unwrap();

            if batch.is_empty() {
                tokio::select! {
                    _ = stream.next() => {},
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {},
                }
                continue;
            }

            let received_at = Instant::now();
            let _ = tx.send(received_at);
            break;
        }
    });

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let enqueued_at = Instant::now();
    flow.enqueue(Job::new("instant_job", json!({"test": "value"})))
        .await?;

    let received_at = rx.await?;
    let wake_latency = received_at.duration_since(enqueued_at);
    println!("Notification wake-up latency: {:?}", wake_latency);

    assert!(wake_latency < std::time::Duration::from_millis(50));
    let _ = worker_handle.await;
    Ok(())
}

#[tokio::test]
async fn test_sqlite_instant_wakeup() -> anyhow::Result<()> {
    let temp_db = format!(
        "sqlite://file:test_wakeup_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let flow = Arc::new(quickstart(&temp_db).await?);

    let enqueued_at = Instant::now();
    flow.enqueue(Job::new("sqlite_wakeup", json!({}))).await?;

    flow.register_handler("sqlite_wakeup", |_job| async move { Ok(()) })
        .await;

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 1);
    assert!(enqueued_at.elapsed() < std::time::Duration::from_millis(100));

    Ok(())
}
