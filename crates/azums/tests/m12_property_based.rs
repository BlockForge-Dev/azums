use azums::{
    make_sqlite_pool, Job, JobLifecycleState, JobStatus, MemoryBackend, SqliteBackend,
    StorageBackend,
};
use chrono::{Duration as ChronoDuration, Utc};
use proptest::{prelude::*, test_runner::Config};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const CASES: u32 = 256;

#[derive(Debug, Clone)]
enum ProgramOp {
    Enqueue {
        priority: i32,
        max_attempts: i32,
        schedule_ms: i64,
        duplicate_key_slot: Option<u8>,
    },
    Lease {
        worker_slot: u8,
        batch_size: i64,
    },
    Complete {
        worker_slot: u8,
    },
    Retry {
        worker_slot: u8,
        delay_ms: i64,
    },
    Dlq {
        worker_slot: u8,
    },
    Cancel {
        worker_slot: Option<u8>,
    },
    Reap,
}

fn program_op_strategy() -> impl Strategy<Value = ProgramOp> {
    prop_oneof![
        (
            -20_i32..=20,
            1_i32..=5,
            -25_i64..=25,
            prop::option::of(0_u8..=7)
        )
            .prop_map(
                |(priority, max_attempts, schedule_ms, duplicate_key_slot)| ProgramOp::Enqueue {
                    priority,
                    max_attempts,
                    schedule_ms,
                    duplicate_key_slot,
                },
            ),
        (0_u8..=15, 1_i64..=5).prop_map(|(worker_slot, batch_size)| ProgramOp::Lease {
            worker_slot,
            batch_size
        }),
        (0_u8..=15).prop_map(|worker_slot| ProgramOp::Complete { worker_slot }),
        (0_u8..=15, 0_i64..=25).prop_map(|(worker_slot, delay_ms)| ProgramOp::Retry {
            worker_slot,
            delay_ms
        }),
        (0_u8..=15).prop_map(|worker_slot| ProgramOp::Dlq { worker_slot }),
        prop::option::of(0_u8..=15).prop_map(|worker_slot| ProgramOp::Cancel { worker_slot }),
        Just(ProgramOp::Reap),
    ]
}

proptest! {
    #![proptest_config(Config {
        cases: CASES,
        max_shrink_iters: 2_048,
        ..Config::default()
    })]

    #[test]
    fn m12_generated_lifecycle_programs_preserve_core_invariants(
        ops in prop::collection::vec(program_op_strategy(), 1..160)
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            run_generated_program(ops).await.unwrap();
        });
    }

    #[test]
    fn m12_generated_lifecycle_state_transitions_are_exact(
        transitions in prop::collection::vec((state_strategy(), state_strategy()), 1..256)
    ) {
        for (from, to) in transitions {
            let expected = from.legal_successors().contains(&to);
            prop_assert_eq!(from.can_transition_to(to), expected);

            let checked = from.ensure_transition_to(to);
            if expected {
                prop_assert!(checked.is_ok(), "{from:?} -> {to:?} should be legal");
            } else {
                prop_assert!(checked.is_err(), "{from:?} -> {to:?} should be illegal");
            }

            if from.is_terminal() {
                prop_assert!(
                    !from.can_transition_to(to),
                    "terminal state {from:?} accepted transition to {to:?}"
                );
            }
        }
    }
}

#[test]
fn m12_sqlite_generated_rollbacks_leave_no_durable_job() {
    let mut runner = proptest::test_runner::TestRunner::new(Config {
        cases: 128,
        max_shrink_iters: 1_024,
        ..Config::default()
    });

    runner
        .run(
            &prop::collection::vec((0_u8..=3, any::<u16>()), 1..64),
            |ops| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    sqlite_rollback_property(ops).await.map_err(|err| {
                        TestCaseError::fail(format!("SQLite rollback property failed: {err:#}"))
                    })
                })
            },
        )
        .unwrap();
}

fn state_strategy() -> impl Strategy<Value = JobLifecycleState> {
    prop_oneof![
        Just(JobLifecycleState::Scheduled),
        Just(JobLifecycleState::Queued),
        Just(JobLifecycleState::Running),
        Just(JobLifecycleState::Completed),
        Just(JobLifecycleState::RetryWait),
        Just(JobLifecycleState::Cancelled),
        Just(JobLifecycleState::Dlq),
    ]
}

