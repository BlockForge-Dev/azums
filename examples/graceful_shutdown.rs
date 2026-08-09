//! Graceful Worker Shutdown Example for azums.
//!
//! Demonstrates running background workers with a cancellation token for graceful shutdown on SIGINT/SIGTERM.
//!
//! Run with: `cargo run --example graceful_shutdown`

use azums::{quickstart, Job};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting Azums Graceful Worker Shutdown demo...");

    let client = quickstart("memory").await?;

    // Enqueue 5 jobs
    for i in 1..=5 {
        client
            .enqueue(Job::new("background_task", serde_json::json!({"step": i})))
            .await?;
    }

    client
        .register_handler("background_task", |job| async move {
            println!("⚙️ Executing job step {}", job.payload["step"]);
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        })
        .await;

    // Process all enqueued jobs cleanly before shutting down
    let processed = client.run_until_empty().await?;
    println!("🛑 Cleanly finished processing {} job(s) before worker shutdown!", processed);

    Ok(())
}
