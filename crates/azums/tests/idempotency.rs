use azums::{make_sqlite_pool, quickstart, Job, SqliteBackend, StorageBackend};
use serde_json::json;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn duplicate_enqueue_attempts_with_same_key_create_one_logical_job() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?.with_queue("m7-dedupe");
    let key = "m7-key-100-duplicates".to_string();

    let mut tasks = Vec::new();
    for _worker_idx in 0..100 {
        let flow = flow.clone();
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            flow.enqueue(
                Job::new("dedupe_job", json!({"attempt": "duplicate"}))
                    .queue("m7-dedupe")
                    .idempotency_key(key),
            )
            .await
        }));
    }

    let mut ids = HashSet::new();
    for task in tasks {
        ids.insert(task.await??);
    }

    assert_eq!(ids.len(), 1, "duplicates should return one logical job id");

    let jobs = flow
        .backend()
        .list_jobs(Some("m7-dedupe"), None, 500, None, None)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].idempotency_key.as_deref(), Some(key.as_str()));

    Ok(())
}

#[tokio::test]
async fn duplicate_delivery_after_side_effect_is_safe_with_application_idempotency(
) -> anyhow::Result<()> {
    let flow = quickstart("memory")
        .await?
        .with_worker_id("m7-worker-a")
        .with_queue("m7-crash")
        .with_lease_seconds(1);

    let side_effect_store = Arc::new(tokio::sync::Mutex::new(HashSet::<String>::new()));
    let actual_side_effects = Arc::new(AtomicUsize::new(0));
    let handler_calls = Arc::new(AtomicUsize::new(0));

    let store = side_effect_store.clone();
    let side_effects = actual_side_effects.clone();
    let calls = handler_calls.clone();
    flow.register_handler("charge_card", move |job| {
        let store = store.clone();
        let side_effects = side_effects.clone();
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            let key = job
                .payload
                .get("operation_key")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();

            let mut processed = store.lock().await;
            if processed.insert(key) {
                side_effects.fetch_add(1, Ordering::SeqCst);
            }

            if calls.load(Ordering::SeqCst) == 1 {
                anyhow::bail!("SYSTEM_FAILURE: crashed after side effect before ACK");
            }

            Ok(())
        }
    })
    .await;

    let job_id = flow
        .enqueue(
            Job::new("charge_card", json!({"operation_key": "payment-123"}))
                .queue("m7-crash")
                .idempotency_key("enqueue-payment-123"),
        )
        .await?;

    assert_eq!(flow.run_until_empty().await?, 1);
    let job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "queued");
    assert!(
        job.run_at > chrono::Utc::now(),
        "first delivery failure should be waiting for retry"
    );

    tokio::time::sleep(std::time::Duration::from_millis(2300)).await;
    assert_eq!(flow.run_until_empty().await?, 1);

    let final_job = flow.backend().get_job(job_id).await?.unwrap();
    assert_eq!(final_job.status, "succeeded");
    assert_eq!(
        handler_calls.load(Ordering::SeqCst),
        2,
        "at-least-once delivery may execute the handler twice"
    );
    assert_eq!(
        actual_side_effects.load(Ordering::SeqCst),
        1,
        "application idempotency key should guard the side effect"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn sqlite_duplicate_enqueue_attempts_with_same_key_create_one_logical_job(
) -> anyhow::Result<()> {
    let pool = make_sqlite_pool(&format!(
        "sqlite://file:m7_sqlite_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    ))
    .await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;
    let backend = Arc::new(backend);
    let key = "sqlite-m7-key".to_string();

    let mut tasks = Vec::new();
    for _ in 0..100 {
        let backend = backend.clone();
        let key = key.clone();
        tasks.push(tokio::spawn(async move {
            backend
                .enqueue(
                    Job::new("sqlite_dedupe", json!({"source": "duplicate"}))
                        .idempotency_key(key)
                        .into(),
                )
                .await
        }));
    }

    let mut ids = HashSet::new();
    for task in tasks {
        ids.insert(task.await??);
    }

    assert_eq!(ids.len(), 1);
    let rows = backend.list_jobs(None, None, 500, None, None).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].idempotency_key.as_deref(), Some("sqlite-m7-key"));

    Ok(())
}
