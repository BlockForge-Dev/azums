mod common;

use azums::{make_sqlite_pool, Job, JobsRepo, PostgresBackend, SqliteBackend, StorageBackend};
use serde_json::json;
use sqlx::Connection;
use std::process::Command;
use uuid::Uuid;

async fn sqlite_backend(name: &str) -> anyhow::Result<SqliteBackend> {
    let db_url = format!(
        "sqlite://file:{name}_{}?mode=memory&cache=shared",
        Uuid::new_v4()
    );
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_state (
            id TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        "#,
    )
    .execute(backend.pool())
    .await?;
    Ok(backend)
}

async fn sqlite_counts(backend: &SqliteBackend, app_id: &str, job_type: &str) -> (i64, i64) {
    let app_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_state WHERE id = ?")
        .bind(app_id)
        .fetch_one(backend.pool())
        .await
        .unwrap();
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE job_type = ?")
        .bind(job_type)
        .fetch_one(backend.pool())
        .await
        .unwrap();
    (app_count, job_count)
}

#[tokio::test]
async fn sqlite_transactional_enqueue_commit_and_rollback_boundaries() -> anyhow::Result<()> {
    let backend = sqlite_backend("m4_sqlite_boundaries").await?;

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("commit-app")
        .bind("committed")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("commit-job", json!({"case": "commit"})).into(),
        )
        .await?;
    tx.commit().await?;
    assert_eq!(
        sqlite_counts(&backend, "commit-app", "commit-job").await,
        (1, 1)
    );

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("before-enqueue-app")
        .bind("rollback")
        .execute(&mut *tx)
        .await?;
    tx.rollback().await?;
    assert_eq!(
        sqlite_counts(&backend, "before-enqueue-app", "before-enqueue-job").await,
        (0, 0)
    );

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("after-enqueue-app")
        .bind("rollback")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("after-enqueue-job", json!({"case": "rollback"})).into(),
        )
        .await?;
    tx.rollback().await?;
    assert_eq!(
        sqlite_counts(&backend, "after-enqueue-app", "after-enqueue-job").await,
        (0, 0)
    );

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("before-commit-app")
        .bind("dropped")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("before-commit-job", json!({"case": "drop"})).into(),
        )
        .await?;
    drop(tx);
    assert_eq!(
        sqlite_counts(&backend, "before-commit-app", "before-commit-job").await,
        (0, 0)
    );

    Ok(())
}

#[tokio::test]
async fn sqlite_transactional_enqueue_commit_failure_rolls_back_job_and_app_state(
) -> anyhow::Result<()> {
    let backend = sqlite_backend("m4_sqlite_commit_failure").await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS commit_parent (
            id INTEGER PRIMARY KEY
        );
        "#,
    )
    .execute(backend.pool())
    .await?;
    sqlx::query("DROP TABLE IF EXISTS commit_child")
        .execute(backend.pool())
        .await?;
    sqlx::query(
        r#"
        CREATE TABLE commit_child (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NOT NULL,
            FOREIGN KEY(parent_id) REFERENCES commit_parent(id) DEFERRABLE INITIALLY DEFERRED
        )
        "#,
    )
    .execute(backend.pool())
    .await?;

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("commit-fail-app")
        .bind("should-rollback")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO commit_child (id, parent_id) VALUES (?, ?)")
        .bind(1_i64)
        .bind(404_i64)
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("commit-fail-job", json!({"case": "commit-failure"})).into(),
        )
        .await?;

    let commit = tx.commit().await;
    assert!(commit.is_err(), "deferred FK should fail during commit");
    assert_eq!(
        sqlite_counts(&backend, "commit-fail-app", "commit-fail-job").await,
        (0, 0)
    );

    Ok(())
}

#[test]
fn sqlite_process_termination_rolls_back_uncommitted_transaction() -> anyhow::Result<()> {
    let db_path = std::env::temp_dir().join(format!("azums-m4-{}.db", Uuid::new_v4()));
    let exe = std::env::current_exe()?;
    let status = Command::new(exe)
        .arg("--exact")
        .arg("sqlite_child_process_exits_with_uncommitted_transaction")
        .arg("--nocapture")
        .env("AZUMS_M4_CHILD_DB", &db_path)
        .status()?;

    assert!(
        !status.success(),
        "child process should exit abruptly without committing"
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = make_sqlite_pool(&db_url).await?;
        let backend = SqliteBackend::new(pool);
        backend.run_migrations().await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS app_state (
                id TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(backend.pool())
        .await?;

        assert_eq!(
            sqlite_counts(&backend, "child-app", "child-job").await,
            (0, 0)
        );
        anyhow::Ok(())
    })?;

    let _ = std::fs::remove_file(db_path);
    Ok(())
}

