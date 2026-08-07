use postgresflow::{Job, MockBackend, QuickstartFlow, StorageBackend};
use std::sync::Arc;

#[tokio::test]
async fn test_mock_backend_call_recording() -> anyhow::Result<()> {
    let mock = MockBackend::with_memory();
    let backend: Arc<dyn StorageBackend> = Arc::new(mock.clone());

    let flow = QuickstartFlow::new(backend);

    let _id = flow
        .enqueue(Job::new(
            "send_notification",
            serde_json::json!({"user": "alice"}),
        ))
        .await?;

    mock.assert_enqueued_job_type("send_notification");

    let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executed_clone = executed.clone();

    flow.register_handler("send_notification", move |_job| {
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

    let calls = mock.calls();
    assert!(!calls.is_empty());
    println!("Recorded calls in MockBackend: {calls:?}");

    mock.clear_calls();
    assert!(mock.calls().is_empty());

    Ok(())
}
