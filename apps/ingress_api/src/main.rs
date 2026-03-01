use adapter_contract::AdapterRegistry;
use anyhow::Context;
use axum::body::Bytes;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use execution_core::integration::postgresq::{PostgresQConfig, PostgresQStore};
use execution_core::ports::Clock;
use execution_core::{
    AdapterId, AuthContext, Authorizer, CoreError, ExecutionCore, IntentId, IntentKind,
    NormalizedIntent, OperatorPrincipal, ReplayPolicy, RequestId, RetryPolicy, SystemClock,
    TenantId,
};
use hmac::{Hmac, Mac};
use observability::{
    apply_request_context, derive_request_context, init_metrics, init_tracing, record_http_request,
    render_metrics, ObservabilityConfig,
};
use platform_auth::{
    constant_time_eq, env_var_opt, extract_bearer_token, header_opt, parse_kv_map,
    parse_principal_tenant_map as shared_parse_principal_tenant_map,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
struct AppState {
    core: Arc<ExecutionCore>,
    audit_store: Arc<IngressIntakeAuditStore>,
    auth: Arc<IngressAuthConfig>,
    schemas: Arc<IngressSchemaRegistry>,
    clock: Arc<SystemClock>,
}

#[derive(Clone)]
struct IngressIntakeAuditStore {
    pool: PgPool,
}

impl IngressIntakeAuditStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_intake_audits (
                audit_id UUID PRIMARY KEY,
                request_id TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                channel TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                method TEXT NOT NULL,
                principal_id TEXT NULL,
                submitter_kind TEXT NULL,
                auth_scheme TEXT NULL,
                intent_kind TEXT NULL,
                correlation_id TEXT NULL,
                idempotency_key TEXT NULL,
                idempotency_decision TEXT NULL,
                validation_result TEXT NOT NULL,
                rejection_reason TEXT NULL,
                error_status INTEGER NULL,
                error_message TEXT NULL,
                accepted_intent_id TEXT NULL,
                accepted_job_id TEXT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_intake_audits_tenant_created_idx
            ON ingress_api_intake_audits(tenant_id, created_at DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_intake_audits_request_idx
            ON ingress_api_intake_audits(request_id, created_at DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress intake audit schema")?;
        }

        Ok(())
    }

    async fn record(&self, record: &IngressIntakeAuditRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_intake_audits (
                audit_id,
                request_id,
                tenant_id,
                channel,
                endpoint,
                method,
                principal_id,
                submitter_kind,
                auth_scheme,
                intent_kind,
                correlation_id,
                idempotency_key,
                idempotency_decision,
                validation_result,
                rejection_reason,
                error_status,
                error_message,
                accepted_intent_id,
                accepted_job_id,
                details_json,
                created_at_ms
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21
            )
            "#,
        )
        .bind(record.audit_id)
        .bind(&record.request_id)
        .bind(&record.tenant_id)
        .bind(&record.channel)
        .bind(&record.endpoint)
        .bind(&record.method)
        .bind(&record.principal_id)
        .bind(&record.submitter_kind)
        .bind(&record.auth_scheme)
        .bind(&record.intent_kind)
        .bind(&record.correlation_id)
        .bind(&record.idempotency_key)
        .bind(&record.idempotency_decision)
        .bind(&record.validation_result)
        .bind(&record.rejection_reason)
        .bind(record.error_status)
        .bind(&record.error_message)
        .bind(&record.accepted_intent_id)
        .bind(&record.accepted_job_id)
        .bind(&record.details_json)
        .bind(record.created_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to persist ingress intake audit row")?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct IngressIntakeAuditRecord {
    audit_id: Uuid,
    request_id: String,
    tenant_id: String,
    channel: String,
    endpoint: String,
    method: String,
    principal_id: Option<String>,
    submitter_kind: Option<String>,
    auth_scheme: Option<String>,
    intent_kind: Option<String>,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    idempotency_decision: Option<String>,
    validation_result: String,
    rejection_reason: Option<String>,
    error_status: Option<i32>,
    error_message: Option<String>,
    accepted_intent_id: Option<String>,
    accepted_job_id: Option<String>,
    details_json: Value,
    created_at_ms: u64,
}

impl IngressIntakeAuditRecord {
    fn new(
        request_id: String,
        channel: IngressChannel,
        endpoint: &str,
        method: &str,
        created_at_ms: u64,
    ) -> Self {
        Self {
            audit_id: Uuid::new_v4(),
            request_id,
            tenant_id: "__unknown__".to_owned(),
            channel: channel.as_str().to_owned(),
            endpoint: endpoint.to_owned(),
            method: method.to_owned(),
            principal_id: None,
            submitter_kind: None,
            auth_scheme: None,
            intent_kind: None,
            correlation_id: None,
            idempotency_key: None,
            idempotency_decision: None,
            validation_result: "rejected".to_owned(),
            rejection_reason: Some("intake_incomplete".to_owned()),
            error_status: None,
            error_message: None,
            accepted_intent_id: None,
            accepted_job_id: None,
            details_json: json!({}),
            created_at_ms,
        }
    }

    fn set_tenant(&mut self, tenant_id: &str) {
        self.tenant_id = tenant_id.to_owned();
    }

    fn set_submitter(&mut self, submitter: &SubmitterIdentity) {
        self.principal_id = Some(submitter.principal_id.clone());
        self.submitter_kind = Some(submitter.kind.as_str().to_owned());
        self.auth_scheme = Some(submitter.auth_scheme.clone());
    }

    fn mark_accepted(&mut self, response: &SubmitIntentResponse) {
        self.validation_result = "accepted".to_owned();
        self.rejection_reason = None;
        self.error_status = None;
        self.error_message = None;
        self.accepted_intent_id = Some(response.intent_id.clone());
        self.accepted_job_id = Some(response.job_id.clone());
        self.idempotency_decision = Some(idempotency_decision_for_route_rule(
            &response.route_rule,
            self.idempotency_key.is_some(),
        ));
        self.details_json = json!({
            "route_rule": response.route_rule,
            "adapter_id": response.adapter_id,
            "state": response.state,
        });
    }

    fn mark_rejected(&mut self, reason: impl Into<String>, err: &ApiError, details_json: Value) {
        self.validation_result = "rejected".to_owned();
        self.rejection_reason = Some(reason.into());
        self.error_status = Some(err.status.as_u16() as i32);
        self.error_message = Some(err.message.clone());
        if self.idempotency_decision.is_none()
            && err.status == StatusCode::CONFLICT
            && err.message.to_ascii_lowercase().contains("idempotency")
        {
            self.idempotency_decision = Some("conflict".to_owned());
        }
        self.details_json = details_json;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SubmitterKind {
    ApiKeyHolder,
    SignedWebhookSender,
    InternalService,
    WalletBackend,
}

impl SubmitterKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ApiKeyHolder => "api_key_holder",
            Self::SignedWebhookSender => "signed_webhook_sender",
            Self::InternalService => "internal_service",
            Self::WalletBackend => "wallet_backend",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IngressChannel {
    Api,
    Webhook,
}

impl IngressChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Webhook => "webhook",
        }
    }
}

#[derive(Debug, Clone)]
struct SubmitterIdentity {
    principal_id: String,
    kind: SubmitterKind,
    auth_scheme: String,
}

#[derive(Clone)]
struct IngressAuthConfig {
    global_bearer_token: Option<String>,
    tenant_bearer_tokens: HashMap<String, String>,
    global_api_key: Option<String>,
    tenant_api_keys: HashMap<String, String>,
    webhook_signature_secrets: HashMap<String, String>,
    principal_submitter_kinds: HashMap<String, SubmitterKind>,
    principal_tenants: HashMap<String, HashSet<String>>,
    api_allowed_submitters: HashSet<SubmitterKind>,
    webhook_allowed_submitters: HashSet<SubmitterKind>,
    require_principal_id: bool,
    require_submitter_kind: bool,
    require_principal_submitter_binding: bool,
    require_principal_tenant_binding: bool,
    require_api_key_for_api_key_holder: bool,
}

impl IngressAuthConfig {
    fn from_env() -> Result<Self, anyhow::Error> {
        let cfg = Self {
            global_bearer_token: env_var_opt("INGRESS_BEARER_TOKEN"),
            tenant_bearer_tokens: parse_kv_map(env_var_opt("INGRESS_TENANT_TOKENS").as_deref()),
            global_api_key: env_var_opt("INGRESS_API_KEY"),
            tenant_api_keys: parse_kv_map(env_var_opt("INGRESS_TENANT_API_KEYS").as_deref()),
            webhook_signature_secrets: parse_kv_map(
                env_var_opt("INGRESS_WEBHOOK_SIGNATURE_SECRETS").as_deref(),
            ),
            principal_submitter_kinds: parse_principal_submitter_kind_map(
                env_var_opt("INGRESS_PRINCIPAL_SUBMITTER_BINDINGS").as_deref(),
            ),
            principal_tenants: parse_principal_tenant_map(
                env_var_opt("INGRESS_PRINCIPAL_TENANT_BINDINGS").as_deref(),
            ),
            api_allowed_submitters: parse_submitter_set(Some(
                env_or(
                    "INGRESS_API_ALLOWED_SUBMITTERS",
                    "api_key_holder,internal_service,wallet_backend",
                )
                .as_str(),
            ))?,
            webhook_allowed_submitters: parse_submitter_set(Some(
                env_or(
                    "INGRESS_WEBHOOK_ALLOWED_SUBMITTERS",
                    "signed_webhook_sender,internal_service",
                )
                .as_str(),
            ))?,
            require_principal_id: env_bool("INGRESS_REQUIRE_PRINCIPAL_ID", true),
            require_submitter_kind: env_bool("INGRESS_REQUIRE_SUBMITTER_KIND", true),
            require_principal_submitter_binding: env_bool(
                "INGRESS_REQUIRE_PRINCIPAL_SUBMITTER_BINDING",
                true,
            ),
            require_principal_tenant_binding: env_bool(
                "INGRESS_REQUIRE_PRINCIPAL_TENANT_BINDING",
                true,
            ),
            require_api_key_for_api_key_holder: env_bool(
                "INGRESS_REQUIRE_API_KEY_FOR_API_KEY_HOLDER",
                true,
            ),
        };

        Ok(cfg)
    }

    fn authenticate_submitter(
        &self,
        tenant_id: &str,
        headers: &HeaderMap,
        channel: IngressChannel,
    ) -> Result<SubmitterIdentity, ApiError> {
        let principal_id = header_opt(headers, "x-principal-id");
        let principal_id = match principal_id {
            Some(value) => value,
            None if self.require_principal_id => {
                return Err(ApiError::unauthorized("missing x-principal-id"));
            }
            None => "anonymous".to_owned(),
        };

        let kind_from_header = match header_opt(headers, "x-submitter-kind") {
            Some(value) => Some(parse_submitter_kind(&value).ok_or_else(|| {
                ApiError::bad_request(format!("invalid x-submitter-kind `{value}`"))
            })?),
            None => None,
        };
        let mut kind = match kind_from_header {
            Some(kind) => kind,
            None if self.require_submitter_kind => {
                return Err(ApiError::unauthorized("missing x-submitter-kind"));
            }
            None => SubmitterKind::InternalService,
        };

        match self.principal_submitter_kinds.get(principal_id.as_str()) {
            Some(bound_kind) if *bound_kind == kind => {}
            Some(bound_kind) => {
                return Err(ApiError::forbidden(format!(
                    "principal `{}` submitter kind mismatch: expected `{}` got `{}`",
                    principal_id,
                    bound_kind.as_str(),
                    kind.as_str()
                )));
            }
            None if self.require_principal_submitter_binding => {
                return Err(ApiError::forbidden(format!(
                    "principal `{}` is not mapped to any submitter kind",
                    principal_id
                )));
            }
            None if kind_from_header.is_none() => {
                kind = SubmitterKind::InternalService;
            }
            None => {}
        }

        match self.principal_tenants.get(principal_id.as_str()) {
            Some(tenants) if tenants.contains(tenant_id) => {}
            Some(_) => {
                return Err(ApiError::forbidden(format!(
                    "principal `{}` is not allowed for tenant `{tenant_id}`",
                    principal_id
                )));
            }
            None if self.require_principal_tenant_binding => {
                return Err(ApiError::forbidden(format!(
                    "principal `{}` is not bound to any tenant",
                    principal_id
                )));
            }
            None => {}
        }

        let allowed = match channel {
            IngressChannel::Api => &self.api_allowed_submitters,
            IngressChannel::Webhook => &self.webhook_allowed_submitters,
        };
        if !allowed.contains(&kind) {
            return Err(ApiError::forbidden(format!(
                "submitter kind `{}` is not allowed for {} channel",
                kind.as_str(),
                channel.as_str()
            )));
        }

        let auth_scheme = if matches!(kind, SubmitterKind::ApiKeyHolder)
            && self.require_api_key_for_api_key_holder
        {
            self.authenticate_api_key(tenant_id, headers)?;
            "api_key".to_owned()
        } else {
            self.authenticate_bearer(tenant_id, headers)?;
            "bearer".to_owned()
        };

        Ok(SubmitterIdentity {
            principal_id,
            kind,
            auth_scheme,
        })
    }

    fn authenticate_bearer(&self, tenant_id: &str, headers: &HeaderMap) -> Result<(), ApiError> {
        let token = extract_bearer_token(headers)
            .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;

        if let Some(expected) = self.tenant_bearer_tokens.get(tenant_id) {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            return Err(ApiError::unauthorized("invalid bearer token"));
        }

        if let Some(expected) = self.global_bearer_token.as_ref() {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            return Err(ApiError::unauthorized("invalid bearer token"));
        }

        Err(ApiError::unauthorized(
            "no ingress bearer token configured for tenant",
        ))
    }

    fn authenticate_api_key(&self, tenant_id: &str, headers: &HeaderMap) -> Result<(), ApiError> {
        let api_key = header_opt(headers, "x-api-key")
            .ok_or_else(|| ApiError::unauthorized("missing x-api-key"))?;

        if let Some(expected) = self.tenant_api_keys.get(tenant_id) {
            if constant_time_eq(api_key.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            return Err(ApiError::unauthorized("invalid x-api-key"));
        }

        if let Some(expected) = self.global_api_key.as_ref() {
            if constant_time_eq(api_key.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            return Err(ApiError::unauthorized("invalid x-api-key"));
        }

        Err(ApiError::unauthorized(
            "no ingress api-key configured for tenant",
        ))
    }

    fn verify_webhook_signature(
        &self,
        tenant_id: &str,
        body: &[u8],
        headers: &HeaderMap,
        required: bool,
    ) -> Result<(), ApiError> {
        let Some(secret) = self.webhook_signature_secrets.get(tenant_id) else {
            if required {
                return Err(ApiError::unauthorized(
                    "no webhook signing secret configured for tenant",
                ));
            }
            return Ok(());
        };

        let signature_header = headers
            .get("x-webhook-signature")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("missing x-webhook-signature"))?;

        let signature_hex = signature_header
            .strip_prefix("v1=")
            .unwrap_or(signature_header);
        let expected = compute_webhook_signature(secret, body)?;
        if constant_time_eq(signature_hex.as_bytes(), expected.as_bytes()) {
            return Ok(());
        }

        Err(ApiError::unauthorized("invalid webhook signature"))
    }
}

#[derive(Clone)]
struct IngressAuthorizer {
    allowed_adapters: HashSet<String>,
}

impl Authorizer for IngressAuthorizer {
    fn can_route_adapter(&self, _tenant_id: &TenantId, adapter_id: &AdapterId) -> bool {
        self.allowed_adapters.contains(adapter_id.as_str())
    }

    fn can_replay(&self, _principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        false
    }

    fn can_trigger_manual_action(
        &self,
        _principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
struct IngressSchemaRegistry {
    strict: bool,
    by_intent: HashMap<String, BuiltinIntentSchema>,
}

impl IngressSchemaRegistry {
    fn from_env(routes: &[RouteMapping]) -> Result<Self, anyhow::Error> {
        let strict = env_bool("INGRESS_REQUIRE_SCHEMA_FOR_ALL_ROUTES", true);
        let raw = env_or(
            "INGRESS_INTENT_SCHEMAS",
            "solana.transfer.v1=solana.transfer.v1;solana.broadcast.v1=solana.broadcast.v1",
        );
        let mappings = parse_intent_schema_map(&raw)?;
        Self::from_mappings(mappings, strict, routes)
    }

    fn from_mappings(
        mappings: Vec<(String, String)>,
        strict: bool,
        routes: &[RouteMapping],
    ) -> Result<Self, anyhow::Error> {
        let mut by_intent = HashMap::new();

        for (intent_kind, schema_id) in mappings {
            let schema = BuiltinIntentSchema::from_id(&schema_id).ok_or_else(|| {
                anyhow::anyhow!("unsupported schema id `{schema_id}` for intent `{intent_kind}`")
            })?;
            by_intent.insert(intent_kind, schema);
        }

        if strict {
            for route in routes {
                if !by_intent.contains_key(route.intent_kind.as_str()) {
                    anyhow::bail!(
                        "missing schema mapping for routed intent `{}` (set INGRESS_INTENT_SCHEMAS)",
                        route.intent_kind
                    );
                }
            }
        }

        Ok(Self { strict, by_intent })
    }

    fn validate_intent_payload(&self, intent_kind: &str, payload: &Value) -> Result<(), ApiError> {
        let Some(schema) = self.by_intent.get(intent_kind).copied() else {
            if self.strict {
                return Err(ApiError::bad_request(format!(
                    "missing payload schema for intent `{intent_kind}`"
                )));
            }
            return Ok(());
        };

        schema.validate_payload(payload).map_err(|err| {
            ApiError::bad_request(format!(
                "payload does not match intent schema for `{intent_kind}`: {err}"
            ))
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum BuiltinIntentSchema {
    SolanaTransferV1,
    SolanaBroadcastV1,
}

impl BuiltinIntentSchema {
    fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "solana.transfer.v1" => Some(Self::SolanaTransferV1),
            "solana.broadcast.v1" => Some(Self::SolanaBroadcastV1),
            _ => None,
        }
    }

    fn validate_payload(self, payload: &Value) -> Result<(), String> {
        match self {
            Self::SolanaTransferV1 => validate_solana_intent_payload(payload, false),
            Self::SolanaBroadcastV1 => validate_solana_intent_payload(payload, false),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SolanaIntentPayloadSchema {
    #[serde(default)]
    intent_id: Option<String>,
    #[serde(rename = "type", default)]
    intent_type: Option<String>,
    #[serde(default, alias = "to")]
    to_addr: Option<String>,
    amount: i64,
    #[serde(default, alias = "signed_tx_b64", alias = "signed_tx")]
    signed_tx_base64: Option<String>,
    #[serde(default)]
    skip_preflight: Option<bool>,
    #[serde(default)]
    cu_limit: Option<i64>,
    #[serde(default, alias = "cu_price")]
    cu_price_micro_lamports: Option<i64>,
    #[serde(default, alias = "blockhash")]
    blockhash_used: Option<String>,
    #[serde(default)]
    simulation_outcome: Option<String>,
    #[serde(default, alias = "provider")]
    provider_used: Option<String>,
    #[serde(default)]
    rpc_url: Option<String>,
}

fn validate_solana_intent_payload(payload: &Value, require_signed_tx: bool) -> Result<(), String> {
    let parsed: SolanaIntentPayloadSchema =
        serde_json::from_value(payload.clone()).map_err(|err| err.to_string())?;

    ensure_non_empty_optional("intent_id", parsed.intent_id.as_deref())?;
    ensure_non_empty_optional("type", parsed.intent_type.as_deref())?;
    ensure_non_empty_optional("to_addr", parsed.to_addr.as_deref())?;
    ensure_non_empty_optional("signed_tx_base64", parsed.signed_tx_base64.as_deref())?;
    ensure_non_empty_optional("blockhash_used", parsed.blockhash_used.as_deref())?;
    ensure_non_empty_optional("simulation_outcome", parsed.simulation_outcome.as_deref())?;
    ensure_non_empty_optional("provider_used", parsed.provider_used.as_deref())?;
    ensure_non_empty_optional("rpc_url", parsed.rpc_url.as_deref())?;

    let to_addr = parsed
        .to_addr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "payload field `to_addr` (or alias `to`) is required".to_owned())?;
    if to_addr.is_empty() {
        return Err("payload field `to_addr` must not be empty".to_owned());
    }

    if parsed.amount <= 0 {
        return Err("payload field `amount` must be a positive integer".to_owned());
    }

    if let Some(cu_limit) = parsed.cu_limit {
        if cu_limit <= 0 {
            return Err("payload field `cu_limit` must be positive when provided".to_owned());
        }
        if i32::try_from(cu_limit).is_err() {
            return Err("payload field `cu_limit` must fit into 32-bit integer".to_owned());
        }
    }

    if let Some(cu_price_micro_lamports) = parsed.cu_price_micro_lamports {
        if cu_price_micro_lamports < 0 {
            return Err(
                "payload field `cu_price_micro_lamports` must be >= 0 when provided".to_owned(),
            );
        }
    }

    if require_signed_tx && parsed.signed_tx_base64.is_none() {
        return Err(
            "payload field `signed_tx_base64` is required for this intent schema".to_owned(),
        );
    }

    let _ = parsed.skip_preflight;
    Ok(())
}

fn ensure_non_empty_optional(field: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(format!("payload field `{field}` must not be empty"));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SubmitIntentRequest {
    intent_kind: String,
    payload: Value,
    metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Serialize)]
struct SubmitIntentResponse {
    ok: bool,
    tenant_id: String,
    intent_id: String,
    job_id: String,
    adapter_id: String,
    state: String,
    route_rule: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let observability = Arc::new(ObservabilityConfig::from_env("ingress_api"));
    init_tracing(observability.as_ref()).context("failed to initialize observability")?;
    let _metrics_handle = init_metrics().context("failed to initialize metrics recorder")?;

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is required for ingress_api")?;
    let bind_addr = env_or("INGRESS_API_BIND", "0.0.0.0:8081");
    let max_connections = env_u32("INGRESS_DB_MAX_CONNECTIONS", 8);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("failed to connect to postgres")?;
    let audit_store = Arc::new(IngressIntakeAuditStore::new(pool.clone()));
    audit_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress intake audit schema")?;

    let store = Arc::new(PostgresQStore::new(
        pool,
        PostgresQConfig {
            dispatch_queue: env_or("EXECUTION_DISPATCH_QUEUE", "execution.dispatch"),
            callback_queue: env_or("EXECUTION_CALLBACK_QUEUE", "execution.callback"),
            ..PostgresQConfig::default()
        },
    ));
    store
        .ensure_schema()
        .await
        .context("failed to ensure execution core schema")?;

    let routes = parse_route_map(env_or(
        "INGRESS_INTENT_ROUTES",
        "solana.transfer.v1=adapter_solana;solana.broadcast.v1=adapter_solana",
    ))?;
    let schemas = Arc::new(IngressSchemaRegistry::from_env(&routes)?);
    let mut registry = AdapterRegistry::new();
    let mut allowed_adapters = HashSet::new();
    for route in routes {
        let adapter_id = AdapterId::from(route.adapter_id.clone());
        registry.register_route(
            route.intent_kind.clone(),
            adapter_id.clone(),
            format!("ingress_route_map:{}", route.intent_kind),
        );
        allowed_adapters.insert(adapter_id.to_string());
    }

    let core = Arc::new(ExecutionCore::new(
        store,
        Arc::new(registry),
        Arc::new(IngressAuthorizer { allowed_adapters }),
        RetryPolicy::default(),
        ReplayPolicy::default(),
        Arc::new(SystemClock),
    ));

    let state = AppState {
        core,
        audit_store,
        auth: Arc::new(IngressAuthConfig::from_env()?),
        schemas,
        clock: Arc::new(SystemClock),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/requests", post(submit_request))
        .route("/webhooks/:source", post(submit_webhook))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            observability.clone(),
            observability_middleware,
        ));

    let addr: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("invalid INGRESS_API_BIND `{bind_addr}`"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(bind = %addr, "ingress_api listening");
    axum::serve(listener, app).await?;
    Ok(())
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

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn metrics() -> Response {
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

async fn submit_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubmitIntentRequest>,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Api,
        "/api/requests",
        "POST",
        state.clock.now_ms(),
    );

    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "missing_tenant_header",
                json!({ "stage": "tenant_header" }),
            )
            .await
        }
    };
    audit.set_tenant(&tenant_id);

    let submitter =
        match state
            .auth
            .authenticate_submitter(&tenant_id, &headers, IngressChannel::Api)
        {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "submitter_auth_failed",
                    json!({ "stage": "auth" }),
                )
                .await
            }
        };
    audit.set_submitter(&submitter);

    let correlation_id = header_opt(&headers, "x-correlation-id");
    let idempotency_key = header_opt(&headers, "x-idempotency-key");
    audit.correlation_id = correlation_id.clone();
    audit.idempotency_key = idempotency_key.clone();

    let intent_kind = payload.intent_kind.trim().to_owned();
    if intent_kind.is_empty() {
        return reject_with_audit(
            &state,
            &mut audit,
            ApiError::bad_request("intent_kind is required"),
            "missing_intent_kind",
            json!({ "stage": "validate_request" }),
        )
        .await;
    }
    audit.intent_kind = Some(intent_kind.clone());

    let mut metadata = payload.metadata.unwrap_or_default();
    metadata.insert("ingress.channel".to_owned(), "api".to_owned());
    metadata.insert("request_id".to_owned(), request_id.to_string());
    if let Some(value) = correlation_id.as_ref() {
        metadata.insert("correlation_id".to_owned(), value.clone());
    }
    if let Some(value) = idempotency_key.as_ref() {
        metadata.insert("idempotency_key".to_owned(), value.clone());
    }
    metadata.insert(
        "submitter.principal_id".to_owned(),
        submitter.principal_id.clone(),
    );
    metadata.insert(
        "submitter.kind".to_owned(),
        submitter.kind.as_str().to_owned(),
    );

    let result = submit_normalized_intent(
        &state,
        tenant_id,
        IntentKind::new(intent_kind),
        payload.payload,
        request_id.clone(),
        correlation_id,
        idempotency_key,
        Some(AuthContext {
            principal_id: Some(submitter.principal_id),
            submitter_kind: Some(submitter.kind.as_str().to_owned()),
            auth_scheme: Some(submitter.auth_scheme),
            channel: Some("api".to_owned()),
        }),
        metadata,
    )
    .await;

    match result {
        Ok(Json(response)) => {
            audit.mark_accepted(&response);
            persist_intake_audit(&state, &audit).await;
            Ok(Json(response))
        }
        Err(err) => {
            let reason = classify_ingress_rejection_reason(&err);
            reject_with_audit(
                &state,
                &mut audit,
                err,
                reason,
                json!({ "stage": "submit_intent" }),
            )
            .await
        }
    }
}

async fn submit_webhook(
    State(state): State<AppState>,
    Path(source): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Webhook,
        "/webhooks/:source",
        "POST",
        state.clock.now_ms(),
    );

    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "missing_tenant_header",
                json!({ "stage": "tenant_header" }),
            )
            .await
        }
    };
    audit.set_tenant(&tenant_id);

    let submitter =
        match state
            .auth
            .authenticate_submitter(&tenant_id, &headers, IngressChannel::Webhook)
        {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "submitter_auth_failed",
                    json!({ "stage": "auth" }),
                )
                .await
            }
        };
    audit.set_submitter(&submitter);

    let correlation_id = header_opt(&headers, "x-correlation-id");
    let idempotency_key = header_opt(&headers, "x-idempotency-key");
    audit.correlation_id = correlation_id.clone();
    audit.idempotency_key = idempotency_key.clone();

    if let Err(err) = state.auth.verify_webhook_signature(
        &tenant_id,
        &body,
        &headers,
        matches!(submitter.kind, SubmitterKind::SignedWebhookSender),
    ) {
        return reject_with_audit(
            &state,
            &mut audit,
            err,
            "webhook_signature_invalid",
            json!({ "stage": "signature_verification" }),
        )
        .await;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::bad_request(format!("invalid webhook json payload: {err}")),
                "invalid_webhook_json",
                json!({ "stage": "parse_payload" }),
            )
            .await
        }
    };

    let intent_kind = header_opt(&headers, "x-intent-kind")
        .filter(|kind| !kind.trim().is_empty())
        .unwrap_or_else(|| format!("webhook.{}.v1", sanitize_source(&source)));
    audit.intent_kind = Some(intent_kind.clone());

    let mut metadata = BTreeMap::new();
    metadata.insert("ingress.channel".to_owned(), "webhook".to_owned());
    metadata.insert("webhook.source".to_owned(), sanitize_source(&source));
    metadata.insert("request_id".to_owned(), request_id.to_string());
    if let Some(value) = correlation_id.as_ref() {
        metadata.insert("correlation_id".to_owned(), value.clone());
    }
    if let Some(value) = idempotency_key.as_ref() {
        metadata.insert("idempotency_key".to_owned(), value.clone());
    }
    if let Some(value) = header_opt(&headers, "x-webhook-id") {
        metadata.insert("webhook_id".to_owned(), value);
    }
    metadata.insert(
        "submitter.principal_id".to_owned(),
        submitter.principal_id.clone(),
    );
    metadata.insert(
        "submitter.kind".to_owned(),
        submitter.kind.as_str().to_owned(),
    );

    let result = submit_normalized_intent(
        &state,
        tenant_id,
        IntentKind::new(intent_kind),
        payload,
        request_id.clone(),
        correlation_id,
        idempotency_key,
        Some(AuthContext {
            principal_id: Some(submitter.principal_id),
            submitter_kind: Some(submitter.kind.as_str().to_owned()),
            auth_scheme: Some(submitter.auth_scheme),
            channel: Some("webhook".to_owned()),
        }),
        metadata,
    )
    .await;

    match result {
        Ok(Json(response)) => {
            audit.mark_accepted(&response);
            persist_intake_audit(&state, &audit).await;
            Ok(Json(response))
        }
        Err(err) => {
            let reason = classify_ingress_rejection_reason(&err);
            reject_with_audit(
                &state,
                &mut audit,
                err,
                reason,
                json!({ "stage": "submit_intent", "webhook_source": sanitize_source(&source) }),
            )
            .await
        }
    }
}

