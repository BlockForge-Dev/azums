/// Application and worker runtime configuration loaded from environment variables or `.env`.
///
/// Supported environment variables:
/// - `DATABASE_URL`: Postgres database connection string (required).
/// - `AZUMS_WORKER_ID`: Unique worker identifier string (default: container hostname or `"worker-1"`).
/// - `AZUMS_QUEUE`: Default queue name to poll (default: `"default"`).
/// - `AZUMS_LEASE_SECONDS`: Lease lock duration in seconds (default: `10s`).
/// - `AZUMS_DEQUEUE_BATCH_SIZE`: Maximum jobs leased per batch query (default: `256`, max `4096`).
/// - `AZUMS_REAP_INTERVAL_MS`: Expired lock reaping interval in milliseconds (default: `5000ms`).
/// - `AZUMS_VERBOSE_JOB_LOGS`: Enable verbose execution logging (`1`/`true`/`0`/`false`).
/// - `AZUMS_ADMIN_ADDR`: Admin HTTP API bind address (e.g., `"0.0.0.0:3003"` or `"off"`).
/// - `AZUMS_API_TOKEN`: Optional API key token required in `x-api-key` header for Admin REST endpoints.
/// - `AZUMS_MIGRATE_ON_STARTUP`: Run SQL migrations automatically on startup (`1`/`0`).
/// - `AZUMS_MAX_PAYLOAD_BYTES`: Maximum allowed job payload size in bytes (default: `262144` / 256KB).
/// - `AZUMS_MAX_ENQUEUE_PER_MINUTE`: Maximum enqueues per minute per queue (default: `10000`).
/// # Examples
///
/// ```rust,no_run
/// use azums::Config;
///
/// let config = Config::from_env()?;
/// assert!(config.lease_seconds > 0);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct Config {
    /// Storage connection URL used by the runtime.
    pub database_url: String,
    /// Unique identity attached to leases and attempts.
    pub worker_id: String,
    /// Queue polled by this worker.
    pub queue: String,
    /// Duration of each job lease in seconds.
    pub lease_seconds: i64,
    /// Maximum jobs requested by one dequeue operation.
    pub dequeue_batch_size: i64,
    /// Interval between expired-lease recovery passes in milliseconds.
    pub reap_interval_ms: u64,
    /// Whether detailed per-job logs are emitted.
    pub verbose_job_logs: bool,
    /// Optional admin API bind address; `None` disables the listener.
    pub admin_addr: Option<String>,
    /// Optional token required by protected admin endpoints.
    pub api_token: Option<String>,
    /// Whether database migrations run during application startup.
    pub migrate_on_startup: bool,
    /// Maximum accepted serialized payload size in bytes.
    pub max_payload_bytes: usize,
    /// Maximum enqueue operations accepted per queue and minute.
    pub max_enqueues_per_minute_per_queue: i64,
    /// Interval between automatic maintenance passes in seconds.
    pub maintenance_interval_secs: u64,
    /// Number of SQLite pages requested by each incremental vacuum pass.
    pub sqlite_incremental_vacuum_n: u64,
}

/// # Examples
///
/// ```rust,no_run
/// use azums::Config;
///
/// let config = Config::from_env()?;
/// assert!(config.lease_seconds > 0);
/// # Ok::<(), anyhow::Error>(())
/// ```
impl Config {
    /// Loads configuration settings from environment variables and `.env`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use azums::Config;
    ///
    /// # fn doc_test() -> anyhow::Result<()> {
    /// std::env::set_var("DATABASE_URL", "postgres://localhost/azums_dev");
    /// let cfg = Config::from_env()?;
    /// assert_eq!(cfg.queue, "default");
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL is missing"))?;
        //.map_err(...) converts that error into an anyhow::Error
        //std::env::var returns Result<String, VarError>

        let worker_id = env_or_fallback("AZUMS_WORKER_ID", "WORKER_ID")
            .or_else(|| std::env::var("HOSTNAME").ok())
            .unwrap_or_else(|| "worker-1".to_string());

        let queue =
            env_or_fallback("AZUMS_QUEUE", "QUEUE").unwrap_or_else(|| "default".to_string());

        let lease_seconds = env_or_fallback("AZUMS_LEASE_SECONDS", "LEASE_SECONDS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let dequeue_batch_size = env_or_fallback("AZUMS_DEQUEUE_BATCH_SIZE", "DEQUEUE_BATCH_SIZE")
            .and_then(|s| s.parse().ok())
            .unwrap_or(256)
            .clamp(1, 4096);

        let reap_interval_ms = env_or_fallback("AZUMS_REAP_INTERVAL_MS", "REAP_INTERVAL_MS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(5_000)
            .clamp(250, 60_000);

        let verbose_job_logs = env_bool("AZUMS_VERBOSE_JOB_LOGS").unwrap_or(false);

        let admin_addr = env_or_fallback("AZUMS_ADMIN_ADDR", "ADMIN_ADDR")
            .and_then(|s| normalize_optional_addr(&s));

        let api_token = env_or_fallback("AZUMS_API_TOKEN", "API_TOKEN");

        let migrate_on_startup = env_bool("AZUMS_MIGRATE_ON_STARTUP").unwrap_or(false);

        let max_payload_bytes = env_or_fallback("AZUMS_MAX_PAYLOAD_BYTES", "MAX_PAYLOAD_BYTES")
            .and_then(|s| s.parse().ok())
            .unwrap_or(256 * 1024);

        let max_enqueues_per_minute_per_queue =
            env_or_fallback("AZUMS_MAX_ENQUEUE_PER_MINUTE", "MAX_ENQUEUE_PER_MINUTE")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000);

        let maintenance_interval_secs = env_or_fallback(
            "AZUMS_MAINTENANCE_INTERVAL_SECS",
            "MAINTENANCE_INTERVAL_SECS",
        )
        .and_then(|s| s.parse().ok())
        .unwrap_or(300)
        .clamp(10, 86400);

        let sqlite_incremental_vacuum_n = env_or_fallback(
            "AZUMS_SQLITE_INCREMENTAL_VACUUM_N",
            "SQLITE_INCREMENTAL_VACUUM_N",
        )
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
        .clamp(1, 100_000);

        Ok(Self {
            database_url,
            worker_id,
            queue,
            lease_seconds,
            dequeue_batch_size,
            reap_interval_ms,
            verbose_job_logs,
            admin_addr,
            api_token,
            migrate_on_startup,
            max_payload_bytes,
            max_enqueues_per_minute_per_queue,
            maintenance_interval_secs,
            sqlite_incremental_vacuum_n,
        })
    }

    //   Construct a Config

    // Wrap it in Ok

    // Return it to the caller
}

fn env_or_fallback(primary: &str, fallback: &str) -> Option<String> {
    std::env::var(primary)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var(fallback)
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

fn env_bool(key: &str) -> Option<bool> {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn normalize_optional_addr(value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if matches!(v.to_lowercase().as_str(), "0" | "off" | "false" | "none") {
        return None;
    }
    Some(v.to_string())
}
