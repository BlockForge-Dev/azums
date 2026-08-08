use azums::{Job, MemoryBackend, StorageBackend};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_high_concurrency_workers_no_duplicates() -> anyhow::Result<()> {
    let total_jobs = 100;
    let worker_count = 5;

    let backend = Arc::new(MemoryBackend::new());
    backend.run_migrations().await?;

    // Enqueue 100 jobs
    for i in 0..total_jobs {
        backend
            .enqueue(Job::new("concurrent_task", serde_json::json!({"seq": i})).into())
            .await?;
    }

    let completed_counter = Arc::new(AtomicU32::new(0));
    let processed_ids = Arc::new(Mutex::new(HashSet::new()));

    // Spawn 5 concurrent worker loops
    let mut tasks = Vec::new();
    for worker_idx in 0..worker_count {
        let backend_clone = backend.clone();
        let counter_clone = completed_counter.clone();
        let ids_clone = processed_ids.clone();
        let worker_name = format!("concurrent-worker-{worker_idx}");

        tasks.push(tokio::spawn(async move {
            loop {
                let leased = backend_clone
                    .dequeue_and_lease("default", &worker_name, 10, 5)
                    .await
                    .unwrap();

                if leased.is_empty() {
                    // Check if all jobs done
                    if counter_clone.load(Ordering::SeqCst) >= total_jobs as u32 {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    continue;
                }

                for job in leased {
                    let attempts = backend_clone
                        .start_attempts_batch(&["default".to_string()], &[job.id], &worker_name)
                        .await
                        .unwrap();

                    if let Some((jid, att_id, _)) = attempts.first() {
                        // Atomic duplicate check
                        {
                            let mut set = ids_clone.lock().unwrap();
                            assert!(
                                !set.contains(jid),
                                "DUPLICATE JOB EXECUTION DETECTED for job {jid}!"
                            );
                            set.insert(*jid);
                        }

                        backend_clone
                            .complete_job(*jid, *att_id, &worker_name, 2)
                            .await
                            .unwrap();

                        counter_clone.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }));
    }

    for task in tasks {
        task.await?;
    }

    assert_eq!(
        completed_counter.load(Ordering::SeqCst),
        total_jobs as u32,
        "All 100 jobs must be executed exactly once!"
    );
    assert_eq!(
        processed_ids.lock().unwrap().len(),
        total_jobs,
        "HashSet must contain 100 unique job IDs!"
    );

    Ok(())
}
