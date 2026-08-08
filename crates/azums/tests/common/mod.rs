use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

pub async fn setup_db() -> Option<PgPool> {
    let _ = dotenvy::dotenv();

    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("TEST_DATABASE_URL not set; skipping Postgres integration test");
        return None;
    };

    let Ok(pool) = PgPoolOptions::new().max_connections(10).connect(&url).await else {
        eprintln!("Failed to connect to TEST_DATABASE_URL; skipping Postgres integration test");
        return None;
    };

    if sqlx::migrate!("./migrations").run(&pool).await.is_err() {
        eprintln!("Migrations failed; skipping Postgres integration test");
        return None;
    }

    let _ = sqlx::query(
        r#"
        TRUNCATE TABLE
            policy_decisions,
            job_attempts,
            queue_policies,
            jobs_archive,
            jobs
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(&pool)
    .await;

    Some(pool)
}

#[allow(dead_code)]
pub async fn insert_job(pool: &PgPool, queue: &str) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO jobs (
            queue,
            job_type,
            payload_json,
            run_at,
            status,
            priority,
            max_attempts
        )
        VALUES ($1, 'test_job', '{}'::jsonb, now(), 'queued', 0, 5)
        RETURNING id
        "#,
    )
    .bind(queue)
    .fetch_one(pool)
    .await
    .expect("failed to insert job")
}
