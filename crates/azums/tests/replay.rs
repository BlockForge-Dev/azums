mod common;

use azums::jobs::JobsRepo;
use chrono::{Duration as ChronoDuration, Utc};
use common::setup_db;
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_job_full(pool: &PgPool, queue: &str, job_type: &str) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO jobs (queue, job_type, payload_json, run_at, status, priority, max_attempts)
        VALUES ($1, $2, '{"hello":"world"}'::jsonb, now(), 'succeeded', 7, 3)
        RETURNING id
        "#,
    )
    .bind(queue)
    .bind(job_type)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn replay_creates_new_job_with_lineage() {
    let Some(pool) = setup_db().await else {
        return;
    };
    let repo = JobsRepo::new(pool.clone());

    let old_id = insert_job_full(&pool, "default", "my_job").await;

    let new_id = repo.replay_job(old_id, None, None).await.unwrap();

    // new job exists
    let (queue, job_type, status, replay_of_job_id): (String, String, String, Option<Uuid>) =
        sqlx::query_as(
            r#"
            SELECT queue, job_type, status, replay_of_job_id
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(new_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status, "queued");
    assert_eq!(queue, "default");
    assert_eq!(job_type, "my_job");
    assert_eq!(replay_of_job_id, Some(old_id));
}

#[tokio::test]
async fn replay_allows_overrides() {
    let Some(pool) = setup_db().await else {
        return;
    };
    let repo = JobsRepo::new(pool.clone());

    let old_id = insert_job_full(&pool, "default", "my_job").await;

    let run_at = Utc::now() + ChronoDuration::seconds(30);
    let new_id = repo
        .replay_job(old_id, Some("priority-queue"), Some(run_at))
        .await
        .unwrap();

    let (queue, db_run_at, replay_of_job_id): (String, chrono::DateTime<Utc>, Option<Uuid>) =
        sqlx::query_as(
            r#"
            SELECT queue, run_at, replay_of_job_id
            FROM jobs
            WHERE id = $1
            "#,
        )
        .bind(new_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(queue, "priority-queue");
    assert_eq!(replay_of_job_id, Some(old_id));

    // run_at should be close (db now vs rust now differences can exist; compare >=)
    assert!(db_run_at >= run_at - ChronoDuration::seconds(1));
}
