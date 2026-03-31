use adapter_contract::AdapterRegistry;
use adapter_solana::{register_default_solana_adapter, SolanaAdapterConfig, SolanaQueueAdapter};
use callback_core::{
    CallbackDispatcher, HttpCallbackDispatcher, HttpCallbackDispatcherConfig,
    PostgresQCallbackWorker, PostgresQCallbackWorkerConfig, PostgresQDeliveryStore,
    StdoutCallbackDispatcher, TenantRoutedCallbackDispatcher,
};
use execution_core::integration::postgresq::{
    PostgresQConfig, PostgresQStore, PostgresQWorker, PostgresQWorkerConfig,
};
use execution_core::{
    AdapterId, Authorizer, ExecutionCore, OperatorPrincipal, ReplayPolicy, RetryPolicy,
    SystemClock, TenantId,
};
use exception_intelligence::PostgresExceptionStore;
use recon_core::{PostgresReconStore, ReconWorker, ReconWorkerConfig};
use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct EnvAuthorizer {
    allowed_adapters: HashSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerMode {
    All,
    Dispatch,
    Callback,
    Recon,
}

impl WorkerMode {
    fn from_env() -> anyhow::Result<Self> {
        match env_or("EXECUTION_WORKER_MODE", "all")
            .to_ascii_lowercase()
            .as_str()
        {
            "all" | "both" => Ok(Self::All),
            "dispatch" => Ok(Self::Dispatch),
            "callback" => Ok(Self::Callback),
            "recon" | "reconciliation" => Ok(Self::Recon),
            other => Err(anyhow::anyhow!(
                "EXECUTION_WORKER_MODE must be one of: all, dispatch, callback, recon (got `{other}`)"
            )),
        }
    }

    fn runs_dispatch(self) -> bool {
        matches!(self, Self::All | Self::Dispatch)
    }

    fn runs_callback(self) -> bool {
        matches!(self, Self::All | Self::Callback)
    }

    fn runs_recon(self) -> bool {
        matches!(self, Self::All | Self::Recon)
    }
}

impl Authorizer for EnvAuthorizer {
    fn can_route_adapter(&self, _tenant_id: &TenantId, adapter_id: &AdapterId) -> bool {
        self.allowed_adapters.contains(adapter_id.as_str())
    }

    fn can_replay(&self, principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        matches!(principal.role, execution_core::OperatorRole::Admin)
    }

    fn can_trigger_manual_action(
        &self,
        principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        matches!(principal.role, execution_core::OperatorRole::Admin)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let database_url =
        env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is required"))?;
    let worker_mode = WorkerMode::from_env()?;
    let dispatch_db_max_connections = env_u32(
        "EXECUTION_DISPATCH_DB_MAX_CONNECTIONS",
        env_u32("EXECUTION_DB_MAX_CONNECTIONS", 8),
    );
    let callback_db_max_connections = env_u32(
        "EXECUTION_CALLBACK_DB_MAX_CONNECTIONS",
        env_u32("EXECUTION_DB_MAX_CONNECTIONS", 8),
    );
    let db_acquire_timeout_secs = env_u64("EXECUTION_DB_ACQUIRE_TIMEOUT_SECS", 10);

    let dispatch_queue = env_or("EXECUTION_DISPATCH_QUEUE", "execution.dispatch");
    let callback_queue = env_or("EXECUTION_CALLBACK_QUEUE", "execution.callback");
    let dispatch_notify_channel = PostgresQStore::notify_channel_for_queue(&dispatch_queue);
    let worker_id = env_or("EXECUTION_WORKER_ID", "execution-core-worker");
    let callback_worker_id = env_or("EXECUTION_CALLBACK_WORKER_ID", "execution-callback-worker");
    let lease_seconds = env_i64("EXECUTION_LEASE_SECONDS", 30);
    let default_batch_size = env_i64("EXECUTION_BATCH_SIZE", 32);
    let dispatch_batch_size = env_i64("EXECUTION_DISPATCH_BATCH_SIZE", default_batch_size);
    let callback_batch_size = env_i64("EXECUTION_CALLBACK_BATCH_SIZE", default_batch_size);
    let idle_sleep_ms = env_u64("EXECUTION_IDLE_SLEEP_MS", 50);
    let dispatch_notify_max_wait_ms = env_u64("EXECUTION_DISPATCH_NOTIFY_MAX_WAIT_MS", 500);
    let reap_interval_ms = env_u64("EXECUTION_REAP_INTERVAL_MS", 5_000);
    let queue_job_max_attempts = env_i32("EXECUTION_QUEUE_JOB_MAX_ATTEMPTS", 25);
    let queue_retry_base_delay_secs = env_i64("EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS", 1);
    let queue_retry_max_delay_secs = env_i64("EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS", 300);
    let solana_sync_max_polls = env_usize("SOLANA_SYNC_MAX_POLLS", 8);
    let solana_sync_poll_delay_ms = env_u64("SOLANA_SYNC_POLL_DELAY_MS", 1_200);
    let callback_delivery_url = env::var("EXECUTION_CALLBACK_DELIVERY_URL")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let callback_delivery_token = env::var("EXECUTION_CALLBACK_DELIVERY_TOKEN")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let callback_signing_secret = env::var("EXECUTION_CALLBACK_SIGNING_SECRET")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let callback_signing_key_id = env::var("EXECUTION_CALLBACK_SIGNING_KEY_ID")
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let callback_timeout_ms = env_u64("EXECUTION_CALLBACK_TIMEOUT_MS", 10_000);
    let recon_poll_interval_ms = env_u64("EXECUTION_RECON_POLL_INTERVAL_MS", 500);
    let recon_intake_batch_size = env_u32("EXECUTION_RECON_INTAKE_BATCH_SIZE", 100);
    let recon_reconcile_batch_size = env_u32("EXECUTION_RECON_RECONCILE_BATCH_SIZE", 32);
    let recon_max_retry_attempts = env_u32("EXECUTION_RECON_MAX_RETRY_ATTEMPTS", 3);
    let recon_retry_backoff_ms = env_u64("EXECUTION_RECON_RETRY_BACKOFF_MS", 5_000);
    let callback_allow_private_destinations =
        env_bool("EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS", false);
    let callback_allowed_hosts = env::var("EXECUTION_CALLBACK_ALLOWED_HOSTS")
        .ok()
        .map(|hosts| {
            hosts
                .split(',')
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(|host| host.to_ascii_lowercase())
                .collect::<HashSet<_>>()
        })
        .filter(|hosts| !hosts.is_empty());

    let dispatch_task = if worker_mode.runs_dispatch() {
        let pool = build_pool(&database_url, dispatch_db_max_connections, db_acquire_timeout_secs)
            .await?;
        let store_cfg = PostgresQConfig {
            dispatch_queue: dispatch_queue.clone(),
            callback_queue: callback_queue.clone(),
            queue_job_max_attempts,
            ..PostgresQConfig::default()
        };
        let store = Arc::new(PostgresQStore::new(pool.clone(), store_cfg));
        store.ensure_schema().await?;

        let mut registry = AdapterRegistry::new();
        let solana_adapter = Arc::new(SolanaQueueAdapter::new(
            pool,
            SolanaAdapterConfig {
                sync_max_polls: solana_sync_max_polls,
                sync_poll_delay_ms: solana_sync_poll_delay_ms,
                ..SolanaAdapterConfig::default()
            },
        ));
        register_default_solana_adapter(&mut registry, solana_adapter);

        let allowed_adapters: HashSet<String> =
            env_or("EXECUTION_ALLOWED_ADAPTERS", "adapter_solana")
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect();

        let core = Arc::new(ExecutionCore::new(
            store.clone(),
            Arc::new(registry),
            Arc::new(EnvAuthorizer { allowed_adapters }),
            RetryPolicy::from_env(),
            ReplayPolicy::default(),
            Arc::new(SystemClock),
        ));

        let worker = PostgresQWorker::new(
            core,
            store,
            PostgresQWorkerConfig {
                queue: dispatch_queue,
                worker_id: worker_id.clone(),
                lease_seconds,
                batch_size: dispatch_batch_size,
                idle_sleep_ms,
                notify_max_wait_ms: dispatch_notify_max_wait_ms,
                listener_database_url: Some(database_url.clone()),
                notify_channel: Some(dispatch_notify_channel),
                reap_interval_ms,
                retry_base_delay_secs: queue_retry_base_delay_secs,
                retry_max_delay_secs: queue_retry_max_delay_secs,
            },
        );

        println!("execution_core dispatch worker starting worker_id={worker_id}");
        Some(tokio::spawn(async move { worker.run_forever().await }))
    } else {
        None
    };

    let callback_task = if worker_mode.runs_callback() {
        let pool = build_pool(&database_url, callback_db_max_connections, db_acquire_timeout_secs)
            .await?;
        let callback_fallback_dispatcher: Arc<dyn CallbackDispatcher> =
            if let Some(url) = callback_delivery_url {
                let mut dispatcher_cfg = HttpCallbackDispatcherConfig::new(url);
                dispatcher_cfg.bearer_token = callback_delivery_token;
                dispatcher_cfg.timeout_ms = callback_timeout_ms;
                dispatcher_cfg.signature_secret = callback_signing_secret;
                dispatcher_cfg.signature_key_id = callback_signing_key_id;
                dispatcher_cfg.allowed_hosts = callback_allowed_hosts;
                dispatcher_cfg.allow_private_destinations = callback_allow_private_destinations;
                Arc::new(HttpCallbackDispatcher::new(dispatcher_cfg)?)
            } else {
                Arc::new(StdoutCallbackDispatcher)
            };
        let tenant_destination_store = Arc::new(PostgresQDeliveryStore::new(pool.clone()));
        tenant_destination_store.ensure_schema().await?;
        let callback_dispatcher: Arc<dyn CallbackDispatcher> = Arc::new(
            TenantRoutedCallbackDispatcher::new(
                tenant_destination_store,
                callback_fallback_dispatcher,
            ),
        );
        let callback_worker = PostgresQCallbackWorker::new(
            pool,
            callback_dispatcher,
            PostgresQCallbackWorkerConfig {
                queue: callback_queue,
                worker_id: callback_worker_id.clone(),
                lease_seconds,
                batch_size: callback_batch_size,
                idle_sleep_ms,
                reap_interval_ms,
                ..PostgresQCallbackWorkerConfig::default()
            },
        );
        callback_worker.ensure_schema().await?;

        println!("execution_core callback worker starting worker_id={callback_worker_id}");
        Some(tokio::spawn(async move { callback_worker.run_forever().await }))
    } else {
        None
    };

    let recon_task = if worker_mode.runs_recon() {
        let pool = build_pool(&database_url, callback_db_max_connections, db_acquire_timeout_secs)
            .await?;
        let recon_store = Arc::new(PostgresReconStore::new(pool.clone()));
        let exception_store = Arc::new(PostgresExceptionStore::new(pool));
        let recon_worker = ReconWorker::new(
            recon_store,
            exception_store,
            ReconWorkerConfig {
                poll_interval_ms: recon_poll_interval_ms,
                intake_batch_size: recon_intake_batch_size,
                reconcile_batch_size: recon_reconcile_batch_size,
                max_retry_attempts: recon_max_retry_attempts,
                retry_backoff_ms: recon_retry_backoff_ms,
            },
        );
        recon_worker.ensure_schema().await?;
        println!("execution_core recon worker starting");
        Some(tokio::spawn(async move { recon_worker.run_forever().await }))
    } else {
        None
    };

    match (dispatch_task, callback_task, recon_task) {
        (Some(dispatch_task), Some(callback_task), Some(recon_task)) => {
            let (dispatch_res, callback_res, recon_res) =
                tokio::try_join!(dispatch_task, callback_task, recon_task)?;
            dispatch_res?;
            callback_res?;
            recon_res?;
        }
        (Some(dispatch_task), Some(callback_task), None) => {
            let (dispatch_res, callback_res) = tokio::try_join!(dispatch_task, callback_task)?;
            dispatch_res?;
            callback_res?;
        }
        (Some(dispatch_task), None, Some(recon_task)) => {
            let (dispatch_res, recon_res) = tokio::try_join!(dispatch_task, recon_task)?;
            dispatch_res?;
            recon_res?;
        }
        (None, Some(callback_task), Some(recon_task)) => {
            let (callback_res, recon_res) = tokio::try_join!(callback_task, recon_task)?;
            callback_res?;
            recon_res?;
        }
        (Some(dispatch_task), None, None) => {
            dispatch_task.await??;
        }
        (None, Some(callback_task), None) => {
            callback_task.await??;
        }
        (None, None, Some(recon_task)) => {
            recon_task.await??;
        }
        (None, None, None) => {
            return Err(anyhow::anyhow!(
                "EXECUTION_WORKER_MODE disabled dispatch, callback, and recon workers"
            ));
        }
    }
    Ok(())
}

async fn build_pool(
    database_url: &str,
    max_connections: u32,
    acquire_timeout_secs: u64,
) -> anyhow::Result<sqlx::PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
        .connect(database_url)
        .await?)
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_i32(key: &str, default: i32) -> i32 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .and_then(|v| {
            let trimmed = v.trim().to_ascii_lowercase();
            match trimmed.as_str() {
                "1" | "true" | "yes" | "y" | "on" => Some(true),
                "0" | "false" | "no" | "n" | "off" => Some(false),
                _ => None,
            }
        })
        .unwrap_or(default)
}
