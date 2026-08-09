//! PostgreSQL Hello World Example for azums.
//!
//! Run with: `DATABASE_URL=postgres://postgres:postgrespassword@localhost:5432/azums_dev cargo run --example hello_world_postgres`

use azums::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgrespassword@localhost:5432/azums_dev".to_string());

    println!("🚀 Connecting to PostgreSQL at: {}", db_url);

    // Connect to PostgreSQL (runs schema migrations automatically)
    let flow = match quickstart(&db_url).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("⚠️ Database connection skipped: {}", e);
            return Ok(());
        }
    };

    // Enqueue job
    let id = flow
        .enqueue(Job::new("send_email", serde_json::json!({"to": "user@example.com"})))
        .await?;
    println!("📥 Enqueued PostgreSQL job ID: {}", id);

    // Register handler
    flow.register_handler("send_email", |job| async move {
        println!("📧 Sent email to: {}", job.payload["to"]);
        Ok(())
    })
    .await;

    // Run until queue is empty
    let count = flow.run_until_empty().await?;
    println!("✅ Finished processing {} job(s) from PostgreSQL!", count);

    Ok(())
}