#[tokio::test]
async fn sqlite_child_process_exits_with_uncommitted_transaction() -> anyhow::Result<()> {
    let Some(db_path) = std::env::var_os("AZUMS_M4_CHILD_DB") else {
        return Ok(());
    };
    let db_url = format!(
        "sqlite://{}?mode=rwc",
        std::path::PathBuf::from(db_path).display()
    );
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = SqliteBackend::new(pool);
    backend.run_migrations().await?;
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS app_state (
            id TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        "#,
    )
    .execute(backend.pool())
    .await?;

    let mut tx = backend.pool().begin().await?;
    sqlx::query("INSERT INTO app_state (id, value) VALUES (?, ?)")
        .bind("child-app")
        .bind("uncommitted")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(&mut tx, Job::new("child-job", json!({})).into())
        .await?;

    std::process::exit(9);
}

async fn setup_postgres_transaction_table(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS azums_m4_app_state (
            id text PRIMARY KEY,
            value text NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM azums_m4_app_state")
        .execute(pool)
        .await?;
    Ok(())
}

async fn postgres_counts(pool: &sqlx::PgPool, app_id: &str, job_type: &str) -> (i64, i64) {
    let app_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM azums_m4_app_state WHERE id = $1")
            .bind(app_id)
            .fetch_one(pool)
            .await
            .unwrap();
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE job_type = $1")
        .bind(job_type)
        .fetch_one(pool)
        .await
        .unwrap();
    (app_count, job_count)
}

#[tokio::test]
async fn postgres_transactional_enqueue_commit_and_rollback_boundaries() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    setup_postgres_transaction_table(&pool).await?;
    let backend = PostgresBackend::new(pool.clone());

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO azums_m4_app_state (id, value) VALUES ($1, $2)")
        .bind("pg-commit-app")
        .bind("committed")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("pg-commit-job", json!({"case": "commit"})).into(),
        )
        .await?;
    tx.commit().await?;
    assert_eq!(
        postgres_counts(&pool, "pg-commit-app", "pg-commit-job").await,
        (1, 1)
    );

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO azums_m4_app_state (id, value) VALUES ($1, $2)")
        .bind("pg-after-enqueue-app")
        .bind("rollback")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("pg-after-enqueue-job", json!({"case": "rollback"})).into(),
        )
        .await?;
    tx.rollback().await?;
    assert_eq!(
        postgres_counts(&pool, "pg-after-enqueue-app", "pg-after-enqueue-job").await,
        (0, 0)
    );

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO azums_m4_app_state (id, value) VALUES ($1, $2)")
        .bind("pg-before-commit-app")
        .bind("dropped")
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("pg-before-commit-job", json!({"case": "drop"})).into(),
        )
        .await?;
    drop(tx);
    assert_eq!(
        postgres_counts(&pool, "pg-before-commit-app", "pg-before-commit-job").await,
        (0, 0)
    );

    Ok(())
}

#[tokio::test]
async fn postgres_transactional_enqueue_commit_failure_rolls_back_job_and_app_state(
) -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    setup_postgres_transaction_table(&pool).await?;
    let backend = PostgresBackend::new(pool.clone());

    sqlx::query("DROP TABLE IF EXISTS azums_m4_commit_child")
        .execute(&pool)
        .await?;
    sqlx::query("DROP TABLE IF EXISTS azums_m4_commit_parent")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE azums_m4_commit_parent (id int PRIMARY KEY)")
        .execute(&pool)
        .await?;
    sqlx::query(
        r#"
        CREATE TABLE azums_m4_commit_child (
            id int PRIMARY KEY,
            parent_id int NOT NULL REFERENCES azums_m4_commit_parent(id) DEFERRABLE INITIALLY DEFERRED
        )
        "#,
    )
    .execute(&pool)
    .await?;

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO azums_m4_app_state (id, value) VALUES ($1, $2)")
        .bind("pg-commit-fail-app")
        .bind("should-rollback")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO azums_m4_commit_child (id, parent_id) VALUES ($1, $2)")
        .bind(1_i32)
        .bind(404_i32)
        .execute(&mut *tx)
        .await?;
    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new("pg-commit-fail-job", json!({"case": "commit-failure"})).into(),
        )
        .await?;

    let commit = tx.commit().await;
    assert!(commit.is_err(), "deferred FK should fail during commit");
    assert_eq!(
        postgres_counts(&pool, "pg-commit-fail-app", "pg-commit-fail-job").await,
        (0, 0)
    );

    Ok(())
}

#[tokio::test]
async fn postgres_transactional_enqueue_connection_loss_rolls_back_uncommitted_work(
) -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    setup_postgres_transaction_table(&pool).await?;

    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;
    let repo = JobsRepo::new(pool.clone());
    sqlx::query("INSERT INTO azums_m4_app_state (id, value) VALUES ($1, $2)")
        .bind("pg-connection-loss-app")
        .bind("uncommitted")
        .execute(&mut *tx)
        .await?;
    repo.enqueue_in_tx(
        &mut tx,
        Job::new("pg-connection-loss-job", json!({"case": "connection-drop"})).into(),
    )
    .await?;

    drop(tx);
    drop(conn);

    assert_eq!(
        postgres_counts(&pool, "pg-connection-loss-app", "pg-connection-loss-job").await,
        (0, 0)
    );

    Ok(())
}