async fn run_generated_program(ops: Vec<ProgramOp>) -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;

    let mut logical_jobs = HashSet::<Uuid>::new();
    let mut idempotency_index = HashMap::<u8, Uuid>::new();
    let mut leased_by_worker = HashMap::<String, Vec<Uuid>>::new();
    let mut max_attempt_by_job = HashMap::<Uuid, i32>::new();

    for (seq, op) in ops.into_iter().enumerate() {
        match op {
            ProgramOp::Enqueue {
                priority,
                max_attempts,
                schedule_ms,
                duplicate_key_slot,
            } => {
                let mut job = Job::new(
                    "m12-property",
                    json!({ "seq": seq, "duplicate_key_slot": duplicate_key_slot }),
                )
                .priority(priority)
                .max_attempts(max_attempts)
                .run_at(Utc::now() + ChronoDuration::milliseconds(schedule_ms));

                if let Some(slot) = duplicate_key_slot {
                    job = job.idempotency_key(format!("m12-key-{slot}"));
                }

                let job_id = backend.enqueue(job.into()).await?;
                if let Some(slot) = duplicate_key_slot {
                    if let Some(existing) = idempotency_index.insert(slot, job_id) {
                        assert_eq!(
                            existing, job_id,
                            "same idempotency key created a second job"
                        );
                    }
                }
                logical_jobs.insert(job_id);
            }
            ProgramOp::Lease {
                worker_slot,
                batch_size,
            } => {
                let worker = worker_id(worker_slot);
                let leased = backend
                    .lease_jobs_batch("default", &worker, 0, batch_size)
                    .await?;
                assert_unique_leases(&leased);
                leased_by_worker
                    .entry(worker)
                    .or_default()
                    .extend(leased.into_iter().map(|job| job.id));
            }
            ProgramOp::Complete { worker_slot } => {
                let worker = worker_id(worker_slot);
                if let Some(job_id) = pop_leased(&mut leased_by_worker, &worker) {
                    if let Ok(attempt) = start_one_attempt(&backend, job_id, &worker).await {
                        backend
                            .mark_succeeded(job_id, attempt.1, &worker, 1)
                            .await?;
                    }
                }
            }
            ProgramOp::Retry {
                worker_slot,
                delay_ms,
            } => {
                let worker = worker_id(worker_slot);
                if let Some(job_id) = pop_leased(&mut leased_by_worker, &worker) {
                    if let Ok((_, attempt_id, attempt_no)) =
                        start_one_attempt(&backend, job_id, &worker).await
                    {
                        backend
                            .reschedule_for_retry(
                                job_id,
                                attempt_id,
                                &worker,
                                1,
                                Utc::now() + ChronoDuration::milliseconds(delay_ms),
                                "PROPERTY_RETRY",
                                "generated retry",
                                attempt_no,
                            )
                            .await?;
                    }
                }
            }
            ProgramOp::Dlq { worker_slot } => {
                let worker = worker_id(worker_slot);
                if let Some(job_id) = pop_leased(&mut leased_by_worker, &worker) {
                    if let Ok((_, attempt_id, attempt_no)) =
                        start_one_attempt(&backend, job_id, &worker).await
                    {
                        backend
                            .mark_dlq(
                                job_id,
                                attempt_id,
                                &worker,
                                1,
                                "PROPERTY_DLQ",
                                "PROPERTY_ERROR",
                                "generated dlq",
                                attempt_no,
                            )
                            .await?;
                    }
                }
            }
            ProgramOp::Cancel { worker_slot } => {
                if let Some(job_id) = first_live_job(&backend).await? {
                    let worker = worker_slot.map(worker_id);
                    let _ = backend.cancel_job(job_id, worker.as_deref()).await;
                }
            }
            ProgramOp::Reap => {
                backend.reap_expired_locks().await?;
            }
        }

        assert_backend_invariants(&backend, &logical_jobs, &mut max_attempt_by_job).await?;
    }

    Ok(())
}

