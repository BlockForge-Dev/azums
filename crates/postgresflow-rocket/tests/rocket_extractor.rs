use postgresflow_rocket::{BackgroundJobs, JobQueue};
use rocket::{http::Status, local::asynchronous::Client, post, routes, serde::json::Json};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[post("/enqueue", format = "json", data = "<payload>")]
async fn handle_enqueue(queue: JobQueue, payload: Json<Value>) -> Json<Value> {
    let job_id = queue
        .enqueue_now("default", "rocket_task", payload.into_inner())
        .await
        .unwrap();
    Json(json!({ "status": "queued", "id": job_id }))
}

#[tokio::test]
async fn test_rocket_job_queue_extractor() -> anyhow::Result<()> {
    let jobs = BackgroundJobs::from_url("memory").await?;

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    jobs.register_handler("rocket_task", move |job| {
        let ex = executed_clone.clone();
        async move {
            assert_eq!(job.payload["hello"], "rocket");
            ex.store(true, Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let rocket = rocket::build()
        .manage(jobs.clone())
        .mount("/", routes![handle_enqueue]);

    let client = Client::tracked(rocket).await?;
    let resp = client
        .post("/enqueue")
        .json(&json!({ "hello": "rocket" }))
        .dispatch()
        .await;

    assert_eq!(resp.status(), Status::Ok);

    let worker_handle = jobs.spawn_worker();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    worker_handle.abort();

    assert!(executed.load(Ordering::SeqCst));

    Ok(())
}
