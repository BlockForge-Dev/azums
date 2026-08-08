use actix_web::{post, test, web, App, HttpResponse, Responder};
use azums_actix::{BackgroundJobs, JobQueue};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[post("/enqueue")]
async fn handle_enqueue(queue: JobQueue) -> impl Responder {
    let job_id = queue
        .enqueue_now("default", "actix_task", json!({"foo": "bar"}))
        .await
        .unwrap();
    HttpResponse::Ok().json(json!({ "status": "queued", "id": job_id }))
}

#[actix_web::test]
async fn test_actix_job_queue_extractor() -> anyhow::Result<()> {
    let jobs = BackgroundJobs::from_url("memory").await?;

    let executed = Arc::new(AtomicBool::new(false));
    let executed_clone = executed.clone();

    jobs.register_handler("actix_task", move |job| {
        let ex = executed_clone.clone();
        async move {
            assert_eq!(job.payload["foo"], "bar");
            ex.store(true, Ordering::SeqCst);
            Ok(())
        }
    })
    .await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(jobs.clone()))
            .service(handle_enqueue),
    )
    .await;

    let req = test::TestRequest::post().uri("/enqueue").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let worker_handle = jobs.spawn_worker();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    worker_handle.abort();

    assert!(executed.load(Ordering::SeqCst));

    Ok(())
}