async fn submit_normalized_intent(
    state: &AppState,
    tenant_id: String,
    kind: IntentKind,
    payload: Value,
    request_id: RequestId,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    auth_context: Option<AuthContext>,
    metadata: BTreeMap<String, String>,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    state
        .schemas
        .validate_intent_payload(kind.as_str(), &payload)?;

    let intent = NormalizedIntent {
        request_id: Some(request_id),
        intent_id: IntentId::new(),
        tenant_id: TenantId::from(tenant_id.clone()),
        kind,
        payload,
        correlation_id,
        idempotency_key,
        auth_context,
        metadata,
        received_at_ms: state.clock.now_ms(),
    };

    let submitted = state
        .core
        .submit_intent(intent)
        .await
        .map_err(map_core_error)?;

    Ok(Json(SubmitIntentResponse {
        ok: true,
        tenant_id,
        intent_id: submitted.job.intent_id.to_string(),
        job_id: submitted.job.job_id.to_string(),
        adapter_id: submitted.job.adapter_id.to_string(),
        state: format!("{:?}", submitted.job.state),
        route_rule: submitted.route_rule,
    }))
}

#[derive(Debug, Clone)]
struct RouteMapping {
    intent_kind: String,
    adapter_id: String,
}

fn parse_route_map(raw: String) -> Result<Vec<RouteMapping>, anyhow::Error> {
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (kind, adapter) = trimmed
            .split_once('=')
            .with_context(|| format!("invalid route entry `{trimmed}` (expected kind=adapter)"))?;
        let kind = kind.trim().to_owned();
        let adapter = adapter.trim().to_owned();
        if kind.is_empty() || adapter.is_empty() {
            anyhow::bail!("invalid route entry `{trimmed}`");
        }
        out.push(RouteMapping {
            intent_kind: kind,
            adapter_id: adapter,
        });
    }
    if out.is_empty() {
        anyhow::bail!("INGRESS_INTENT_ROUTES is empty");
    }
    Ok(out)
}