async fn sqlite_rollback_property(ops: Vec<(u8, u16)>) -> anyhow::Result<()> {
    let pool = make_sqlite_pool(&format!(
        "sqlite://file:m12_sqlite_{}?mode=memory&cache=shared",
        Uuid::new_v4()
    ))
    .await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;

    sqlx::query("CREATE TABLE app_state (id TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(backend.pool())
        .await?;

    let mut committed = 0_i64;
    let mut rolled_back = 0_i64;

    for (kind, value) in ops {
        let app_id = format!("app-{kind}-{value}-{committed}-{rolled_back}");
        let job_type = format!("m12-tx-{kind}-{value}");
        let mut tx = backend.pool().begin().await?;
        sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
            .bind(&app_id)
            .bind(value.to_string())
            .execute(&mut *tx)
            .await?;
        backend
            .enqueue_in_tx(
                &mut tx,
                Job::new(&job_type, json!({ "value": value })).into(),
            )
            .await?;

        if kind == 0 {
            tx.commit().await?;
            committed += 1;
        } else {
            tx.rollback().await?;
            rolled_back += 1;
            assert_eq!(
                sqlite_count(&backend, "app_state", "id", &app_id).await?,
                0,
                "rollback leaked durable app state"
            );
            assert_eq!(
                sqlite_count(&backend, "jobs", "job_type", &job_type).await?,
                0,
                "rollback leaked durable job state"
            );
        }
    }

    let durable_apps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_state")
        .fetch_one(backend.pool())
        .await?;
    let durable_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(backend.pool())
        .await?;
    assert_eq!(durable_apps, committed);
    assert_eq!(durable_jobs, committed);

    Ok(())
}

async fn sqlite_count(
    backend: &SqliteBackend,
    table: &str,
    column: &str,
    value: &str,
) -> anyhow::Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?");
    Ok(sqlx::query_scalar(&sql)
        .bind(value)
        .fetch_one(backend.pool())
        .await?)
}

async fn start_one_attempt(
    backend: &MemoryBackend,
    job_id: Uuid,
    worker: &str,
) -> anyhow::Result<(Uuid, Uuid, i32)> {
    let attempts = backend
        .start_attempts_batch(&["default".to_string()], &[job_id], worker)
        .await?;
    attempts
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no attempt created for {job_id}"))
}

async fn first_live_job(backend: &MemoryBackend) -> anyhow::Result<Option<Uuid>> {
    Ok(backend
        .list_jobs(None, None, 512, None, None)
        .await?
        .into_iter()
        .find(|job| !JobStatus::parse(&job.status).unwrap().is_terminal())
        .map(|job| job.id))
}

async fn assert_backend_invariants(
    backend: &MemoryBackend,
    logical_jobs: &HashSet<Uuid>,
    max_attempt_by_job: &mut HashMap<Uuid, i32>,
) -> anyhow::Result<()> {
    let mut seen_jobs = HashSet::new();
    let mut active_running = HashSet::new();

    for job_id in logical_jobs {
        let job = backend
            .get_job(*job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("committed logical job {job_id} disappeared"))?;
        assert!(seen_jobs.insert(job.id), "job listed twice: {}", job.id);

        let status = JobStatus::parse(&job.status)?;
        if status == JobStatus::Running {
            assert!(
                job.locked_by.is_some(),
                "running job {} has no worker lease",
                job.id
            );
            assert!(
                active_running.insert(job.id),
                "duplicate valid running lease for {}",
                job.id
            );
        }

        if status.is_terminal() {
            assert!(
                job.locked_by.is_none() && job.lock_expires_at.is_none(),
                "terminal job {} retained active lease metadata",
                job.id
            );
            assert!(
                backend.cancel_job(job.id, None).await.is_err(),
                "terminal job {} accepted cancellation",
                job.id
            );
        }
    }

    for job_id in logical_jobs {
        assert!(
            seen_jobs.contains(job_id),
            "committed logical job {job_id} disappeared"
        );
    }

    let mut attempt_numbers_by_job = HashMap::<Uuid, Vec<i32>>::new();
    for attempt in backend.attempts_snapshot() {
        attempt_numbers_by_job
            .entry(attempt.job_id)
            .or_default()
            .push(attempt.attempt_no);
    }

    for (job_id, mut attempt_numbers) in attempt_numbers_by_job {
        attempt_numbers.sort_unstable();
        let unique_attempts = attempt_numbers.iter().copied().collect::<HashSet<_>>();
        assert_eq!(
            unique_attempts.len(),
            attempt_numbers.len(),
            "duplicate attempt number for job {job_id}: {attempt_numbers:?}"
        );

        let previous_max = max_attempt_by_job.entry(job_id).or_default();
        let current_max = attempt_numbers.last().copied().unwrap_or_default();
        assert!(
            current_max >= *previous_max,
            "attempt number decreased for job {job_id}: {} then {current_max}",
            previous_max
        );
        *previous_max = current_max;
    }

    Ok(())
}

fn assert_unique_leases(jobs: &[Job]) {
    let mut seen = HashSet::new();
    for job in jobs {
        assert!(seen.insert(job.id), "duplicate job in same lease batch");
        assert_eq!(job.status, "running");
        assert!(job.locked_by.is_some());
    }
}

fn pop_leased(leased_by_worker: &mut HashMap<String, Vec<Uuid>>, worker: &str) -> Option<Uuid> {
    leased_by_worker.get_mut(worker).and_then(Vec::pop)
}

fn worker_id(slot: u8) -> String {
    format!("m12-worker-{}", slot % 16)
}
