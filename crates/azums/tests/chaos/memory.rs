use azums::{Job, JobStatus, MemoryBackend, StorageBackend};
use chrono::Utc;
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde_json::json;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
enum Fault {
    WorkerCrashBeforeAttempt,
    WorkerCrashDuringAttempt,
    SigkillBeforeAck,
    HandlerPanic,
    HandlerTimeout,
    ConnectionTimeout,
    ConnectionReset,
    PermanentFailure,
    RetryableFailure,
    SuccessfulAck,
}

pub async fn run_randomized_scenarios(scenarios: usize, seed: u64) -> anyhow::Result<()> {
    for scenario_idx in 0..scenarios {
        let scenario_seed = seed ^ ((scenario_idx as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        run_one_scenario(scenario_idx, scenario_seed).await?;
    }
    Ok(())
}

async fn run_one_scenario(scenario_idx: usize, seed: u64) -> anyhow::Result<()> {
    let mut rng = StdRng::seed_from_u64(seed);
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let job_count = rng.random_range(4..=16);
    let worker_count = rng.random_range(1..=8);
    let mut job_ids = Vec::with_capacity(job_count);

    for seq in 0..job_count {
        let mut job = Job::new("m11-chaos", json!({ "scenario": scenario_idx, "seq": seq }))
            .max_attempts(rng.random_range(1..=4))
            .priority(rng.random_range(-2..=5));

        if rng.random_bool(0.2) {
            job = job.run_at(Utc::now() - chrono::Duration::milliseconds(rng.random_range(0..=50)));
        }
        if rng.random_bool(0.1) {
            job = job.deadline_at(Utc::now() - chrono::Duration::milliseconds(1));
        }

        job_ids.push(backend.enqueue(job.into()).await?);
    }

    let mut terminal_jobs = HashSet::new();
    let max_steps = job_count * 12;

    for _ in 0..max_steps {
        backend.reap_expired_locks().await?;

        if terminal_jobs.len() == job_count {
            break;
        }

        let worker_id = format!("chaos-worker-{}", rng.random_range(0..worker_count));
        let lease_seconds = 0;
        let batch_size = rng.random_range(1..=3);
        let leased = backend
            .lease_jobs_batch("default", &worker_id, lease_seconds, batch_size)
            .await?;

        for job in leased {
            let fault = random_fault(&mut rng);
            apply_fault(
                &backend,
                &worker_id,
                job.id,
                job.max_attempts,
                fault,
                &mut rng,
            )
            .await?;
        }

        update_terminal_set(&backend, &job_ids, &mut terminal_jobs).await?;
    }

    recover_and_drain(&backend, &job_ids).await?;
    assert_invariants(&backend, &job_ids).await?;
    Ok(())
}

async fn apply_fault(
    backend: &MemoryBackend,
    worker_id: &str,
    job_id: Uuid,
    max_attempts: i32,
    fault: Fault,
    rng: &mut StdRng,
) -> anyhow::Result<()> {
    match fault {
        Fault::WorkerCrashBeforeAttempt => Ok(()),
        Fault::WorkerCrashDuringAttempt | Fault::SigkillBeforeAck => {
            let _ = backend
                .start_attempts_batch(&["default".to_string()], &[job_id], worker_id)
                .await?;
            Ok(())
        }
        Fault::HandlerPanic => {
            let attempts = backend
                .start_attempts_batch(&["default".to_string()], &[job_id], worker_id)
                .await?;
            backend
                .mark_dlq(
                    job_id,
                    attempts[0].1,
                    worker_id,
                    rng.random_range(0..=50),
                    "PANIC",
                    "PANIC",
                    "chaos handler panic",
                    attempts[0].2,
                )
                .await
        }
        Fault::PermanentFailure => {
            let attempts = backend
                .start_attempts_batch(&["default".to_string()], &[job_id], worker_id)
                .await?;
            backend
                .mark_dlq(
                    job_id,
                    attempts[0].1,
                    worker_id,
                    rng.random_range(0..=50),
                    "PERMANENT_ERROR",
                    "PERMANENT_ERROR",
                    "chaos permanent failure",
                    attempts[0].2,
                )
                .await
        }
        Fault::HandlerTimeout
        | Fault::ConnectionTimeout
        | Fault::ConnectionReset
        | Fault::RetryableFailure => {
            let attempts = backend
                .start_attempts_batch(&["default".to_string()], &[job_id], worker_id)
                .await?;
            let attempt_no = attempts[0].2;
            if attempt_no >= max_attempts {
                backend
                    .mark_dlq(
                        job_id,
                        attempts[0].1,
                        worker_id,
                        rng.random_range(0..=50),
                        "MAX_ATTEMPTS_EXCEEDED",
                        fault.error_code(),
                        "chaos retry budget exhausted",
                        attempt_no,
                    )
                    .await
            } else {
                backend
                    .reschedule_for_retry(
                        job_id,
                        attempts[0].1,
                        worker_id,
                        rng.random_range(0..=50),
                        Utc::now(),
                        fault.error_code(),
                        "chaos retryable failure",
                        attempt_no,
                    )
                    .await
            }
        }
        Fault::SuccessfulAck => {
            let attempts = backend
                .start_attempts_batch(&["default".to_string()], &[job_id], worker_id)
                .await?;
            backend
                .mark_succeeded(job_id, attempts[0].1, worker_id, rng.random_range(0..=50))
                .await
        }
    }
}

async fn recover_and_drain(backend: &MemoryBackend, job_ids: &[Uuid]) -> anyhow::Result<()> {
    for _ in 0..128 {
        backend.reap_expired_locks().await?;

        let leased = backend
            .lease_jobs_batch("default", "chaos-recovery-worker", 0, 32)
            .await?;

        if leased.is_empty() {
            let terminal = count_terminal(backend, job_ids).await?;
            if terminal == job_ids.len() {
                return Ok(());
            }
            continue;
        }

        for job in leased {
            let attempts = backend
                .start_attempts_batch(
                    std::slice::from_ref(&job.dataset_id),
                    &[job.id],
                    "chaos-recovery-worker",
                )
                .await?;
            backend
                .mark_succeeded(job.id, attempts[0].1, "chaos-recovery-worker", 1)
                .await?;
        }
    }

    anyhow::bail!("chaos recovery drain did not converge")
}

async fn assert_invariants(backend: &MemoryBackend, job_ids: &[Uuid]) -> anyhow::Result<()> {
    let mut seen = HashSet::new();
    for job_id in job_ids {
        assert!(
            seen.insert(*job_id),
            "test generated duplicate job id {job_id}"
        );
        let job = backend
            .get_job(*job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("committed job {job_id} disappeared"))?;

        let status = JobStatus::parse(&job.status)?;
        assert!(
            status.is_terminal(),
            "job {} did not reach terminal state after recovery: {}",
            job.id,
            job.status
        );
        assert!(job.locked_by.is_none(), "terminal job retained locked_by");
        assert!(
            job.lock_expires_at.is_none(),
            "terminal job retained lock_expires_at"
        );

        let terminal_cancel = backend.cancel_job(*job_id, None).await;
        assert!(
            terminal_cancel.is_err(),
            "terminal job {} accepted cancellation",
            job.id
        );
    }

    Ok(())
}

async fn update_terminal_set(
    backend: &MemoryBackend,
    job_ids: &[Uuid],
    terminal_jobs: &mut HashSet<Uuid>,
) -> anyhow::Result<()> {
    for job_id in job_ids {
        if terminal_jobs.contains(job_id) {
            continue;
        }
        let Some(job) = backend.get_job(*job_id).await? else {
            anyhow::bail!("committed job {job_id} disappeared during chaos");
        };
        if JobStatus::parse(&job.status)?.is_terminal() {
            terminal_jobs.insert(*job_id);
        }
    }
    Ok(())
}

async fn count_terminal(backend: &MemoryBackend, job_ids: &[Uuid]) -> anyhow::Result<usize> {
    let mut count = 0;
    for job_id in job_ids {
        if let Some(job) = backend.get_job(*job_id).await? {
            if JobStatus::parse(&job.status)?.is_terminal() {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn random_fault(rng: &mut StdRng) -> Fault {
    match rng.random_range(0..10) {
        0 => Fault::WorkerCrashBeforeAttempt,
        1 => Fault::WorkerCrashDuringAttempt,
        2 => Fault::SigkillBeforeAck,
        3 => Fault::HandlerPanic,
        4 => Fault::HandlerTimeout,
        5 => Fault::ConnectionTimeout,
        6 => Fault::ConnectionReset,
        7 => Fault::PermanentFailure,
        8 => Fault::RetryableFailure,
        _ => Fault::SuccessfulAck,
    }
}

impl Fault {
    fn error_code(self) -> &'static str {
        match self {
            Fault::HandlerTimeout | Fault::ConnectionTimeout => "TIMEOUT",
            Fault::ConnectionReset => "DB_DISCONNECT",
            Fault::RetryableFailure => "SYSTEM_FAILURE",
            _ => "HANDLER_ERROR",
        }
    }
}