fn parse_intent_schema_map(raw: &str) -> Result<Vec<(String, String)>, anyhow::Error> {
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (intent_kind, schema_id) = trimmed.split_once('=').with_context(|| {
            format!("invalid schema entry `{trimmed}` (expected intent_kind=schema_id)")
        })?;
        let intent_kind = intent_kind.trim().to_owned();
        let schema_id = schema_id.trim().to_owned();
        if intent_kind.is_empty() || schema_id.is_empty() {
            anyhow::bail!("invalid schema entry `{trimmed}`");
        }
        out.push((intent_kind, schema_id));
    }
    Ok(out)
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    header_opt(headers, "x-tenant-id")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing x-tenant-id"))
}

fn compute_webhook_signature(secret: &str, body: &[u8]) -> Result<String, ApiError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|err| ApiError::internal(format!("invalid webhook signing secret: {err}")))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn sanitize_source(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = normalized.trim_matches('_').to_owned();
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

async fn persist_intake_audit(state: &AppState, record: &IngressIntakeAuditRecord) {
    if let Err(err) = state.audit_store.record(record).await {
        warn!(error = %err, "failed to persist ingress intake audit row");
    }
}

async fn reject_with_audit<T>(
    state: &AppState,
    record: &mut IngressIntakeAuditRecord,
    err: ApiError,
    reason: &str,
    details_json: Value,
) -> Result<T, ApiError> {
    record.mark_rejected(reason, &err, details_json);
    persist_intake_audit(state, record).await;
    Err(err)
}

fn classify_ingress_rejection_reason(err: &ApiError) -> &'static str {
    match err.status {
        StatusCode::BAD_REQUEST => {
            let lower = err.message.to_ascii_lowercase();
            if lower.contains("payload does not match intent schema")
                || lower.contains("missing payload schema")
            {
                "schema_validation_failed"
            } else if lower.contains("unsupported intent") {
                "unsupported_intent"
            } else {
                "bad_request"
            }
        }
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::CONFLICT => {
            if err.message.to_ascii_lowercase().contains("idempotency") {
                "idempotency_conflict"
            } else {
                "conflict"
            }
        }
        StatusCode::SERVICE_UNAVAILABLE => "core_unavailable",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_error",
        _ => "ingress_rejected",
    }
}

