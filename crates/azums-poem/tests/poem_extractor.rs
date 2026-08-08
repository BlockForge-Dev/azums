use azums_poem::{BackgroundJobs, JobQueue};
use poem::{handler, test::TestClient, web::Json, EndpointExt, Route};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[handler]
async fn handle_enqueue(queue: JobQueue) -> Json<serde_json::Value> {
    let job_id = queue
        .enqueue_now("default", "poem_task", json!({"ping": "pong"}))
        .await
        .unwrap();
    Json(json!({ "status": "queued", "id": job_id }))
}

#[tokio::test]
async fn test_poem_job_queue_extractor() -> anyhow::Result<()> {
    let jobs = BackgroundJobs::from_url("memory").await?;

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    jobs.register_handler("poem_task", move |job| {
        let ex = executed_clone.clone();
        async move {
            assert_eq!(job.payload["ping"], "pong");
            ex.store(true, Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let app = Route::new()
        .at("/enqueue", poem::post(handle_enqueue))
        .data(jobs.clone());

    let cli = TestClient::new(app);
    let resp = cli.post("/enqueue").send().await;
    resp.assert_status_is_ok();

    let worker_handle = jobs.spawn_worker();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    worker_handle.abort();

    assert!(executed.load(Ordering::SeqCst));

    Ok(())
}
