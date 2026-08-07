use async_trait::async_trait;
use postgresflow::{quickstart, Client, Job, JobProcessor, NewJob};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
struct OrderNotification {
    order_id: String,
    customer_email: String,
    total_cents: u64,
}

struct OrderProcessor {
    attempt_counter: Arc<AtomicU32>,
}

#[async_trait]
impl JobProcessor for OrderProcessor {
    async fn process(&self, job: Job) -> anyhow::Result<()> {
        let payload: OrderNotification = job.payload_typed()?;
        let attempts = self.attempt_counter.fetch_add(1, Ordering::SeqCst) + 1;

        println!(
            "📦 [Attempt #{attempts}] Processing order {} for {} (${:.2})",
            payload.order_id,
            payload.customer_email,
            payload.total_cents as f64 / 100.0
        );

        if attempts < 2 {
            anyhow::bail!("Simulated transient gateway connection timeout");
        }

        println!("✅ Order {} processed successfully!", payload.order_id);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Starting PostgresFlow Full-Features Demonstration");

    // 1. Initialize Client using in-memory backend
    let client: Client = quickstart("memory").await?;

    // 2. Register trait-based JobProcessor
    let attempt_counter = Arc::new(AtomicU32::new(0));
    client
        .register_processor(
            "process_order",
            OrderProcessor {
                attempt_counter: attempt_counter.clone(),
            },
        )
        .await;

    // 3. Register closure-based handler for welcome email
    client
        .register_handler("welcome_email", |job| async move {
            println!("📧 Sending welcome email to {}", job.payload["email"]);
            Ok(())
        })
        .await;

    // 4. Enqueue immediate job
    let single_id = client
        .enqueue(Job::new(
            "process_order",
            json!({
                "order_id": "ord_9981",
                "customer_email": "user@example.com",
                "total_cents": 4999
            }),
        ))
        .await?;
    println!("Single job enqueued: {single_id}");

    // 5. Enqueue batch of jobs
    let batch_ids = client
        .enqueue_batch(vec![NewJob {
            queue: "default".into(),
            job_type: "welcome_email".into(),
            payload_json: json!({"email": "newuser@example.com"}),
            run_at: chrono::Utc::now(),
            priority: 10,
            max_attempts: 3,
        }])
        .await?;
    println!("Batch jobs enqueued: {batch_ids:?}");

    // 6. Process jobs in worker queue until empty
    let processed_count = client.run_until_empty().await?;
    println!("🎉 All jobs completed! Total processed: {processed_count}");

    // 7. Graceful shutdown
    client.shutdown().await?;
    println!("👋 PostgresFlow shutdown complete.");

    Ok(())
}
