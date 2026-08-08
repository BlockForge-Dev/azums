use azums::{quickstart, Job, QueueConfig, QueueOrdering};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_single_worker_strict_fifo_ordering() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?.with_queue("fifo_test");
    flow.configure_queue("fifo_test", QueueConfig::new(QueueOrdering::Fifo)).await;

    let received = Arc::new(Mutex::new(Vec::new()));
    let rec_clone = received.clone();

    flow.register_handler("seq_job", move |job| {
        let rec = rec_clone.clone();
        async move {
            let seq = job.payload["seq"].as_u64().unwrap() as usize;
            rec.lock().unwrap().push(seq);
            Ok(())
        }
    })
    .await;

    // Enqueue 100 jobs sequentially
    for i in 0..100 {
        flow.enqueue(
            Job::new("seq_job", serde_json::json!({ "seq": i }))
                .queue("fifo_test"),
        )
        .await?;
    }

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 100);

    let seqs = received.lock().unwrap().clone();
    let expected: Vec<usize> = (0..100).collect();

    assert_eq!(
        seqs, expected,
        "Single-worker FIFO queue must process jobs in exact enqueued order"
    );

    Ok(())
}

#[tokio::test]
async fn test_fastest_ordering_mode() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?.with_queue("fastest_test");
    flow.configure_queue("fastest_test", QueueConfig::new(QueueOrdering::Fastest)).await;

    let count = Arc::new(Mutex::new(0));
    let count_clone = count.clone();

    flow.register_handler("fast_job", move |_job| {
        let c = count_clone.clone();
        async move {
            *c.lock().unwrap() += 1;
            Ok(())
        }
    })
    .await;

    for i in 0..50 {
        flow.enqueue(
            Job::new("fast_job", serde_json::json!({ "idx": i }))
                .queue("fastest_test"),
        )
        .await?;
    }

    let processed = flow.run_until_empty().await?;
    assert_eq!(processed, 50);
    assert_eq!(*count.lock().unwrap(), 50);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "sqlite")]
async fn test_sqlite_fifo_ordering() -> anyhow::Result<()> {
    let flow = quickstart("sqlite::memory:").await?.with_queue("sqlite_fifo");
    flow.configure_queue("sqlite_fifo", QueueConfig::new(QueueOrdering::Fifo)).await;

    let received = Arc::new(Mutex::new(Vec::new()));
    let rec_clone = received.clone();

    flow.register_handler("sq_job", move |job| {
        let rec = rec_clone.clone();
        async move {
            let seq = job.payload["seq"].as_u64().unwrap() as usize;
            rec.lock().unwrap().push(seq);
            Ok(())
        }
    })
    .await;

    for i in 0..100 {
        flow.enqueue(
            Job::new("sq_job", serde_json::json!({ "seq": i }))
                .queue("sqlite_fifo"),
        )
        .await?;
    }

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 100);

    let seqs = received.lock().unwrap().clone();
    let expected: Vec<usize> = (0..100).collect();
    assert_eq!(seqs, expected, "SQLite FIFO queue must process jobs in exact enqueued order");

    Ok(())
}

#[tokio::test]
async fn test_multi_worker_fifo_batch_leasing() -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let flow = quickstart("memory").await?.with_queue("multi_fifo");
    flow.configure_queue("multi_fifo", QueueConfig::new(QueueOrdering::Fifo)).await;

    let total = Arc::new(AtomicUsize::new(0));

    for i in 0..100 {
        flow.enqueue(
            Job::new("multi_job", serde_json::json!({ "seq": i }))
                .queue("multi_fifo"),
        )
        .await?;
    }

    let backend = flow.backend().clone();
    let counter = total.clone();

    // Spawn 4 concurrent worker tasks pulling batches under Fifo ordering
    let mut handles = Vec::new();
    for worker_idx in 0..4 {
        let b = backend.clone();
        let c = counter.clone();
        let handle = tokio::spawn(async move {
            let worker_id = format!("worker-{worker_idx}");
            let mut leased_count = 0;
            loop {
                let batch = b
                    .lease_jobs_batch_with_ordering(
                        "multi_fifo",
                        &worker_id,
                        10,
                        10,
                        azums_core::QueueOrdering::Fifo,
                    )
                    .await
                    .unwrap();

                if batch.is_empty() {
                    break;
                }
                leased_count += batch.len();
                c.fetch_add(batch.len(), Ordering::SeqCst);

                // Mark succeeded
                for job in batch {
                    let attempts = b
                        .start_attempts_batch(&[job.dataset_id], &[job.id], &worker_id)
                        .await
                        .unwrap();
                    if let Some((jid, aid, _)) = attempts.first() {
                        b.mark_succeeded(*jid, *aid, &worker_id, 1).await.unwrap();
                    }
                }
            }
            leased_count
        });
        handles.push(handle);
    }

    for h in handles {
        h.await?;
    }

    assert_eq!(total.load(Ordering::SeqCst), 100);
    Ok(())
}
