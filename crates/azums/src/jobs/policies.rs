use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow)]
/// PostgreSQL execution limits applied to one queue.
/// # Examples
///
/// ```rust,no_run
/// use azums::PoliciesRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let policies = PoliciesRepo::new(pool);
/// let _ = policies;
/// # Ok::<(), sqlx::Error>(())
/// ```
pub struct QueuePolicy {
    /// Queue governed by this policy.
    pub queue: String,
    /// Maximum attempts that may start during one minute.
    pub max_attempts_per_minute: i32,
    /// Maximum simultaneously running jobs.
    pub max_in_flight: i32,
    /// Delay applied when the queue is throttled, in milliseconds.
    pub throttle_delay_ms: i32,
}

#[derive(Clone)]
/// PostgreSQL repository for queue execution policies.
/// # Examples
///
/// ```rust,no_run
/// use azums::PoliciesRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let policies = PoliciesRepo::new(pool);
/// let _ = policies;
/// # Ok::<(), sqlx::Error>(())
/// ```
pub struct PoliciesRepo {
    pool: PgPool,
}

/// # Examples
///
/// ```rust,no_run
/// use azums::PoliciesRepo;
/// use sqlx::postgres::PgPoolOptions;
///
/// let pool = PgPoolOptions::new()
///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
/// let policies = PoliciesRepo::new(pool);
/// let _ = policies;
/// # Ok::<(), sqlx::Error>(())
/// ```
impl PoliciesRepo {
    /// Creates a policy repository backed by `pool`.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::PoliciesRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let policies = PoliciesRepo::new(pool);
    /// let _ = policies;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Returns the policy configured for `queue`, if one exists.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::PoliciesRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let policies = PoliciesRepo::new(pool);
    /// let _ = policies;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn get_policy(&self, queue: &str) -> anyhow::Result<Option<QueuePolicy>> {
        let rec = sqlx::query_as::<_, QueuePolicy>(
            r#"
            SELECT queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms
            FROM queue_policies
            WHERE queue = $1
            "#,
        )
        .bind(queue)
        .fetch_optional(&self.pool)
        .await?;

        Ok(rec)
    }

    /// Creates or replaces the execution policy for `queue`.
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::PoliciesRepo;
    /// use sqlx::postgres::PgPoolOptions;
    ///
    /// let pool = PgPoolOptions::new()
    ///     .connect_lazy("postgres://postgres:postgres@localhost/azums")?;
    /// let policies = PoliciesRepo::new(pool);
    /// let _ = policies;
    /// # Ok::<(), sqlx::Error>(())
    /// ```
    pub async fn upsert_policy(
        &self,
        queue: &str,
        max_attempts_per_minute: i32,
        max_in_flight: i32,
        throttle_delay_ms: i32,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO queue_policies(queue, max_attempts_per_minute, max_in_flight, throttle_delay_ms)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT(queue) DO UPDATE
            SET max_attempts_per_minute = EXCLUDED.max_attempts_per_minute,
                max_in_flight = EXCLUDED.max_in_flight,
                throttle_delay_ms = EXCLUDED.throttle_delay_ms
            "#,
        )
        .bind(queue)
        .bind(max_attempts_per_minute)
        .bind(max_in_flight)
        .bind(throttle_delay_ms)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
