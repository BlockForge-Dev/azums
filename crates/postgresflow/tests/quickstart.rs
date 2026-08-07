use postgresflow::{quickstart, Job};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn test_quickstart_flow() {
    let Ok(test_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping test_quickstart_flow");
        return;
    };

    let flow = quickstart(&test_url)
        .await
        .expect("quickstart should connect and run migrations");

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
