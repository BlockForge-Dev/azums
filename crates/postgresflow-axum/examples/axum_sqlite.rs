use axum::{routing::post, Json, Router};
use postgresflow_axum::{BackgroundJobs, JobQueue};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
struct CreateOrderRequest {
    customer_email: String,
    total_amount_cents: i64,
}

#[derive(Serialize)]
struct CreateOrderResponse {
    status: String,
    job_id: String,
}

async fn handle_create_order(
    queue: JobQueue,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, (http::StatusCode, String)> {
    println!(
        "Received order request for {} (${:.2})",
        payload.customer_email,
        payload.total_amount_cents as f64 / 100.0
    );

    let job_id = queue
        .enqueue_now(
            "orders",
            "order_processing",
            json!({
                "email": payload.customer_email,
                "amount": payload.total_amount_cents
            }),
        )
        .await
        .map_err(|e| (http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(CreateOrderResponse {
        status: "queued".to_string(),
        job_id: job_id.to_string(),
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = "sqlite://axum_orders.db?mode=rwc";
    println!("Connecting BackgroundJobs to embedded SQLite at: {db_path}");

    let jobs = BackgroundJobs::from_url(db_path).await?;

    // Register async job handler
    jobs.register_handler("order_processing", |job| async move {
        println!(
            "📦 Background Worker Processing Order for {}: amount=${:.2}",
            job.payload["email"],
            job.payload["amount"].as_f64().unwrap_or(0.0) / 100.0
        );
        Ok(())
    })
    .await;

    // Spawn background worker loop into Tokio runtime
    jobs.spawn_worker();

    // Construct Axum application with BackgroundJobs state
    let app = Router::new()
        .route("/orders", post(handle_create_order))
        .with_state(jobs);

    let addr = "127.0.0.1:3000";
    println!("🚀 Axum web server running on http://{addr}");
    println!("Try: curl -X POST http://{addr}/orders -H 'Content-Type: application/json' -d '{{\"customer_email\":\"user@example.com\",\"total_amount_cents\":4999}}'");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    // Clean up sqlite files on exit
    let _ = std::fs::remove_file("axum_orders.db");
    let _ = std::fs::remove_file("axum_orders.db-wal");
    let _ = std::fs::remove_file("axum_orders.db-shm");

    Ok(())
}
