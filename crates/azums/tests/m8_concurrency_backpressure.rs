mod common;

use azums::{
    BackendCapabilities, BackpressureCapability, Job, MemoryBackend, OrderingCapability,
    PolicyDecisionsRepo, StorageBackend,
};
use chrono::Utc;
use serial_test::serial;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use uuid::Uuid;

const M8_WORKER_MATRIX: &[usize] = &[1, 2, 5, 10, 50, 100];

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn m8_worker_matrix_has_no_invalid_claims_or_duplicate_completions() -> anyhow::Result<()> {
    let jobs_per_case = std::env::var("AZUMS_M8_CI_JOBS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1_000);

    for &worker_count in M8_WORKER_MATRIX {
        run_memory_worker_matrix_case(worker_count, jobs_per_case).await?;
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn m8_default_overload_behavior_is_backlog_without_shedding() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let total_jobs = 10_000usize;
    for seq in 0..total_jobs {
        backend
            .enqueue(Job::new("m8_backlog", serde_json::json!({ "seq": seq })).into())
            .await?;
    }

    let leased = backend
        .lease_jobs_batch("m8-overload", "m8-worker", 30, 1_000)
        .await?;
    assert!(leased.is_empty());

    let leased = backend
        .lease_jobs_batch("default", "m8-worker", 30, 1_000)
        .await?;
    assert_eq!(leased.len(), 1_000);

    for job in leased {
        let attempts = backend
            .start_attempts_batch(&[job.dataset_id], &[job.id], "m8-worker")
            .await?;
        let (_, attempt_id, _) = attempts[0];
        backend
            .mark_succeeded(job.id, attempt_id, "m8-worker", 1)
            .await?;
    }

    let queued = count_jobs(&backend, None, Some("queued")).await?;
    let completed = count_jobs(&backend, None, Some("succeeded")).await?;

    assert_eq!(queued, 9_000);
    assert_eq!(completed, 1_000);
    assert_eq!(
        backend.capabilities().backpressure,
        BackpressureCapability::BacklogOnly
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn m8_queue_isolation_priority_and_fair_worker_progress_are_predictable() -> anyhow::Result<()>
{
    let backend = Arc::new(MemoryBackend::new());
    backend.run_migrations().await?;

    for seq in 0..50usize {
        backend
            .enqueue(
                Job::new("queue_a", serde_json::json!({ "seq": seq }))
                    .queue("queue-a")
                    .priority(if seq % 10 == 0 { 10 } else { 0 })
                    .into(),
            )
            .await?;
        backend
            .enqueue(
                Job::new("queue_b", serde_json::json!({ "seq": seq }))
                    .queue("queue-b")
                    .priority(0)
                    .into(),
            )
            .await?;
    }

    let first_a = backend
        .lease_jobs_batch("queue-a", "m8-priority-worker", 30, 5)
        .await?;
    assert_eq!(first_a.len(), 5);
    assert!(
        first_a.iter().all(|job| job.queue == "queue-a"),
        "workers must not cross queue boundaries"
    );
    assert!(
        first_a.iter().all(|job| job.priority == 10),
        "higher-priority queued work must lease before lower-priority work"
    );

    for job in first_a {
        let attempts = backend
            .start_attempts_batch(&[job.dataset_id], &[job.id], "m8-priority-worker")
            .await?;
        backend
            .mark_succeeded(job.id, attempts[0].1, "m8-priority-worker", 1)
            .await?;
    }

    let queue_b_jobs = run_workers_until_empty(backend.clone(), "queue-b", 10, 50).await?;
    assert_eq!(queue_b_jobs.processed_count, 50);
    assert!(
        !queue_b_jobs.worker_counts.is_empty(),
        "at least one worker should make progress"
    );

    let queue_a_remaining = count_jobs(&backend, Some("queue-a"), Some("queued")).await?;
    let queue_b_remaining = count_jobs(&backend, Some("queue-b"), Some("queued")).await?;
    assert_eq!(queue_a_remaining, 45);
    assert_eq!(queue_b_remaining, 0);

    Ok(())
}

#[tokio::test]
#[serial]
async fn m8_postgres_policy_backpressure_rate_limits_without_dropping_jobs() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };

    let backend = azums::PostgresBackend::new(pool.clone());
    assert_eq!(
        backend.capabilities().backpressure,
        BackpressureCapability::ExecutionRateLimit
    );

    sqlx::query(
        r#"
        INSERT INTO queue_policies (queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms)
        VALUES ('m8-policy', 100000, 0, 250)
        ON CONFLICT (queue) DO UPDATE
        SET max_attempts_per_minute = EXCLUDED.max_attempts_per_minute,
            max_in_flight = EXCLUDED.max_in_flight,
            throttle_delay_ms = EXCLUDED.throttle_delay_ms
        "#,
    )
    .execute(&pool)
    .await?;

    let job_id = backend
        .enqueue(
            Job::new("m8_policy_job", serde_json::json!({}))
                .queue("m8-policy")
                .into(),
        )
        .await?;
    let before = Utc::now();

    let leased = backend
        .lease_jobs_batch("m8-policy", "m8-policy-worker", 30, 1)
        .await?;
    assert!(leased.is_empty());

    let job = backend.get_job(job_id).await?.expect("job remains visible");
    assert_eq!(job.status, "queued");
    assert!(
        job.run_at > before,
        "throttled jobs are deferred, not dropped"
    );

    let decisions = PolicyDecisionsRepo::new(pool).list_for_job(job_id).await?;
    let decision = decisions.last().expect("policy decision is observable");
    assert_eq!(decision.decision, "THROTTLED");
    assert_eq!(decision.reason_code, "IN_FLIGHT_EXCEEDED");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "opt-in stress harness; set AZUMS_M8_STRESS=1 and optionally AZUMS_M8_JOB_COUNTS=10000,50000,100000,1000000"]
async fn m8_large_job_count_stress_matrix() -> anyhow::Result<()> {
    if std::env::var("AZUMS_M8_STRESS").as_deref() != Ok("1") {
        eprintln!("set AZUMS_M8_STRESS=1 to run the M8 large stress matrix");
        return Ok(());
    }

    let job_counts = parse_usize_csv("AZUMS_M8_JOB_COUNTS", &[10_000, 50_000, 100_000, 1_000_000]);
    let worker_counts = parse_usize_csv("AZUMS_M8_WORKERS", M8_WORKER_MATRIX);

    for jobs in job_counts {
        for workers in &worker_counts {
            let started = Instant::now();
            eprintln!("M8_STRESS_START jobs={jobs} workers={workers}");
            run_memory_worker_matrix_case(*workers, jobs).await?;
            eprintln!(
                "M8_STRESS_PASS jobs={jobs} workers={workers} elapsed_ms={}",
                started.elapsed().as_millis()
            );
        }
    }

    Ok(())
}

#[derive(Debug)]
struct WorkerRunSummary {
    processed_count: usize,
    worker_counts: HashMap<String, usize>,
}

async fn run_memory_worker_matrix_case(
    worker_count: usize,
    job_count: usize,
) -> anyhow::Result<()> {
    let backend = Arc::new(MemoryBackend::new());
    backend.run_migrations().await?;

    for seq in 0..job_count {
        backend
            .enqueue(Job::new("m8_matrix", serde_json::json!({ "seq": seq })).into())
            .await?;
    }

    let summary =
        run_workers_until_empty(backend.clone(), "default", worker_count, job_count).await?;
    assert_eq!(
        summary.processed_count, job_count,
        "{worker_count} workers must complete all jobs exactly once"
    );

    let metrics = backend
        .as_observability()
        .expect("memory observability")
        .queue_metrics(Some("default"))
        .await?;
    let metrics = metrics.first().expect("default queue metrics");
    let running = metrics.worker_count as usize;
    let queued = metrics.queue_depth as usize;
    let completed = metrics.jobs_completed as usize;

    assert_eq!(running, 0, "no job may be left running");
    assert_eq!(
        queued, 0,
        "no runnable job may silently disappear into backlog"
    );
    assert_eq!(completed, job_count);

    Ok(())
}

async fn run_workers_until_empty(
    backend: Arc<MemoryBackend>,
    queue: &str,
    worker_count: usize,
    expected_total: usize,
) -> anyhow::Result<WorkerRunSummary> {
    let completed_counter = Arc::new(AtomicUsize::new(0));
    let processed_ids = Arc::new(Mutex::new(HashSet::<Uuid>::new()));
    let worker_counts = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let queue = queue.to_string();
    let batch_size = expected_total.div_ceil(100).clamp(25, 25_000) as i64;

    let mut tasks = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let backend = backend.clone();
        let completed_counter = completed_counter.clone();
        let processed_ids = processed_ids.clone();
        let worker_counts = worker_counts.clone();
        let queue = queue.clone();
        let worker_id = format!("m8-worker-{worker_idx}");

        tasks.push(tokio::spawn(async move {
            loop {
                let leased = backend
                    .lease_jobs_batch(&queue, &worker_id, 30, batch_size)
                    .await
                    .unwrap();

                if leased.is_empty() {
                    if completed_counter.load(Ordering::SeqCst) >= expected_total {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    continue;
                }

                let mut updates = Vec::with_capacity(leased.len());
                for job in leased {
                    {
                        let mut seen = processed_ids.lock().unwrap();
                        assert!(
                            seen.insert(job.id),
                            "duplicate active claim for job {}",
                            job.id
                        );
                    }

                    let attempts = backend
                        .start_attempts_batch(&[job.dataset_id], &[job.id], &worker_id)
                        .await
                        .unwrap();
                    let (_, attempt_id, _) = attempts[0];
                    updates.push((job.id, attempt_id, 1));
                }

                for (job_id, attempt_id, latency_ms) in updates {
                    backend
                        .mark_succeeded(job_id, attempt_id, &worker_id, latency_ms)
                        .await
                        .unwrap();
                    completed_counter.fetch_add(1, Ordering::SeqCst);
                    *worker_counts
                        .lock()
                        .unwrap()
                        .entry(worker_id.clone())
                        .or_default() += 1;
                }
            }
        }));
    }

    for task in tasks {
        task.await?;
    }

    let worker_counts = Arc::try_unwrap(worker_counts)
        .expect("worker count map still has references")
        .into_inner()
        .unwrap();

    Ok(WorkerRunSummary {
        processed_count: completed_counter.load(Ordering::SeqCst),
        worker_counts,
    })
}

async fn count_jobs(
    backend: &MemoryBackend,
    queue: Option<&str>,
    status: Option<&str>,
) -> anyhow::Result<usize> {
    let mut count = 0usize;
    let mut cursor_created_at = None;
    let mut cursor_id = None;

    loop {
        let page = backend
            .list_jobs(queue, status, 500, cursor_created_at, cursor_id)
            .await?;
        if page.is_empty() {
            break;
        }

        count += page.len();
        let last = page.last().expect("page is non-empty");
        cursor_created_at = Some(last.created_at);
        cursor_id = Some(last.id);
    }

    Ok(count)
}

fn parse_usize_csv(env_var: &str, default: &[usize]) -> Vec<usize> {
    std::env::var(env_var)
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|part| part.trim().parse::<usize>().ok())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| default.to_vec())
}

#[test]
fn m8_capability_contract_names_backpressure_modes() {
    assert_eq!(
        BackendCapabilities::memory().backpressure,
        BackpressureCapability::BacklogOnly
    );
    assert_eq!(
        BackendCapabilities::sqlite().backpressure,
        BackpressureCapability::BacklogOnly
    );
    assert_eq!(
        BackendCapabilities::postgres().backpressure,
        BackpressureCapability::ExecutionRateLimit
    );
    assert_eq!(
        BackendCapabilities::redis().backpressure,
        BackpressureCapability::BacklogOnly
    );
    assert_eq!(
        BackendCapabilities::postgres().ordering,
        OrderingCapability::FifoAndFastestLeasing
    );
}
