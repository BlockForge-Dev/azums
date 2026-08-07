use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

/// Connects to a PostgreSQL database URL and returns a tuned `PgPool`.
///
/// Respects environment configuration for pool tuning:
/// - `PGFLOW_DB_MAX_CONNECTIONS`: Max connection pool size (default: `4`, range `1-32`).
/// - `PGFLOW_DB_ACQUIRE_TIMEOUT_SECS`: Connection acquire timeout in seconds (default: `10s`).
/// - `PGFLOW_DISABLE_JIT`: Disables Postgres JIT compilation on new sessions (default: `true`).
/// - `PGFLOW_DISABLE_SYNC_COMMIT`: Sets `synchronous_commit = OFF` (default: `false`).
///
/// # Examples
///
/// ```rust,no_run
/// use postgresflow::make_pool;
///
/// # async fn doc_test() -> anyhow::Result<()> {
/// let pool = make_pool("postgres://postgres:postgres@localhost:5432/postgresflow_dev").await?;
/// # Ok(())
/// # }
/// ```
pub async fn make_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let max_connections = std::env::var("PGFLOW_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(4)
        .clamp(1, 32);

    let acquire_timeout_secs = std::env::var("PGFLOW_DB_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(10)
        .clamp(1, 60);

    let disable_sync_commit = env_bool("PGFLOW_DISABLE_SYNC_COMMIT", false);
    let disable_jit = env_bool("PGFLOW_DISABLE_JIT", true);

    let mut opts = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs));

    opts = opts.after_connect(move |conn, _meta| {
        Box::pin(async move {
            if disable_sync_commit {
                sqlx::query("SET synchronous_commit = OFF")
                    .execute(&mut *conn)
                    .await?;
            }
            if disable_jit {
                sqlx::query("SET jit = OFF").execute(&mut *conn).await?;
            }
            Ok(())
        })
    });

    let pool = opts.connect(database_url).await?;

    Ok(pool)
}

/// Executes all embedded SQL schema migrations on the provided database connection pool.
///
/// # Examples
///
/// ```rust,no_run
/// use postgresflow::{make_pool, run_migrations};
///
/// # async fn doc_test() -> anyhow::Result<()> {
/// let pool = make_pool("postgres://localhost/postgresflow_dev").await?;
/// run_migrations(&pool).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
