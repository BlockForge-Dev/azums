use postgresflow::{quickstart, Job};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Tests the full quickstart flow: enqueue → register_handler → run_until_empty.
///
/// Uses the in-memory backend so this test works on all platforms without
/// requiring a running PostgreSQL instance.  Postgres-specific behaviour
/// (leasing, retries, DLQ, partitioning) is exercised by the other
/// integration tests that go through `setup_db()`.
#[tokio::test]
async fn test_quickstart_flow() {
    let flow = quickstart("memory")
        .await
        .expect("quickstart(memory) should succeed");

    let processed = Arc::new(AtomicBool::new(false));
    let processed_clone = processed.clone();

    flow.enqueue(Job::new("greet", serde_json::json!({"name": "World"})))
        .await
        .expect("enqueue should succeed");

    flow.register_handler("greet", move |job| {
        let processed = processed_clone.clone();
        async move {
            assert_eq!(job.payload["name"], "World");
            processed.store(true, Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let count = flow
        .run_until_empty()
        .await
        .expect("run_until_empty should succeed");

    assert_eq!(count, 1);
    assert!(processed.load(Ordering::SeqCst));
}
