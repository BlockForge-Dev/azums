use axum::{routing::post, Json, Router};
use azums_axum::{BackgroundJobs, JobQueue};
use serde_json::json;
use tower::ServiceExt;

async fn test_handler(
    queue: JobQueue,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (http::StatusCode, String)> {
    let job_id = queue
        .enqueue_now("default", "test_job", payload)
        .await
        .map_err(|e| (http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "queued", "id": job_id })))
}

#[tokio::test]
async fn test_axum_job_queue_extractor() -> anyhow::Result<()> {
    let jobs = BackgroundJobs::from_url("memory").await?;

    let executed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let executed_clone = executed.clone();

    jobs.register_handler("test_job", move |job| {
        let ex = executed_clone.clone();
        async move {
            assert_eq!(job.payload["foo"], "bar");
            ex.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let app = Router::new()
        .route("/enqueue", post(test_handler))
        .with_state(jobs.clone());

    let req = http::Request::builder()
        .method("POST")
        .uri("/enqueue")
        .header("content-type", "application/json")
        .body(axum::body::Body::from(json!({ "foo": "bar" }).to_string()))?;

    let response = app.oneshot(req).await?;
    assert_eq!(response.status(), http::StatusCode::OK);

    let worker_handle = jobs.spawn_worker();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    worker_handle.abort();

    assert!(executed.load(std::sync::atomic::Ordering::SeqCst));

    Ok(())
}
