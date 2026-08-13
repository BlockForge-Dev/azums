use azums::{quickstart, Job};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let queue = quickstart("memory").await?.with_queue("welcome");

    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    queue
        .register_handler("send_welcome_email", move |job| {
            let handler_calls = handler_calls.clone();
            async move {
                let call_no = handler_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if call_no == 1 {
                    anyhow::bail!("SYSTEM_FAILURE: pretend the email API was down");
                }

                println!("Welcome email sent to {}", job.payload["email"]);
                Ok(())
            }
        })
        .await;

    let job_id = queue
        .enqueue(
            Job::new("send_welcome_email", json!({ "email": "new@example.com" }))
                .queue(queue.queue())
                .max_attempts(3),
        )
        .await?;

    queue.run_until_empty().await?;
    println!("After first run: {:?}", queue.get_job(job_id).await?);

    tokio::time::sleep(std::time::Duration::from_millis(2_300)).await;
    queue.run_until_empty().await?;
    println!("After retry: {:?}", queue.get_job(job_id).await?);

    Ok(())
}
