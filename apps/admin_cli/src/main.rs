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
use sqlx::postgres::PgPoolOptions;
use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
struct EnvAuthorizer {
    allowed_adapters: HashSet<String>,
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

    let pool = PgPoolOptions::new()
        .max_connections(env_u32("EXECUTION_DB_MAX_CONNECTIONS", 8))
        .acquire_timeout(Duration::from_secs(env_u64(
            "EXECUTION_DB_ACQUIRE_TIMEOUT_SECS",
            10,
        )))
        .connect(&database_url)
        .await?;

    let dispatch_queue = env_or("EXECUTION_DISPATCH_QUEUE", "execution.dispatch");
    let callback_queue = env_or("EXECUTION_CALLBACK_QUEUE", "execution.callback");
    let worker_id = env_or("EXECUTION_WORKER_ID", "execution-core-worker");
    let callback_worker_id = env_or("EXECUTION_CALLBACK_WORKER_ID", "execution-callback-worker");
    let lease_seconds = env_i64("EXECUTION_LEASE_SECONDS", 30);
    let batch_size = env_i64("EXECUTION_BATCH_SIZE", 32);
    let idle_sleep_ms = env_u64("EXECUTION_IDLE_SLEEP_MS", 250);
    let reap_interval_ms = env_u64("EXECUTION_REAP_INTERVAL_MS", 5_000);
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

    let store_cfg = PostgresQConfig {
        dispatch_queue: dispatch_queue.clone(),
        callback_queue: callback_queue.clone(),
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

    let allowed_adapters: HashSet<String> = env_or("EXECUTION_ALLOWED_ADAPTERS", "adapter_solana")
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let core = Arc::new(ExecutionCore::new(
        store.clone(),
        Arc::new(registry),
        Arc::new(EnvAuthorizer { allowed_adapters }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(SystemClock),
    ));

    let worker = PostgresQWorker::new(
        core,
        store.clone(),
        PostgresQWorkerConfig {
            queue: dispatch_queue,
            worker_id: worker_id.clone(),
            lease_seconds,
            batch_size,
            idle_sleep_ms,
            reap_interval_ms,
        },
    );

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
    let tenant_destination_store = Arc::new(PostgresQDeliveryStore::new(store.pool().clone()));
    tenant_destination_store.ensure_schema().await?;
    let callback_dispatcher: Arc<dyn CallbackDispatcher> = Arc::new(
        TenantRoutedCallbackDispatcher::new(
            tenant_destination_store,
            callback_fallback_dispatcher,
        ),
    );
    let callback_worker = PostgresQCallbackWorker::new(
        store.pool().clone(),
        callback_dispatcher,
        PostgresQCallbackWorkerConfig {
            queue: callback_queue,
            worker_id: callback_worker_id.clone(),
            lease_seconds,
            batch_size,
            idle_sleep_ms,
            reap_interval_ms,
            ..PostgresQCallbackWorkerConfig::default()
        },
    );
    callback_worker.ensure_schema().await?;

    println!("execution_core dispatch worker starting worker_id={worker_id}");
    println!("execution_core callback worker starting worker_id={callback_worker_id}");
    let dispatch_task = tokio::spawn(async move { worker.run_forever().await });
    let callback_task = tokio::spawn(async move { callback_worker.run_forever().await });
    dispatch_task.await??;
    callback_task.await??;
    Ok(())
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
