use azums::{make_sqlite_pool, Job, SqliteBackend, StorageBackend};
use rand::{rngs::StdRng, Rng, SeedableRng};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

pub async fn run_contention_scenario(
    seed: u64,
    job_count: usize,
    worker_count: usize,
) -> anyhow::Result<()> {
    let db_url = format!(
        "sqlite://file:m11_sqlite_contention_{}?mode=memory&cache=shared",
        Uuid::new_v4()
    );
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = Arc::new(SqliteBackend::new(pool));
    backend.run_migrations().await?;

    for seq in 0..job_count {
        backend
            .enqueue(Job::new("m11-sqlite-contention", json!({ "seq": seq })).into())
            .await?;
    }

    let completed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(worker_count);

    for worker_idx in 0..worker_count {
        let backend = backend.clone();
        let completed = completed.clone();
        handles.push(tokio::spawn(async move {
            let mut rng = StdRng::seed_from_u64(seed ^ worker_idx as u64);
            let worker_id = format!("sqlite-chaos-worker-{worker_idx}");

            for _ in 0..512 {
                if completed.load(Ordering::SeqCst) >= job_count {
                    break;
                }

                if backend.reap_expired_locks().await.is_err() {
                    sleep(Duration::from_millis(rng.random_range(1..=5))).await;
                    continue;
                }
                let lease_seconds = 1;
                let Ok(leased) = backend
                    .lease_jobs_batch(
                        "default",
                        &worker_id,
                        lease_seconds,
                        rng.random_range(1..=4),
                    )
                    .await
                else {
                    sleep(Duration::from_millis(rng.random_range(1..=5))).await;
                    continue;
                };

                if leased.is_empty() {
                    sleep(Duration::from_millis(rng.random_range(0..=3))).await;
                    continue;
                }

                for job in leased {
                    if rng.random_bool(0.20) {
                        let _ = backend
                            .start_attempts_batch(
                                std::slice::from_ref(&job.dataset_id),
                                &[job.id],
                                &worker_id,
                            )
                            .await;
                        continue;
                    }

                    let Ok(attempts) = backend
                        .start_attempts_batch(
                            std::slice::from_ref(&job.dataset_id),
                            &[job.id],
                            &worker_id,
                        )
                        .await
                    else {
                        sleep(Duration::from_millis(rng.random_range(1..=5))).await;
                        continue;
                    };
                    if backend
                        .mark_succeeded(job.id, attempts[0].1, &worker_id, 1)
                        .await
                        .is_ok()
                    {
                        completed.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }

            Ok::<(), anyhow::Error>(())
        }));
    }

    for handle in handles {
        handle.await??;
    }

    sleep(Duration::from_millis(1_100)).await;

    for _ in 0..256 {
        if completed.load(Ordering::SeqCst) >= job_count {
            break;
        }

        backend.reap_expired_locks().await?;
        let leased = backend
            .lease_jobs_batch("default", "sqlite-chaos-recovery", 0, 32)
            .await?;
        for job in leased {
            let attempts = backend
                .start_attempts_batch(
                    std::slice::from_ref(&job.dataset_id),
                    &[job.id],
                    "sqlite-chaos-recovery",
                )
                .await?;
            backend
                .mark_succeeded(job.id, attempts[0].1, "sqlite-chaos-recovery", 1)
                .await?;
            completed.fetch_add(1, Ordering::SeqCst);
        }
    }

    assert_eq!(
        completed.load(Ordering::SeqCst),
        job_count,
        "SQLite contention chaos must recover and complete all committed jobs"
    );

    let running = backend
        .list_jobs(Some("default"), Some("running"), 500, None, None)
        .await?;
    let queued = backend
        .list_jobs(Some("default"), Some("queued"), 500, None, None)
        .await?;
    assert!(running.is_empty());
    assert!(queued.is_empty());

    Ok(())
}
