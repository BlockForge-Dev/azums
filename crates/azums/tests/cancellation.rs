mod common;

use azums::{make_sqlite_pool, quickstart, Job, PostgresBackend, SqliteBackend, StorageBackend};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn memory_cancel_queued_job_is_terminal() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let job_id = client
        .enqueue(Job::new("cancel_me", json!({"backend": "memory"})))
        .await?;

    client.cancel_job(job_id, None).await?;

    let job = client.backend().get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "canceled");

    let second_cancel = client.cancel_job(job_id, None).await;
    assert!(second_cancel.is_err(), "canceled is terminal");

    Ok(())
}

#[tokio::test]
async fn sqlite_cancel_running_job_requires_lease_owner() -> anyhow::Result<()> {
    let db_url = format!(
        "sqlite://file:test_sqlite_cancel_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = Arc::new(SqliteBackend::new(pool));
    backend.run_migrations().await?;

    let job_id = backend
        .enqueue(Job::new("cancel_running", json!({"backend": "sqlite"})).into())
        .await?;
    backend
        .lease_jobs_batch("default", "sqlite-owner", 30, 1)
        .await?;
    backend
        .start_attempts_batch(&["default".to_string()], &[job_id], "sqlite-owner")
        .await?;

    let wrong_owner = backend.cancel_job(job_id, Some("sqlite-other")).await;
    assert!(wrong_owner.is_err());

    backend.cancel_job(job_id, Some("sqlite-owner")).await?;

    let job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "canceled");
    assert!(job.locked_by.is_none());
    assert!(job.lock_expires_at.is_none());

    Ok(())
}

#[tokio::test]
async fn postgres_cancel_queued_job_when_available() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    let backend = PostgresBackend::new(pool);

    let job_id = backend
        .enqueue(Job::new("cancel_postgres", json!({"backend": "postgres"})).into())
        .await?;
    backend.cancel_job(job_id, None).await?;

    let job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "canceled");

    Ok(())
}

#[tokio::test]
async fn redis_cancel_queued_job_when_available() -> anyhow::Result<()> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let client = match quickstart(&redis_url).await {
        Ok(c) => c,
        Err(_) => {
            eprintln!(
                "Skipping live Redis cancellation test: No Redis server reachable at {redis_url}"
            );
            return Ok(());
        }
    };

    let job_id = client
        .enqueue(
            Job::new("cancel_redis", json!({"backend": "redis"}))
                .queue(format!("cancel-{}", uuid::Uuid::new_v4())),
        )
        .await?;
    client.cancel_job(job_id, None).await?;

    let job = client.backend().get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "canceled");

    Ok(())
}
