mod common;

use azums::{
    jobs::{
        retry::{classify_error, next_delay_seconds, parse_handler_error, ErrorClass, RetryConfig},
        timeline::build_timeline,
        AttemptsRepo, JobsRepo, PolicyDecisionsRepo,
    },
    quickstart, Job,
};
use rand::{rngs::StdRng, SeedableRng};
use serde_json::json;
use uuid::Uuid;

#[test]
fn failure_classification_covers_m6_error_classes() {
    assert_eq!(classify_error("HANDLER_ERROR"), ErrorClass::Retryable);
    assert_eq!(classify_error("TIMEOUT"), ErrorClass::Timeout);
    assert_eq!(classify_error("BAD_PAYLOAD"), ErrorClass::Permanent);
    assert_eq!(classify_error("PANIC"), ErrorClass::Panic);
    assert_eq!(classify_error("CANCELLED"), ErrorClass::Cancelled);
    assert_eq!(classify_error("SYSTEM_FAILURE"), ErrorClass::SystemFailure);

    assert!(classify_error("TIMEOUT").is_retryable());
    assert!(classify_error("SYSTEM_FAILURE").is_retryable());
    assert!(!classify_error("BAD_PAYLOAD").is_retryable());
    assert_eq!(
        classify_error("BAD_PAYLOAD").dlq_reason_code(),
        "PERMANENT_ERROR"
    );
}

#[test]
fn retry_backoff_is_deterministic_without_jitter() {
    let cfg = RetryConfig {
        base_seconds: 1,
        max_seconds: 16,
        jitter_pct: 0.0,
    };
    let mut rng = StdRng::seed_from_u64(42);

    let delays: Vec<i64> = (1..=6)
        .map(|attempt_no| next_delay_seconds(attempt_no, &cfg, &mut rng))
        .collect();

    assert_eq!(delays, vec![1, 2, 4, 8, 16, 16]);
}

#[test]
fn handler_error_parser_accepts_typed_failure_prefixes() {
    assert_eq!(
        parse_handler_error("TIMEOUT: dependency took too long"),
        ("TIMEOUT", "dependency took too long")
    );
    assert_eq!(
        parse_handler_error("permanent: invalid business key"),
        ("PERMANENT_ERROR", "invalid business key")
    );
    assert_eq!(
        parse_handler_error("ordinary failure"),
        ("HANDLER_ERROR", "ordinary failure")
    );
}

async fn insert_dlq_subject(pool: &sqlx::PgPool) -> anyhow::Result<Uuid> {
    let job_id = sqlx::query_scalar(
        r#"
        INSERT INTO jobs (
            queue, job_type, payload_json, run_at, status, priority, max_attempts
        )
        VALUES (
            'm6', 'send_email',
            '{"payload":{"to":"user@example.com"},"metadata":{"request_id":"req_123"}}'::jsonb,
            now(), 'queued', 7, 2
        )
        RETURNING id
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(job_id)
}

#[tokio::test]
async fn dlq_preserves_original_job_attempts_worker_errors_and_replay() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };

    let jobs = JobsRepo::new(pool.clone());
    let attempts = AttemptsRepo::new(pool.clone());
    let policies = PolicyDecisionsRepo::new(pool.clone());
    let job_id = insert_dlq_subject(&pool).await?;

    let job = jobs
        .lease_one_job("m6", "worker-a", 30)
        .await?
        .expect("first attempt should lease");
    let attempt_1 = attempts.start_attempt(job_id, "worker-a").await?;
    jobs.reschedule_for_retry(
        job_id,
        chrono::Utc::now() + chrono::Duration::seconds(1),
        Some("TIMEOUT"),
        Some("dependency timeout"),
    )
    .await?;
    attempts
        .finish_failed(attempt_1.id, 12, "TIMEOUT", "dependency timeout")
        .await?;

    sqlx::query("UPDATE jobs SET run_at = now() WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await?;

    let job_2 = jobs
        .lease_one_job("m6", "worker-b", 30)
        .await?
        .expect("second attempt should lease");
    assert_eq!(job_2.id, job.id);
    let attempt_2 = attempts.start_attempt(job_id, "worker-b").await?;
    attempts
        .finish_failed(attempt_2.id, 34, "TIMEOUT", "dependency timeout again")
        .await?;
    jobs.mark_dlq(
        job_id,
        "worker-b",
        "MAX_ATTEMPTS_EXCEEDED",
        Some("TIMEOUT"),
        Some("dependency timeout again"),
    )
    .await?;

    let dlq_job = jobs.get_job(job_id).await?.expect("job should exist");
    assert_eq!(dlq_job.status, "dlq");
    assert_eq!(
        dlq_job.payload["metadata"]["request_id"].as_str(),
        Some("req_123")
    );
    assert_eq!(dlq_job.priority, 7);
    assert_eq!(dlq_job.max_attempts, 2);
    assert_eq!(
        dlq_job.dlq_reason_code.as_deref(),
        Some("MAX_ATTEMPTS_EXCEEDED")
    );
    assert!(dlq_job.dlq_at.is_some());

    let history = attempts.list_attempts_for_job(job_id).await?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].worker_id, "worker-a");
    assert_eq!(history[1].worker_id, "worker-b");
    assert_eq!(history[0].error_code.as_deref(), Some("TIMEOUT"));
    assert_eq!(
        history[1].error_message.as_deref(),
        Some("dependency timeout again")
    );
    assert!(history.iter().all(|attempt| attempt.finished_at.is_some()));

    let timeline = build_timeline(&jobs, &attempts, &policies, job_id)
        .await?
        .expect("timeline should build");
    assert_eq!(timeline.status, "dlq");
    assert_eq!(timeline.last_worker_id.as_deref(), Some("worker-b"));
    assert_eq!(
        timeline.last_error.unwrap().error_code.as_deref(),
        Some("TIMEOUT")
    );
    assert_eq!(timeline.attempts.len(), 2);

    let replayed_id = jobs.replay_job(job_id, Some("m6-replay"), None).await?;
    let replayed = jobs
        .get_job(replayed_id)
        .await?
        .expect("replay should exist");
    assert_eq!(replayed.status, "queued");
    assert_eq!(replayed.replay_of_job_id, Some(job_id));
    assert_eq!(replayed.payload, dlq_job.payload);
    assert_eq!(replayed.queue, "m6-replay");

    Ok(())
}

#[tokio::test]
async fn quickstart_typed_permanent_error_routes_to_dlq() -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("m6-worker")
        .with_queue("m6-memory");

    flow.register_handler("bad_payload", |_job| async move {
        anyhow::bail!("BAD_PAYLOAD: missing required field")
    })
    .await;

    let job_id = flow
        .enqueue(
            Job::new("bad_payload", json!({"metadata": {"source": "test"}})).queue("m6-memory"),
        )
        .await?;

    assert_eq!(flow.run_until_empty().await?, 1);
    let job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "dlq");
    assert_eq!(job.dlq_reason_code.as_deref(), Some("PERMANENT_ERROR"));

    let replayed_id = flow.backend().replay_job(job_id, None, None).await?;
    let replayed = flow.backend().get_job(replayed_id).await?.unwrap();
    assert_eq!(replayed.status, "queued");
    assert_eq!(replayed.replay_of_job_id, Some(job_id));
    assert_eq!(replayed.payload, job.payload);

    Ok(())
}
