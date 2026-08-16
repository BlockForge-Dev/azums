use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

#[derive(Clone)]
/// PostgreSQL repository for archival, history pruning, and table maintenance.
pub struct MaintenanceRepo {
    pool: PgPool,
}

impl MaintenanceRepo {
    /// Creates a maintenance repository backed by `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Move succeeded jobs older than `cutoff` into jobs_archive (idempotent).
    /// Returns number archived.
    pub async fn archive_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        batch: i64,
    ) -> anyhow::Result<u64> {
        let mut tx = self.pool.begin().await?;

        // Best-effort monthly partition bootstrap (no-op on older schemas).
        sqlx::query(
            r#"
            DO $$
            BEGIN
              IF to_regprocedure('public.ensure_jobs_archive_partition(timestamp with time zone)') IS NOT NULL THEN
                PERFORM public.ensure_jobs_archive_partition(now());
                PERFORM public.ensure_jobs_archive_partition(now() + interval '1 month');
              END IF;
            END
            $$;
            "#,
        )
        .execute(&mut *tx)
        .await?;

        // Insert into archive while avoiding duplicates by id.
        let _inserted = sqlx::query(
            r#"
            WITH candidates AS (
                SELECT
                  id, replay_of_job_id,
                  queue, job_type, payload_json,
                  run_at, status, priority, max_attempts,
                  dlq_reason_code, dlq_at,
                  created_at, updated_at
                FROM jobs
                WHERE status = 'succeeded'
                  AND updated_at < $1
                ORDER BY updated_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT $2
            )
            INSERT INTO jobs_archive (
              id, replay_of_job_id,
              queue, job_type, payload_json,
              run_at, status, priority, max_attempts,
              dlq_reason_code, dlq_at,
              created_at, updated_at
            )
            SELECT
              c.id, c.replay_of_job_id,
              c.queue, c.job_type, c.payload_json,
              c.run_at, c.status, c.priority, c.max_attempts,
              c.dlq_reason_code, c.dlq_at,
              c.created_at, c.updated_at
            FROM candidates c
            WHERE NOT EXISTS (
              SELECT 1
              FROM jobs_archive a
              WHERE a.id = c.id
            )
            "#,
        )
        .bind(cutoff)
        .bind(batch)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        // Delete archived jobs from main table
        // Only delete jobs that exist in archive (safety)
        let deleted = sqlx::query(
            r#"
            DELETE FROM jobs j
            USING jobs_archive a
            WHERE j.id = a.id
              AND j.status = 'succeeded'
              AND j.updated_at < $1
            "#,
        )
        .bind(cutoff)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        tx.commit().await?;

        // deleted is the true "archived & removed" count
        Ok(deleted)
    }

    /// Delete attempts + policy decisions for succeeded jobs older than `cutoff`.
    /// Returns (attempts_deleted, policy_deleted).
    pub async fn delete_history_for_succeeded_older_than(
        &self,
        cutoff: DateTime<Utc>,
        batch: i64,
    ) -> anyhow::Result<(u64, u64)> {
        let mut tx = self.pool.begin().await?;

        // pick job ids in small batches
        let job_ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM jobs
            WHERE status = 'succeeded'
              AND updated_at < $1
            ORDER BY updated_at ASC
            LIMIT $2
            "#,
        )
        .bind(cutoff)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?;

        if job_ids.is_empty() {
            tx.commit().await?;
            return Ok((0, 0));
        }

        let attempts_deleted = sqlx::query(
            r#"
            DELETE FROM job_attempts
            WHERE job_id = ANY($1)
            "#,
        )
        .bind(&job_ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        let policy_deleted = sqlx::query(
            r#"
            DELETE FROM policy_decisions
            WHERE job_id = ANY($1)
            "#,
        )
        .bind(&job_ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        tx.commit().await?;
        Ok((attempts_deleted, policy_deleted))
    }

    /// Executes VACUUM ANALYZE on core job queue tables outside active transactions.
    pub async fn vacuum_analyze(&self) -> anyhow::Result<()> {
        let tables = [
            "jobs",
            "job_attempts",
            "stream_events",
            "policy_decisions",
            "jobs_archive",
        ];
        for table in tables {
            let sql = format!("VACUUM ANALYZE {table}");
            let _ = sqlx::query(&sql).execute(&self.pool).await;
        }
        Ok(())
    }

    /// Queries `pg_stat_user_tables` for dead tuple counts, live tuple counts, and vacuum timestamps.
    pub async fn get_maintenance_status(&self) -> anyhow::Result<Vec<TableMaintenanceInfo>> {
        let rows = sqlx::query_as::<_, TableMaintenanceInfo>(
            r#"
            SELECT
              relname AS table_name,
              COALESCE(n_dead_tup, 0) AS dead_tuples,
              COALESCE(n_live_tup, 0) AS live_tuples,
              last_vacuum,
              last_autovacuum,
              last_analyze,
              last_autoanalyze
            FROM pg_stat_user_tables
            WHERE relname IN ('jobs', 'job_attempts', 'stream_events', 'policy_decisions', 'jobs_archive')
            ORDER BY relname ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}

/// Statistics and vacuum status metadata for a database table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct TableMaintenanceInfo {
    /// PostgreSQL relation name.
    pub table_name: String,
    /// Estimated dead tuple count.
    pub dead_tuples: i64,
    /// Estimated live tuple count.
    pub live_tuples: i64,
    /// Most recent manual vacuum time.
    pub last_vacuum: Option<DateTime<Utc>>,
    /// Most recent automatic vacuum time.
    pub last_autovacuum: Option<DateTime<Utc>>,
    /// Most recent manual analyze time.
    pub last_analyze: Option<DateTime<Utc>>,
    /// Most recent automatic analyze time.
    pub last_autoanalyze: Option<DateTime<Utc>>,
}

/// Convenience: compute cutoff like "now - N days"
pub fn cutoff_days(days: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(days)
}