fn idempotency_decision_for_route_rule(route_rule: &str, has_idempotency_key: bool) -> String {
    if !has_idempotency_key {
        return "not_provided".to_owned();
    }

    if route_rule.starts_with("idempotency_reuse:") {
        "reused_existing".to_owned()
    } else {
        "accepted_new".to_owned()
    }
}

fn map_core_error(err: CoreError) -> ApiError {
    match err {
        CoreError::UnsupportedIntent(kind) => {
            ApiError::bad_request(format!("unsupported intent `{}`", kind.as_str()))
        }
        CoreError::AdapterRoutingDenied {
            tenant_id,
            adapter_id,
        } => ApiError::forbidden(format!(
            "adapter `{adapter_id}` is not allowed for tenant `{tenant_id}`"
        )),
        CoreError::IllegalTransition { from, to } => {
            ApiError::conflict(format!("illegal transition {from:?} -> {to:?}"))
        }
        CoreError::JobNotFound(job_id) => ApiError::not_found(format!("job `{job_id}` not found")),
        CoreError::IntentNotFound(intent_id) => {
            ApiError::not_found(format!("intent `{intent_id}` not found"))
        }
        CoreError::TenantMismatch {
            job_id,
            expected,
            actual,
        } => ApiError::forbidden(format!(
            "tenant mismatch on job `{job_id}` (expected `{expected}`, got `{actual}`)"
        )),
        CoreError::UnauthorizedReplay { principal_id } => {
            ApiError::forbidden(format!("unauthorized replay by `{principal_id}`"))
        }
        CoreError::ReplayDenied { reason } => ApiError::conflict(reason),
        CoreError::IdempotencyConflict { key, reason } => {
            ApiError::conflict(format!("idempotency conflict for key `{key}`: {reason}"))
        }
        CoreError::UnauthorizedManualAction { principal_id } => {
            ApiError::forbidden(format!("unauthorized manual action by `{principal_id}`"))
        }
        CoreError::Store(err) => ApiError::service_unavailable(format!("store error: {err}")),
        CoreError::Routing(err) => ApiError::service_unavailable(format!("routing error: {err}")),
        CoreError::AdapterExecution(err) => {
            ApiError::service_unavailable(format!("adapter execution error: {err}"))
        }
        CoreError::Callback(err) => {
            ApiError::service_unavailable(format!("callback backend error: {err}"))
        }
    }
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    ok: bool,
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                ok: false,
                error: self.message,
            }),
        )
            .into_response()
    }
}

