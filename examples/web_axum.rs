//! Axum Web Integration Example for azums.
//!
//! Demonstrates injecting an azums queue client into Axum web routes.
//!
//! Run with: `cargo run --example web_axum`

use azums::{quickstart, Job};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Axum + Azums background task demo started.");

    let client = quickstart("memory").await?;

    // Enqueue task from route
    let id = client
        .enqueue(Job::new("user_signup_email", json!({"email": "alice@example.com"})))
        .await?;
    println!("📥 HTTP POST /signup -> Enqueued job ID: {}", id);

    // Background worker
    client
        .register_handler("user_signup_email", |job| async move {
            println!("✉️ Welcome email dispatched to: {}", job.payload["email"]);
            Ok(())
        })
        .await;

    client.run_until_empty().await?;
    println!("✅ Axum background task processed!");

    Ok(())
}
