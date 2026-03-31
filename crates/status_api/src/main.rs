use adapter_contract::AdapterRegistry;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use execution_core::integration::postgresq::{PostgresQConfig, PostgresQStore};
use execution_core::{
    AdapterId, Authorizer as CoreAuthorizer, ExecutionCore, OperatorPrincipal, OperatorRole,
    ReplayPolicy, RetryPolicy, SystemClock, TenantId,
};
use observability::{
    apply_request_context, derive_request_context, init_metrics, init_tracing, record_http_request,
    render_metrics, ObservabilityConfig,
};
use exception_intelligence::PostgresExceptionStore;
use recon_core::PostgresReconStore;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use status_api::{
    router, PostgresStatusStore, ReplayGateway, RoleBasedStatusAuthorizer, StatusApiState,
    StatusAuthConfig, StatusAuthorizer,
};
use std::collections::HashSet;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let observability = Arc::new(ObservabilityConfig::from_env("status_api"));
    init_tracing(observability.as_ref())?;
    let _metrics_handle = init_metrics()?;

    let database_url =
        env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required for status_api")?;
    let bind_addr = env_or("STATUS_API_BIND", "0.0.0.0:8082");
    let max_connections = env_u32("STATUS_API_DB_MAX_CONNECTIONS", 8);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await?;

    let store = Arc::new(PostgresStatusStore::new(pool.clone()));
    store.ensure_schema().await?;
    let recon_store = Arc::new(PostgresReconStore::new(pool.clone()));
    recon_store.ensure_schema().await?;
    let exception_store = Arc::new(PostgresExceptionStore::new(pool));
    exception_store.ensure_schema().await?;

    let authorizer: Arc<dyn StatusAuthorizer> = Arc::new(RoleBasedStatusAuthorizer);
    let replay_gateway = build_replay_gateway().await?;
    let auth = Arc::new(StatusAuthConfig::from_env());
    let state = StatusApiState::new(
        store,
        recon_store,
        exception_store,
        authorizer,
        replay_gateway,
        auth,
    );
    let app = router(state)
        .route("/metrics", get(metrics_endpoint))
        .route("/status/metrics", get(metrics_endpoint))
        .layer(middleware::from_fn_with_state(
            observability.clone(),
            observability_middleware,
        ));

    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|err| format!("invalid STATUS_API_BIND `{bind_addr}`: {err}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(bind = %addr, "status_api listening");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn metrics_endpoint() -> Response {
    match render_metrics() {
        Some(payload) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            payload,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "ok": false,
                "error": "metrics recorder is not initialized",
            })),
        )
            .into_response(),
    }
}

async fn observability_middleware(
    State(observability): State<Arc<ObservabilityConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let start = Instant::now();

    let ctx = derive_request_context(request.headers(), observability.as_ref());
    if let Err(err) = apply_request_context(request.headers_mut(), observability.as_ref(), &ctx) {
        warn!(error = %err, "failed to apply observability request headers");
    }

    let mut response = next.run(request).await;
    if let Err(err) = apply_request_context(response.headers_mut(), observability.as_ref(), &ctx) {
        warn!(error = %err, "failed to apply observability response headers");
    }

    let status = response.status().as_u16();
    record_http_request(
        observability.as_ref(),
        &method,
        &path,
        status,
        start.elapsed(),
    );

    response
}

#[derive(Clone)]
struct ReplayCoreAuthorizer {
    allowed_adapters: HashSet<String>,
}

impl CoreAuthorizer for ReplayCoreAuthorizer {
    fn can_route_adapter(&self, _tenant_id: &TenantId, adapter_id: &AdapterId) -> bool {
        self.allowed_adapters.contains(adapter_id.as_str())
    }

    fn can_replay(&self, principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        matches!(principal.role, OperatorRole::Admin)
    }

    fn can_trigger_manual_action(
        &self,
        principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        matches!(principal.role, OperatorRole::Admin)
    }
}

async fn build_replay_gateway() -> Result<Option<Arc<dyn ReplayGateway>>, Box<dyn std::error::Error>>
{
    if !env_bool("STATUS_API_ENABLE_REPLAY", false) {
        return Ok(None);
    }

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required when STATUS_API_ENABLE_REPLAY=true")?;
    let max_connections = env_u32("STATUS_API_REPLAY_DB_MAX_CONNECTIONS", 4);
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await?;

    let store = Arc::new(PostgresQStore::new(
        pool,
        PostgresQConfig {
            dispatch_queue: env_or("EXECUTION_DISPATCH_QUEUE", "execution.dispatch"),
            callback_queue: env_or("EXECUTION_CALLBACK_QUEUE", "execution.callback"),
            ..PostgresQConfig::default()
        },
    ));
    store.ensure_schema().await?;

    let routes = parse_route_map(env_or(
        "STATUS_API_INTENT_ROUTES",
        "solana.transfer.v1=adapter_solana;solana.broadcast.v1=adapter_solana",
    ));
    let mut registry = AdapterRegistry::new();
    let mut allowed_adapters = HashSet::new();
    for (intent_kind, adapter_id) in routes {
        let adapter_id = AdapterId::from(adapter_id);
        registry.register_route(
            intent_kind.clone(),
            adapter_id.clone(),
            format!("status_api_route_map:{intent_kind}"),
        );
        allowed_adapters.insert(adapter_id.to_string());
    }

    let allowed_override = env_or("STATUS_API_ALLOWED_ADAPTERS", "");
    for value in allowed_override.split(',') {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            allowed_adapters.insert(trimmed.to_owned());
        }
    }

    let core = Arc::new(ExecutionCore::new(
        store,
        Arc::new(registry),
        Arc::new(ReplayCoreAuthorizer { allowed_adapters }),
        RetryPolicy::from_env(),
        ReplayPolicy::default(),
        Arc::new(SystemClock),
    ));

    Ok(Some(core as Arc<dyn ReplayGateway>))
}

fn parse_route_map(raw: String) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((kind, adapter_id)) = trimmed.split_once('=') {
            let kind = kind.trim();
            let adapter_id = adapter_id.trim();
            if !kind.is_empty() && !adapter_id.is_empty() {
                out.push((kind.to_owned(), adapter_id.to_owned()));
            }
        }
    }
    out
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}