fn parse_submitter_kind(value: &str) -> Option<SubmitterKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "api_key_holder" | "api-key-holder" => Some(SubmitterKind::ApiKeyHolder),
        "signed_webhook_sender" | "signed-webhook-sender" => {
            Some(SubmitterKind::SignedWebhookSender)
        }
        "internal_service" | "internal-service" => Some(SubmitterKind::InternalService),
        "wallet_backend" | "wallet-backend" => Some(SubmitterKind::WalletBackend),
        _ => None,
    }
}

fn parse_submitter_set(raw: Option<&str>) -> Result<HashSet<SubmitterKind>, anyhow::Error> {
    let mut out = HashSet::new();
    let Some(raw) = raw else {
        return Ok(out);
    };

    for part in raw.split(|ch| ch == ';' || ch == ',' || ch == '|') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = parse_submitter_kind(trimmed).ok_or_else(|| {
            anyhow::anyhow!("unsupported submitter kind `{trimmed}` in ingress config")
        })?;
        out.insert(kind);
    }

    Ok(out)
}

fn parse_principal_submitter_kind_map(raw: Option<&str>) -> HashMap<String, SubmitterKind> {
    let mut out = HashMap::new();
    for (principal, kind_raw) in parse_kv_map(raw) {
        if let Some(kind) = parse_submitter_kind(&kind_raw) {
            out.insert(principal, kind);
        }
    }
    out
}

