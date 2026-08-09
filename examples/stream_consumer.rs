//! Event Streaming Example for azums.
//!
//! Demonstrates producing and consuming durable event streams with offsets.
//!
//! Run with: `cargo run --example stream_consumer`

use azums::{quickstart, NewEvent};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting Azums Durable Event Streaming demo...");

    let client = quickstart("memory").await?;

    // 1. Subscribe to event stream "user_activity"
    let mut stream = client.backend().subscribe("user_activity").await?;

    // 2. Publish events
    client
        .backend()
        .publish("user_activity", NewEvent::new(serde_json::json!({"action": "login", "user_id": 42})))
        .await?;

    client
        .backend()
        .publish("user_activity", NewEvent::new(serde_json::json!({"action": "checkout", "item_id": 99})))
        .await?;

    // 3. Receive events
    if let Some(Ok(event)) = stream.next().await {
        println!("📡 Stream Received Event ID: {} | Action: {}", event.id, event.data["action"]);
    }
    if let Some(Ok(event)) = stream.next().await {
        println!("📡 Stream Received Event ID: {} | Action: {}", event.id, event.data["action"]);
    }

    println!("✅ Event Streaming demo completed!");
    Ok(())
}
