use azums::{make_sqlite_pool, Job, JobsRepo, PostgresBackend, SqliteBackend, StorageBackend};
use serde_json::json;
use serial_test::serial;
use sqlx::Acquire;
use std::{process::Command, time::Duration};
use uuid::Uuid;

mod common;

async fn sqlite_file_backend(path: &std::path::Path) -> anyhow::Result<SqliteBackend> {
    let db_url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;
    Ok(backend)
}

async fn sqlite_job_by_type(backend: &SqliteBackend, job_type: &str) -> anyhow::Result<Job> {
    sqlx::query_as::<_, Job>(
        r#"
        SELECT
            dataset_id, replay_of_job_id, idempotency_key, id, queue, job_type,
            payload_json, run_at, deadline_at, timeout_seconds, recurring_interval_seconds,
            status, priority, max_attempts,
            locked_at, locked_by, lock_expires_at, dlq_reason_code, dlq_at,
            created_at, updated_at
        FROM jobs
        WHERE job_type = ?
        "#,
    )
    .bind(job_type)
    .fetch_one(backend.pool())
    .await
    .map_err(Into::into)
}

async fn sqlite_attempts(
    backend: &SqliteBackend,
    job_id: Uuid,
) -> anyhow::Result<Vec<(i32, String, Option<String>)>> {
    sqlx::query_as(
        r#"
        SELECT attempt_no, status, error_code
        FROM job_attempts
        WHERE job_id = ?
        ORDER BY attempt_no
        "#,
    )
    .bind(job_id)
    .fetch_all(backend.pool())
    .await
    .map_err(Into::into)
}

fn wait_for_marker(path: &std::path::Path) -> anyhow::Result<()> {
    for _ in 0..400 {
        if path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!(
        "child process did not reach crash point marker: {}",
        path.display()
    )
}

fn run_sqlite_child_crash_mode(mode: &str, starts_attempt: bool) -> anyhow::Result<()> {
    let db_path = std::env::temp_dir().join(format!("azums-m5-{mode}-{}.db", Uuid::new_v4()));
    let ready_path = std::env::temp_dir().join(format!("azums-m5-{mode}-{}.ready", Uuid::new_v4()));
    let exe = std::env::current_exe()?;

    if mode == "before_claim" {
        let status = Command::new(&exe)
            .arg("--exact")
            .arg("sqlite_lease_recovery_child")
            .arg("--nocapture")
            .env("AZUMS_M5_CHILD_DB", &db_path)
            .env("AZUMS_M5_CHILD_MODE", mode)
            .env("AZUMS_M5_CHILD_READY", &ready_path)
            .status()?;
        assert!(!status.success(), "child should crash before claim");
        wait_for_marker(&ready_path)?;
    } else {
        let mut child = Command::new(&exe)
            .arg("--exact")
            .arg("sqlite_lease_recovery_child")
            .arg("--nocapture")
            .env("AZUMS_M5_CHILD_DB", &db_path)
            .env("AZUMS_M5_CHILD_MODE", mode)
            .env("AZUMS_M5_CHILD_READY", &ready_path)
            .spawn()?;

        wait_for_marker(&ready_path)?;
        child.kill()?;
        let _ = child.wait()?;
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let backend = sqlite_file_backend(&db_path).await?;
        let job = sqlite_job_by_type(&backend, mode).await?;

        assert_ne!(job.status, "succeeded", "crashed child must not ACK {mode}");
        assert_eq!(job.job_type, mode);

        tokio::time::sleep(Duration::from_millis(1300)).await;
        let reaped = backend.reap_expired_locks().await?;
        if mode == "before_claim" {
            assert_eq!(reaped, 0);
        } else {
            assert_eq!(reaped, 1, "expired lease should be recovered for {mode}");
        }

        let recovered = backend.get_job(job.id).await?.unwrap();
        assert_eq!(recovered.status, "queued");
        assert!(recovered.locked_by.is_none());
        assert!(recovered.lock_expires_at.is_none());

        let attempts = sqlite_attempts(&backend, job.id).await?;
        if starts_attempt {
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0].1, "failed");
            assert_eq!(attempts[0].2.as_deref(), Some("LEASE_EXPIRED"));
        } else {
            assert!(attempts.is_empty());
        }

        let leased = backend
            .lease_jobs_batch("default", "m5-parent", 30, 1)
            .await?;
        assert_eq!(leased.len(), 1);
        assert_eq!(leased[0].id, job.id);

        let attempt = backend
            .start_attempts_batch(
                &[leased[0].dataset_id.clone()],
                &[leased[0].id],
                "m5-parent",
            )
            .await?;
        backend
            .mark_succeeded(leased[0].id, attempt[0].1, "m5-parent", 7)
            .await?;

        let final_job = backend.get_job(job.id).await?.unwrap();
        assert_eq!(final_job.status, "succeeded");

        anyhow::Ok(())
    })?;

    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(ready_path);
    Ok(())
}

