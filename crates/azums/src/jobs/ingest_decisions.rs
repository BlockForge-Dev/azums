use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
/// PostgreSQL repository for producer admission audit decisions.
/// # Examples
///
/// ```rust,no_run
/// use azums::IngestDecisionsRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let decisions = IngestDecisionsRepo::new(pool);
/// let _ = decisions;
/// # Ok::<(), sqlx::Error>(())
/// ```
pub struct IngestDecisionsRepo {
    pool: PgPool,
}

/// # Examples
///
/// ```rust,no_run
/// use azums::IngestDecisionsRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let decisions = IngestDecisionsRepo::new(pool);
/// let _ = decisions;
/// # Ok::<(), sqlx::Error>(())
/// ```
impl IngestDecisionsRepo {
    /// Creates an ingest-decision repository backed by `pool`.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::IngestDecisionsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let decisions = IngestDecisionsRepo::new(pool);
    /// let _ = decisions;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Records an admission decision and returns its generated identifier.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::IngestDecisionsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let decisions = IngestDecisionsRepo::new(pool);
    /// let _ = decisions;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn record(
        &self,
        queue: &str,
        decision: &str,
        reason_code: &str,
        details: Value,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO ingest_decisions (id, queue, decision, reason_code, details_json)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(queue)
        .bind(decision)
        .bind(reason_code)
        .bind(details)
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    /// Lists recent decisions, optionally restricted to one queue.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::IngestDecisionsRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let decisions = IngestDecisionsRepo::new(pool);
    /// let _ = decisions;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn list_recent(
        &self,
        queue: Option<&str>,
        limit: i64,
    ) -> anyhow::Result<
        Vec<(
            Uuid,
            String,
            String,
            String,
            serde_json::Value,
            chrono::DateTime<chrono::Utc>,
        )>,
    > {
        let rows = match queue {
            Some(q) => {
                sqlx::query_as::<
                    _,
                    (
                        Uuid,
                        String,
                        String,
                        String,
                        serde_json::Value,
                        chrono::DateTime<chrono::Utc>,
                    ),
                >(
                    r#"
                    SELECT id, queue, decision, reason_code, details_json, created_at
                    FROM ingest_decisions
                    WHERE queue = $1
                    ORDER BY created_at DESC
                    LIMIT $2
                    "#,
                )
                .bind(q)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<
                    _,
                    (
                        Uuid,
                        String,
                        String,
                        String,
                        serde_json::Value,
                        chrono::DateTime<chrono::Utc>,
                    ),
                >(
                    r#"
                    SELECT id, queue, decision, reason_code, details_json, created_at
                    FROM ingest_decisions
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows)
    }
}
