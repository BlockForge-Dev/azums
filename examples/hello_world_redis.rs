//! Redis Queue & Stream Hello World Example for azums.
//!
//! Run with: `REDIS_URL=redis://127.0.0.1:6379 cargo run --example hello_world_redis`

use azums::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    println!("🚀 Connecting to Redis at: {}", redis_url);

    let flow = match quickstart(&redis_url).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("⚠️ Redis connection skipped: {}", e);
            return Ok(());
        }
    };

    let id = flow
        .enqueue(Job::new("cache_flush", serde_json::json!({"pattern": "user:*"})))
        .await?;
    println!("📥 Enqueued Redis job ID: {}", id);

    flow.register_handler("cache_flush", |job| async move {
        println!("🧹 Flushing cache pattern: {}", job.payload["pattern"]);
        Ok(())
    })
    .await;

    let count = flow.run_until_empty().await?;
    println!("✅ Finished processing {} job(s) from Redis!", count);

    Ok(())
}
