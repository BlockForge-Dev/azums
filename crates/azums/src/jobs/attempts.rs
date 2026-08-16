use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
/// Durable record of one handler execution attempt.
pub struct JobAttempt {
    /// Unique attempt identifier.
    pub id: Uuid,
    /// Job executed by this attempt.
    pub job_id: Uuid,
    /// Monotonically increasing attempt number for the job.
    pub attempt_no: i32,

    /// Time at which handler execution started.
    pub started_at: DateTime<Utc>,
    /// Time at which the attempt finished, if terminal.
    pub finished_at: Option<DateTime<Utc>>,

    /// Persisted attempt status.
    pub status: String,

    /// Machine-readable failure code, if the attempt failed.
    pub error_code: Option<String>,
    /// Human-readable failure detail, if the attempt failed.
    pub error_message: Option<String>,

    /// Measured execution latency in milliseconds.
    pub latency_ms: Option<i32>,
    /// Worker that owned the attempt.
    pub worker_id: String,
}

/// Persisted execution-attempt status.
pub enum AttemptStatus {
    /// Handler execution is in progress.
    Running,
    /// Handler execution completed successfully.
    Succeeded,
    /// Handler execution failed.
    Failed,
}

impl AttemptStatus {
    /// Returns the compact status stored by SQL backends.
    pub fn as_str(&self) -> &'static str {
        match self {
            AttemptStatus::Running => "running",
            AttemptStatus::Succeeded => "succeeded",
            AttemptStatus::Failed => "failed",
        }
    }
}

#[derive(Clone)]
/// PostgreSQL repository for durable job-attempt history.
pub struct AttemptsRepo {
    pool: PgPool,
}

impl AttemptsRepo {
    /// Creates an attempt repository backed by `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert attempt row as "running", auto-increment attempt_no per job.
    pub async fn start_attempt(&self, job_id: Uuid, worker_id: &str) -> anyhow::Result<JobAttempt> {
        let dataset_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT dataset_id
            FROM jobs
            WHERE id = $1
            LIMIT 1
            "#,
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await?;

        self.start_attempt_for_dataset(&dataset_id, job_id, worker_id)
            .await
    }

    /// Insert attempt row as "running" when caller already knows dataset_id.
    pub async fn start_attempt_for_dataset(
        &self,
        dataset_id: &str,
        job_id: Uuid,
        worker_id: &str,
    ) -> anyhow::Result<JobAttempt> {
        let claim_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE dataset_id = $1
              AND id = $2
              AND status = 'running'
              AND locked_by = $3
            "#,
        )
        .bind(dataset_id)
        .bind(job_id)
        .bind(worker_id)
        .fetch_one(&self.pool)
        .await?;
        if claim_count != 1 {
            anyhow::bail!(
                "cannot start attempt for job {job_id}: expected running lease held by {worker_id}"
            );
        }

        let status = AttemptStatus::Running.as_str();

        let attempt = sqlx::query_as::<_, JobAttempt>(
            r#"
            INSERT INTO job_attempts (dataset_id, job_id, attempt_no, status, worker_id)
            VALUES (
              $1,
              $2,
              COALESCE(
                (SELECT MAX(attempt_no) FROM job_attempts WHERE job_id = $2 AND dataset_id = $1),
                0
              ) + 1,
              $3,
              $4
            )
            RETURNING *
            "#,
        )
        .bind(dataset_id)
        .bind(job_id)
        .bind(status)
        .bind(worker_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(attempt)
    }

    /// Insert many "running" attempts in one round-trip.
    /// Returns tuples of (job_id, attempt_id, attempt_no).
    pub async fn start_attempts_batch(
        &self,
        dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>> {
        if dataset_ids.is_empty() || job_ids.is_empty() {
            return Ok(Vec::new());
        }
        if dataset_ids.len() != job_ids.len() {
            anyhow::bail!("dataset_ids and job_ids length mismatch");
        }

        let status = AttemptStatus::Running.as_str();

        let rows = sqlx::query_as::<_, (Uuid, Uuid, i32)>(
            r#"
            WITH input AS (
              SELECT *
              FROM unnest($1::text[], $2::uuid[]) AS t(dataset_id, job_id)
            ),
            inserted AS (
              INSERT INTO job_attempts (dataset_id, job_id, attempt_no, status, worker_id)
              SELECT
                i.dataset_id,
                i.job_id,
                COALESCE(
                  (
                    SELECT MAX(a.attempt_no)
                    FROM job_attempts a
                    WHERE a.dataset_id = i.dataset_id
                      AND a.job_id = i.job_id
                  ),
                  0
                ) + 1,
                $3,
                $4
              FROM input i
              JOIN jobs j
                ON j.dataset_id = i.dataset_id
               AND j.id = i.job_id
               AND j.status = 'running'
               AND j.locked_by = $4
              RETURNING job_id, id, attempt_no
            )
            SELECT job_id, id, attempt_no
            FROM inserted
            "#,
        )
        .bind(dataset_ids)
        .bind(job_ids)
        .bind(status)
        .bind(worker_id)
        .fetch_all(&self.pool)
        .await?;
        if rows.len() != job_ids.len() {
            anyhow::bail!(
                "cannot start attempts: every job must be running under lease held by {worker_id}"
            );
        }

        Ok(rows)
    }

    /// Marks an attempt succeeded and records its final latency.
    pub async fn finish_succeeded(&self, attempt_id: Uuid, latency_ms: i32) -> anyhow::Result<()> {
        let status = AttemptStatus::Succeeded.as_str();

        sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = $2,
                finished_at = now(),
                latency_ms = $3
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(status)
        .bind(latency_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fast-path for successful batch execution: updates many attempts in one statement.
    pub async fn finish_succeeded_batch(&self, updates: &[(Uuid, i32)]) -> anyhow::Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let attempt_ids: Vec<Uuid> = updates.iter().map(|(attempt_id, _)| *attempt_id).collect();
        let latencies_ms: Vec<i32> = updates.iter().map(|(_, latency_ms)| *latency_ms).collect();
        let status = AttemptStatus::Succeeded.as_str();

        sqlx::query(
            r#"
            WITH data AS (
              SELECT
                unnest($1::uuid[]) AS attempt_id,
                unnest($2::int4[]) AS latency_ms
            )
            UPDATE job_attempts a
            SET status = $3,
                finished_at = now(),
                latency_ms = d.latency_ms
            FROM data d
            WHERE a.id = d.attempt_id
            "#,
        )
        .bind(&attempt_ids)
        .bind(&latencies_ms)
        .bind(status)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Marks an attempt failed with its final latency and error details.
    pub async fn finish_failed(
        &self,
        attempt_id: Uuid,
        latency_ms: i32,
        error_code: &str,
        error_message: &str,
    ) -> anyhow::Result<()> {
        let status = AttemptStatus::Failed.as_str();

        sqlx::query(
            r#"
            UPDATE job_attempts
            SET status = $2,
                finished_at = now(),
                latency_ms = $3,
                error_code = $4,
                error_message = $5
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(status)
        .bind(latency_ms)
        .bind(error_code)
        .bind(error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Lists a job's attempts in ascending attempt-number order.
    pub async fn list_attempts_for_job(&self, job_id: Uuid) -> anyhow::Result<Vec<JobAttempt>> {
        let rows = sqlx::query_as::<_, JobAttempt>(
            r#"
            SELECT *
            FROM job_attempts
            WHERE job_id = $1
            ORDER BY attempt_no ASC
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
