use async_trait::async_trait;
use azums::{quickstart, Client, Job, JobProcessor, NewJob};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct EmailPayload {
    to: String,
    subject: String,
}

struct EmailWorker {
    executed: Arc<AtomicBool>,
}

#[async_trait]
impl JobProcessor for EmailWorker {
    async fn process(&self, job: Job) -> anyhow::Result<()> {
        let payload: EmailPayload = job.payload_typed()?;
        assert_eq!(payload.to, "alice@example.com");
        assert_eq!(payload.subject, "Welcome!");
        self.executed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn test_payload_typed_and_client_processor() -> anyhow::Result<()> {
    let job = Job::new(
        "send_email",
        serde_json::json!({
            "to": "alice@example.com",
            "subject": "Welcome!"
        }),
    );

    // Verify payload_typed works directly on Job
    let payload: EmailPayload = job.payload_typed()?;
    assert_eq!(payload.to, "alice@example.com");
    assert_eq!(payload.subject, "Welcome!");

    // Verify Client alias and register_processor
    let client: Client = quickstart("memory").await?;
    let executed = Arc::new(AtomicBool::new(false));

    client
        .register_processor(
            "send_email",
            EmailWorker {
                executed: executed.clone(),
            },
        )
        .await;

    let _id = client.enqueue(job).await?;
    let batch_ids = client
        .enqueue_batch(vec![NewJob {
            queue: "default".into(),
            job_type: "send_email".into(),
            payload_json: serde_json::json!({"to": "alice@example.com", "subject": "Welcome!"}),
            run_at: chrono::Utc::now(),
            priority: 0,
            max_attempts: 5,
        }])
        .await?;

    assert_eq!(batch_ids.len(), 1);

    let processed = client.run_until_empty().await?;
    assert_eq!(processed, 2);
    assert!(executed.load(Ordering::SeqCst));

    client.shutdown().await?;

    Ok(())
}
