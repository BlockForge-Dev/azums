use azums::{quickstart, Job, MemoryBackend, StorageBackend};
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, TimeZone, Utc};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn m9_scheduled_job_never_leases_before_run_at_and_leases_after_pause() -> anyhow::Result<()>
{
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let run_at = Utc::now() + ChronoDuration::milliseconds(250);
    let job_id = backend
        .enqueue(Job::new("m9_scheduled", json!({})).run_at(run_at).into())
        .await?;

    assert!(
        backend
            .lease_jobs_batch("default", "m9-worker", 30, 1)
            .await?
            .is_empty(),
        "scheduled jobs must not lease before run_at"
    );

    tokio::time::sleep(Duration::from_millis(350)).await;

    let leased = backend
        .lease_jobs_batch("default", "m9-worker", 30, 1)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);
    assert!(
        Utc::now() >= leased[0].run_at,
        "jobs leased after downtime/pauses must be eligible by documented time"
    );

    Ok(())
}

#[tokio::test]
async fn m9_past_scheduled_jobs_after_downtime_are_immediately_eligible() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let past = Utc::now() - ChronoDuration::hours(6);
    let job_id = backend
        .enqueue(Job::new("m9_after_downtime", json!({})).run_at(past).into())
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "restart-worker", 30, 1)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);

    Ok(())
}

#[tokio::test]
async fn m9_expired_deadline_jobs_dlq_instead_of_running_late() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let now = Utc::now();
    let job_id = backend
        .enqueue(
            Job::new("m9_deadline", json!({}))
                .run_at(now - ChronoDuration::minutes(5))
                .deadline_at(now - ChronoDuration::minutes(1))
                .into(),
        )
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "deadline-worker", 30, 1)
        .await?;
    assert!(leased.is_empty());

    let job = backend
        .get_job(job_id)
        .await?
        .expect("job remains inspectable");
    assert_eq!(job.status, "dlq");
    assert_eq!(job.dlq_reason_code.as_deref(), Some("DEADLINE_EXCEEDED"));

    Ok(())
}

#[tokio::test]
async fn m9_attempt_timeout_is_classified_and_rescheduled() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?.with_worker_id("timeout-worker");
    let executions = Arc::new(AtomicUsize::new(0));
    let executions_clone = executions.clone();

    flow.register_handler("m9_timeout", move |_job| {
        let executions = executions_clone.clone();
        async move {
            executions.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        }
    })
    .await;

    let job_id = flow
        .enqueue(
            Job::new("m9_timeout", json!({}))
                .timeout_seconds(0)
                .max_attempts(2),
        )
        .await?;

    let processed = flow.run_until_empty().await?;
    assert_eq!(processed, 1);
    assert_eq!(executions.load(Ordering::SeqCst), 1);

    let job = flow.backend().get_job(job_id).await?.expect("job exists");
    assert_eq!(job.status, "queued");
    assert!(job.run_at >= Utc::now());

    Ok(())
}

#[tokio::test]
async fn m9_recurring_success_enqueues_next_occurrence_from_previous_run_at() -> anyhow::Result<()>
{
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let first_run_at = Utc::now() - ChronoDuration::minutes(5);
    let job_id = backend
        .enqueue(
            Job::new("m9_recurring", json!({"tick": 1}))
                .run_at(first_run_at)
                .deadline_at(first_run_at + ChronoDuration::minutes(30))
                .recurring_interval_seconds(60)
                .into(),
        )
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "recurring-worker", 30, 1)
        .await?;
    let attempts = backend
        .start_attempts_batch(
            &[leased[0].dataset_id.clone()],
            &[job_id],
            "recurring-worker",
        )
        .await?;
    backend
        .mark_succeeded(job_id, attempts[0].1, "recurring-worker", 5)
        .await?;

    let queued = backend
        .list_jobs(None, Some("queued"), 10, None, None)
        .await?;
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].run_at, first_run_at + ChronoDuration::seconds(60));
    assert_eq!(
        queued[0].deadline_at,
        Some(first_run_at + ChronoDuration::minutes(31))
    );
    assert_eq!(queued[0].recurring_interval_seconds, Some(60));

    Ok(())
}

#[test]
fn m9_daylight_saving_boundaries_are_resolved_before_utc_run_at() {
    let ny = FixedOffset::west_opt(5 * 3600).unwrap();
    let before_spring_forward: DateTime<Utc> =
        ny.with_ymd_and_hms(2026, 3, 8, 1, 30, 0).unwrap().into();

    let job = Job::new("m9_dst", json!({})).run_at(before_spring_forward);
    assert_eq!(job.run_at.timezone(), Utc);
    assert_eq!(job.run_at, before_spring_forward);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m9_clock_skew_cannot_lease_before_persisted_run_at() -> anyhow::Result<()> {
    let backend = Arc::new(MemoryBackend::new());
    backend.run_migrations().await?;

    let run_at = Utc::now() + ChronoDuration::milliseconds(300);
    backend
        .enqueue(Job::new("m9_skew", json!({})).run_at(run_at).into())
        .await?;

    let early_claims = Arc::new(Mutex::new(Vec::new()));
    let mut tasks = Vec::new();
    for worker_idx in 0..8 {
        let backend = backend.clone();
        let early_claims = early_claims.clone();
        tasks.push(tokio::spawn(async move {
            let worker = format!("skewed-worker-{worker_idx}");
            let leased = backend
                .lease_jobs_batch("default", &worker, 30, 1)
                .await
                .unwrap();
            if !leased.is_empty() {
                early_claims.lock().unwrap().push(worker);
            }
        }));
    }

    for task in tasks {
        task.await?;
    }

    assert!(
        early_claims.lock().unwrap().is_empty(),
        "worker-side clock skew must not bypass backend eligibility"
    );

    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        backend
            .lease_jobs_batch("default", "eligible-worker", 30, 1)
            .await?
            .len(),
        1
    );

    Ok(())
}