fn parse_principal_tenant_map(raw: Option<&str>) -> HashMap<String, HashSet<String>> {
    shared_parse_principal_tenant_map(raw)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_intent_schema_map, parse_principal_tenant_map, parse_submitter_set,
        validate_solana_intent_payload, IngressSchemaRegistry, RouteMapping, SubmitterKind,
    };
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn solana_schema_accepts_valid_payload() {
        let payload = json!({
            "intent_id": "intent_123",
            "type": "transfer",
            "to_addr": "11111111111111111111111111111111",
            "amount": 1,
            "skip_preflight": false,
            "cu_limit": 200000,
            "cu_price_micro_lamports": 0
        });
        let result = validate_solana_intent_payload(&payload, false);
        assert!(result.is_ok(), "expected valid payload: {result:?}");
    }

    #[test]
    fn solana_schema_rejects_unknown_field() {
        let payload = json!({
            "to_addr": "11111111111111111111111111111111",
            "amount": 1,
            "extra": true
        });
        let result = validate_solana_intent_payload(&payload, false);
        let err = result.expect_err("expected schema validation error");
        assert!(
            err.contains("unknown field"),
            "expected unknown field error, got: {err}"
        );
    }

    #[test]
    fn solana_schema_rejects_missing_to_addr() {
        let payload = json!({
            "amount": 1
        });
        let result = validate_solana_intent_payload(&payload, false);
        let err = result.expect_err("expected missing to_addr error");
        assert!(
            err.contains("to_addr"),
            "expected to_addr error message, got: {err}"
        );
    }

    #[test]
    fn strict_registry_requires_route_schema_mapping() {
        let routes = vec![RouteMapping {
            intent_kind: "solana.transfer.v1".to_owned(),
            adapter_id: "adapter_solana".to_owned(),
        }];
        let mappings = vec![(
            "solana.broadcast.v1".to_owned(),
            "solana.broadcast.v1".to_owned(),
        )];
        let result = IngressSchemaRegistry::from_mappings(mappings, true, &routes);
        let err = result.expect_err("expected strict registry error");
        assert!(
            err.to_string().contains("missing schema mapping"),
            "unexpected strict mapping error: {err}"
        );
    }

    #[test]
    fn parse_schema_map_parses_entries() {
        let parsed = parse_intent_schema_map(
            "solana.transfer.v1=solana.transfer.v1;solana.broadcast.v1=solana.broadcast.v1",
        )
        .expect("expected schema map parsing to succeed");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_submitter_set_accepts_supported_kinds() {
        let parsed = parse_submitter_set(Some("api_key_holder,internal_service,wallet_backend"))
            .expect("expected submitter set parsing to succeed");
        let expected: HashSet<SubmitterKind> = HashSet::from([
            SubmitterKind::ApiKeyHolder,
            SubmitterKind::InternalService,
            SubmitterKind::WalletBackend,
        ]);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_submitter_set_rejects_unknown_kind() {
        let err = parse_submitter_set(Some("api_key_holder,unknown"))
            .expect_err("expected unknown submitter kind to fail");
        assert!(
            err.to_string().contains("unsupported submitter kind"),
            "unexpected parse error: {err}"
        );
    }

    #[test]
    fn parse_principal_tenant_map_supports_multi_tenant_binding() {
        let parsed =
            parse_principal_tenant_map(Some("svc-a=tenant_a|tenant_b;svc-b:tenant_c,tenant_d"));
        assert_eq!(parsed.len(), 2);
        assert!(
            parsed
                .get("svc-a")
                .map(|set| set.contains("tenant_a") && set.contains("tenant_b"))
                .unwrap_or(false),
            "expected svc-a tenant bindings"
        );
        assert!(
            parsed
                .get("svc-b")
                .map(|set| set.contains("tenant_c") && set.contains("tenant_d"))
                .unwrap_or(false),
            "expected svc-b tenant bindings"
        );
    }
}
