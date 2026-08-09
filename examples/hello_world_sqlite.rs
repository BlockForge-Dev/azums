//! SQLite Embedded Hello World Example for azums.
//!
//! Run with: `cargo run --example hello_world_sqlite`

use azums::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let sqlite_url = "sqlite://jobs_demo.db?mode=rwc";

    println!("🚀 Initializing SQLite embedded queue at: {}", sqlite_url);

    let flow = quickstart(sqlite_url).await?;

    let id = flow
        .enqueue(Job::new("process_file", serde_json::json!({"path": "/tmp/data.csv"})))
        .await?;
    println!("📥 Enqueued SQLite job ID: {}", id);

    flow.register_handler("process_file", |job| async move {
        println!("📄 Processing file: {}", job.payload["path"]);
        Ok(())
    })
    .await;

    let count = flow.run_until_empty().await?;
    println!("✅ Finished processing {} job(s) from SQLite embedded queue!", count);

    // Clean up demo database file
    let _ = std::fs::remove_file("jobs_demo.db");
    let _ = std::fs::remove_file("jobs_demo.db-shm");
    let _ = std::fs::remove_file("jobs_demo.db-wal");

    Ok(())
}
