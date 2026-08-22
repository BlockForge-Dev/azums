use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Serialize)]
/// Point-in-time PostgreSQL queue health and throughput summary.
/// # Examples
///
/// ```rust,no_run
/// use azums::MetricsRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let metrics = MetricsRepo::new(pool);
/// let _ = metrics;
/// # Ok::<(), sqlx::Error>(())
/// ```
pub struct Metrics {
    /// Time at which the snapshot was calculated.
    pub at: DateTime<Utc>,

    /// Queue represented by the snapshot.
    pub queue: String,
    /// Number of queued jobs currently eligible to run.
    pub runnable_queue_depth: i64,

    // last 60s window
    /// Completed attempts per second over the latest 60-second window.
    pub jobs_per_sec: f64,
    /// Fraction of finished attempts that succeeded.
    pub success_rate: f64,
    /// Fraction of started attempts whose number is at least two.
    pub retry_rate: f64,
    /// Mean finished-attempt latency in milliseconds.
    pub mean_latency_ms: f64,
}

#[derive(Clone)]
/// PostgreSQL repository for queue-level operational metrics.
/// # Examples
///
/// ```rust,no_run
/// use azums::MetricsRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let metrics = MetricsRepo::new(pool);
/// let _ = metrics;
/// # Ok::<(), sqlx::Error>(())
/// ```
pub struct MetricsRepo {
    pool: PgPool,
}

/// # Examples
///
/// ```rust,no_run
/// use azums::MetricsRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let metrics = MetricsRepo::new(pool);
/// let _ = metrics;
/// # Ok::<(), sqlx::Error>(())
/// ```
impl MetricsRepo {
    /// Creates a metrics repository backed by `pool`.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::MetricsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let metrics = MetricsRepo::new(pool);
    /// let _ = metrics;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns one metrics snapshot for every known queue.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::MetricsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let metrics = MetricsRepo::new(pool);
    /// let _ = metrics;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn snapshot_all(&self) -> anyhow::Result<Vec<Metrics>> {
        let queues: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT queue
            FROM jobs
            ORDER BY queue
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(queues.len());
        for queue in queues {
            out.push(self.snapshot_for_queue(&queue).await?);
        }

        Ok(out)
    }

    /// Calculates a metrics snapshot for one queue.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::MetricsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let metrics = MetricsRepo::new(pool);
    /// let _ = metrics;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn snapshot_for_queue(&self, queue: &str) -> anyhow::Result<Metrics> {
        // Depth (runnable queued)
        let depth: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE queue = $1
              AND status = 'queued'
              AND run_at <= now()
            "#,
        )
        .bind(queue)
        .fetch_one(&self.pool)
        .await?;

        // Attempts window stats (last 60 seconds)
        // - throughput ~ attempts finished per sec
        // - success_rate = succeeded / finished
        // - retry_rate = attempts with attempt_no >=2 / total attempts started
        // - mean latency = avg(latency_ms) for finished attempts
        let row = sqlx::query_as::<
            _,
            (
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
                Option<f64>,
            ),
        >(
            r#"
            WITH a AS (
              SELECT a.*
              FROM job_attempts a
              JOIN jobs j ON j.id = a.job_id AND j.dataset_id = a.dataset_id
              WHERE j.queue = $1
                AND a.started_at >= now() - interval '60 seconds'
            ),
            finished AS (
              SELECT *
              FROM a
              WHERE finished_at IS NOT NULL
            )
            SELECT
              (SELECT COUNT(*) FROM finished)::float8 AS finished_count,
              (SELECT COUNT(*) FROM finished WHERE status = 'succeeded')::float8 AS succeeded_count,
              (SELECT COUNT(*) FROM a WHERE attempt_no >= 2)::float8 AS retry_count,
              (SELECT COUNT(*) FROM a)::float8 AS started_count,
              COALESCE((SELECT AVG(latency_ms)::float8 FROM finished), 0.0) AS mean_latency_ms
            "#,
        )
        .bind(queue)
        .fetch_one(&self.pool)
        .await?;

        let finished_count = row.0.unwrap_or(0.0);
        let succeeded_count = row.1.unwrap_or(0.0);
        let retry_count = row.2.unwrap_or(0.0);
        let started_count = row.3.unwrap_or(0.0);
        let mean_latency_ms = row.4.unwrap_or(0.0);

        let jobs_per_sec = finished_count / 60.0;

        let success_rate = if finished_count > 0.0 {
            succeeded_count / finished_count
        } else {
            0.0
        };

        let retry_rate = if started_count > 0.0 {
            retry_count / started_count
        } else {
            0.0
        };

        Ok(Metrics {
            at: Utc::now(),
            queue: queue.to_string(),
            runnable_queue_depth: depth,
            jobs_per_sec,
            success_rate,
            retry_rate,
            mean_latency_ms,
        })
    }
}
