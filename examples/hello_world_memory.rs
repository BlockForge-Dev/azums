//! In-Memory Hello World Example for azums.
//!
//! Run with: `cargo run --example hello_world_memory`

use azums::{quickstart, Job};
use serde::Deserialize;

#[derive(Deserialize)]
struct GreetPayload {
    name: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting Azums In-Memory Quickstart...");

    // 1. Initialize In-Memory client flow
    let client = quickstart("memory").await?;

    // 2. Enqueue background job
    let job_id = client
        .enqueue(Job::new("greet", serde_json::json!({"name": "World"})))
        .await?;
    println!("📥 Enqueued job ID: {}", job_id);

    // 3. Register job processing handler
    client
        .register_handler("greet", |job| async move {
            let payload: GreetPayload = job.payload_typed()?;
            println!("🎉 Executed job handler for: {}", payload.name);
            Ok(())
        })
        .await;

    // 4. Run worker loop until queue is empty
    let processed = client.run_until_empty().await?;
    println!("✅ Processed {} job(s) successfully!", processed);

    Ok(())
}