#[test]
#[serial]
fn sqlite_worker_crash_before_claim_preserves_committed_job() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("before_claim", false)
}

#[test]
#[serial]
fn sqlite_worker_kill_after_claim_recovers_job() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("after_claim", false)
}

#[test]
#[serial]
fn sqlite_worker_kill_during_execution_closes_attempt_and_recovers_job() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("during_execution", true)
}

#[test]
#[serial]
fn sqlite_worker_kill_immediately_before_ack_recovers_job() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("before_ack", true)
}

#[test]
#[serial]
fn sqlite_worker_kill_after_handler_success_before_ack_recovers_job() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("after_handler_success", true)
}

#[test]
#[serial]
fn sqlite_worker_kill_during_heartbeat_recovers_after_last_lease_expires() -> anyhow::Result<()> {
    run_sqlite_child_crash_mode("during_heartbeat", true)
}

#[tokio::test]
async fn sqlite_database_disconnect_rolls_back_claim_without_losing_job() -> anyhow::Result<()> {
    let db_path = std::env::temp_dir().join(format!("azums-m5-disconnect-{}.db", Uuid::new_v4()));
    let backend = sqlite_file_backend(&db_path).await?;
    let job_id = backend
        .enqueue(Job::new("db_disconnect", json!({})).into())
        .await?;

    let mut tx = backend.pool().begin().await?;
    sqlx::query("UPDATE jobs SET status = 'running', locked_by = 'lost-db' WHERE id = ?")
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
    drop(tx);

    let job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "queued");
    assert!(job.locked_by.is_none());

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[tokio::test]
async fn sqlite_lease_recovery_child() -> anyhow::Result<()> {
    let Some(db_path) = std::env::var_os("AZUMS_M5_CHILD_DB") else {
        return Ok(());
    };
    let mode = std::env::var("AZUMS_M5_CHILD_MODE")?;
    let ready_path = std::env::var_os("AZUMS_M5_CHILD_READY")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("AZUMS_M5_CHILD_READY missing"))?;

    let backend = sqlite_file_backend(&std::path::PathBuf::from(db_path)).await?;
    let job_id = backend.enqueue(Job::new(&mode, json!({})).into()).await?;

    if mode == "before_claim" {
        std::fs::write(&ready_path, b"ready")?;
        std::process::exit(9);
    }

    let leased = backend
        .lease_jobs_batch("default", "m5-child", 1, 1)
        .await?;
    assert_eq!(leased.len(), 1);
    assert_eq!(leased[0].id, job_id);

    if mode == "after_claim" {
        std::fs::write(&ready_path, b"ready")?;
        tokio::time::sleep(Duration::from_secs(60)).await;
        return Ok(());
    }

    let attempt = backend
        .start_attempts_batch(&[leased[0].dataset_id.clone()], &[job_id], "m5-child")
        .await?;
    assert_eq!(attempt.len(), 1);

    std::fs::write(&ready_path, b"ready")?;

    if mode == "during_heartbeat" {
        loop {
            let extended = backend.extend_lease(job_id, "m5-child", 1).await?;
            assert!(extended);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    tokio::time::sleep(Duration::from_secs(60)).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn postgres_expired_lease_recovery_closes_attempt_and_requeues() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    let backend = PostgresBackend::new(pool.clone());
    let job_id = backend
        .enqueue(Job::new("pg_lease_recovery", json!({})).into())
        .await?;

    let leased = backend
        .lease_jobs_batch("default", "pg-dead-worker", 2, 1)
        .await?;
    assert_eq!(leased.len(), 1);
    let attempt = backend
        .start_attempts_batch(&[leased[0].dataset_id.clone()], &[job_id], "pg-dead-worker")
        .await?;
    assert_eq!(attempt.len(), 1);

    tokio::time::sleep(Duration::from_millis(2200)).await;
    assert_eq!(backend.reap_expired_locks().await?, 1);

    let job = backend.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "queued");
    assert!(job.locked_by.is_none());

    let attempts: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT attempt_no, status, error_code
        FROM job_attempts
        WHERE job_id = $1
        ORDER BY attempt_no
        "#,
    )
    .bind(job_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        attempts,
        vec![(1, "failed".to_string(), Some("LEASE_EXPIRED".to_string()))]
    );

    let recovered = backend
        .lease_jobs_batch("default", "pg-recovery-worker", 30, 1)
        .await?;
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, job_id);

    Ok(())
}

#[tokio::test]
#[serial]
async fn postgres_connection_loss_rolls_back_uncommitted_claim() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    let repo = JobsRepo::new(pool.clone());
    let job_id = repo
        .enqueue_now("default", "pg_claim_connection_loss", json!({}))
        .await?;

    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    sqlx::query("UPDATE jobs SET status = 'running', locked_by = 'lost-conn' WHERE id = $1")
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
    drop(tx);
    drop(conn);

    let job = repo.get_job(job_id).await?.unwrap();
    assert_eq!(job.status, "queued");
    assert!(job.locked_by.is_none());
    Ok(())
}
