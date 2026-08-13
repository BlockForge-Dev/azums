use azums::{quickstart, Job};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn m17_failed_jobs_are_explainable_without_source_code() -> anyhow::Result<()> {
    let queue = quickstart("memory")
        .await?
        .with_queue("m17-observe")
        .with_worker_id("m17-worker");

    let calls = Arc::new(AtomicUsize::new(0));
    let handler_calls = calls.clone();
    queue
        .register_handler("observable_email", move |_job| {
            let handler_calls = handler_calls.clone();
            async move {
                let call_no = handler_calls.fetch_add(1, Ordering::SeqCst) + 1;
                if call_no == 1 {
                    anyhow::bail!("SYSTEM_FAILURE: transient smtp outage");
                }
                Ok(())
            }
        })
        .await;

    let job_id = queue
        .enqueue(
            Job::new(
                "observable_email",
                json!({
                    "email": "ops@example.com",
                    "trace_id": "trace-m17"
                }),
            )
            .queue(queue.queue())
            .max_attempts(3),
        )
        .await?;

    assert_eq!(queue.run_until_empty().await?, 1);

    let retrying = queue
        .explain_job(job_id)
        .await?
        .expect("retrying job is explainable");
    assert_eq!(retrying.job_id, job_id);
    assert_eq!(retrying.queue, "m17-observe");
    assert_eq!(retrying.retry_count, 1);
    assert_eq!(retrying.last_worker_id.as_deref(), Some("m17-worker"));
    assert_eq!(retrying.trace_id.as_deref(), Some("trace-m17"));
    assert!(retrying
        .last_error
        .as_deref()
        .unwrap_or_default()
        .contains("SYSTEM_FAILURE"));

    let failed_attempt = retrying
        .events
        .iter()
        .find(|event| event.attempt == Some(1))
        .expect("attempt event is durable");
    assert_eq!(failed_attempt.worker_id.as_deref(), Some("m17-worker"));
    assert_eq!(failed_attempt.status, "failed");
    assert!(failed_attempt.duration_ms.is_some());
    assert_eq!(
        failed_attempt.span_attributes().get("azums.job_id"),
        Some(&job_id.to_string())
    );
    assert_eq!(
        failed_attempt.span_attributes().get("trace_id"),
        Some(&"trace-m17".to_string())
    );

    let log_event = queue
        .job_log_event(job_id)
        .await?
        .expect("structured log event exists");
    assert_eq!(log_event["job_id"], job_id.to_string());
    assert_eq!(log_event["attempt"], 1);
    assert_eq!(log_event["worker_id"], "m17-worker");
    assert_eq!(log_event["queue"], "m17-observe");
    assert_eq!(log_event["status"], "queued");
    assert_eq!(log_event["retry_count"], 1);
    assert_eq!(log_event["trace_id"], "trace-m17");
    assert!(log_event["error"]
        .as_str()
        .unwrap_or_default()
        .contains("SYSTEM_FAILURE"));

    let retry_metrics = queue.queue_metrics(Some("m17-observe")).await?;
    let retry_metrics = retry_metrics
        .iter()
        .find(|row| row.queue == "m17-observe")
        .expect("queue metrics include target queue");
    assert_eq!(retry_metrics.jobs_total, 1);
    assert_eq!(retry_metrics.jobs_failed, 1);
    assert_eq!(retry_metrics.jobs_retried, 1);
    assert!(retry_metrics.execution_latency_ms_avg >= 0.0);

    tokio::time::sleep(std::time::Duration::from_millis(2_300)).await;
    assert_eq!(queue.run_until_empty().await?, 1);

    let completed = queue
        .explain_job(job_id)
        .await?
        .expect("completed job remains explainable");
    assert_eq!(completed.status, "succeeded");
    assert_eq!(completed.retry_count, 1);
    assert!(completed.summary.contains("completed after 2 attempt"));

    queue
        .register_handler("bad_payload", |_job| async move {
            anyhow::bail!("BAD_PAYLOAD: missing required email")
        })
        .await;
    let dlq_job_id = queue
        .enqueue(
            Job::new("bad_payload", json!({ "trace_id": "trace-dlq" }))
                .queue(queue.queue())
                .max_attempts(1),
        )
        .await?;
    assert_eq!(queue.run_until_empty().await?, 1);

    let dlq = queue
        .explain_job(dlq_job_id)
        .await?
        .expect("DLQ job is explainable");
    assert_eq!(dlq.status, "dlq");
    assert!(dlq.summary.contains("DLQ"));
    assert!(dlq.last_error.unwrap_or_default().contains("BAD_PAYLOAD"));

    let metrics = queue.queue_metrics(Some("m17-observe")).await?;
    let metrics = metrics
        .iter()
        .find(|row| row.queue == "m17-observe")
        .expect("queue metrics include target queue");
    assert_eq!(metrics.jobs_total, 2);
    assert_eq!(metrics.jobs_completed, 1);
    assert_eq!(metrics.jobs_dlq, 1);

    Ok(())
}
