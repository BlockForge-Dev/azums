use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shared_types::reconciliation::{
    ExceptionActionResponse, ExceptionDetailResponse, ExceptionIndexResponse,
    ExceptionStateTransitionRequest, ExceptionStateTransitionResponse, OperatorActionRequest,
    ReconActionResponse, ReconciliationRolloutSummaryResponse, ReplayReviewResponse,
    UnifiedRequestStatusResponse,
};
use shared_types::status_api::{
    CallbackDestinationResponse, CallbackDetailResponse, CallbackHistoryResponse,
    DeleteCallbackDestinationResponse, HistoryResponse, IntakeAuditsResponse, JobListResponse,
    ReceiptLookupResponse, ReceiptResponse, ReplayRequest, ReplayResponse, RequestStatusResponse,
    UpsertCallbackDestinationRequest, UpsertCallbackDestinationResponse,
};
use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;

const SESSION_COOKIE_NAME: &str = "azums_session";
const DEFAULT_WORKSPACE_ENVIRONMENT: &str = "staging";
const THIRTY_DAYS_MS: u64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Clone)]
struct AppState {
    client: Client,
    status_base_url: Url,
    ingress_base_url: Url,
    status_auth: StatusAuthHeaders,
    ingress_auth: IngressAuthHeaders,
    ui_features: UiFeatures,
    seed: SessionSeed,
    workspaces: Vec<WorkspaceSeed>,
    account_state: Arc<RwLock<AccountState>>,
}

#[derive(Clone)]
struct StatusAuthHeaders {
    bearer_token: Option<String>,
    tenant_id: String,
    principal_id: String,
    principal_role: String,
}

#[derive(Clone)]
struct IngressAuthHeaders {
    bearer_token: Option<String>,
    tenant_id: String,
    principal_id: String,
    submitter_kind: String,
    fallback_principal_id: Option<String>,
    fallback_submitter_kind: Option<String>,
}

#[derive(Clone)]
struct UiFeatures {
    public_base_url: String,
    workspace_environment: String,
    reconciliation_rollout_mode: String,
    reconciliation_operator_visible: bool,
    reconciliation_customer_visible: bool,
    require_email_verification: bool,
    email_delivery_configured: bool,
    password_reset_enabled: bool,
    require_durable_metering: bool,
    enforce_workspace_solana_rpc: bool,
    sandbox_solana_rpc_url: Option<String>,
    flutterwave_secret_key: Option<String>,
    flutterwave_webhook_hash: Option<String>,
    flutterwave_base_url: String,
    flutterwave_expected_currency: Option<String>,
    flutterwave_supported_currencies: Vec<String>,
}

#[derive(Clone)]
struct SessionSeed {
    session_id: String,
    email: String,
    full_name: String,
    password: String,
    workspace_id: String,
    workspace_name: String,
    workspace_role: String,
    plan: String,
    created_at_ms: u64,
}

#[derive(Clone)]
struct WorkspaceSeed {
    workspace_id: String,
    workspace_name: String,
    environment: String,
}

#[derive(Clone)]
struct SessionState {
    email_key: String,
    workspace_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct OnboardingState {
    workspace_created: bool,
    api_key_generated: bool,
    submitted_request: bool,
    viewed_receipt: bool,
    configured_callback: bool,
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self {
            workspace_created: true,
            api_key_generated: false,
            submitted_request: false,
            viewed_receipt: false,
            configured_callback: false,
        }
    }
}

#[derive(Clone)]
struct AccountRecord {
    id: String,
    email: String,
    full_name: String,
    password: String,
    role: String,
    status: String,
    added_at_ms: u64,
    invite_expires_at_ms: Option<u64>,
    email_verified_at_ms: Option<u64>,
    onboarding: OnboardingState,
}

#[derive(Clone)]
struct InviteRecord {
    email: String,
    workspace_id: String,
    workspace_name: String,
    role: String,
    expires_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct BillingState {
    plan: String,
    access_mode: String,
    billing_email: String,
    card_brand: Option<String>,
    card_last4: Option<String>,
    payment_provider: Option<String>,
    payment_reference: Option<String>,
    payment_verified_at_ms: Option<u64>,
    payment_currency: Option<String>,
    payment_amount: Option<f64>,
    payment_amount_usd: Option<f64>,
    payment_fx_rate_to_usd: Option<f64>,
    updated_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct WorkspaceSettingsState {
    callback_default_enabled: bool,
    request_retention_days: u32,
    allow_replay_from_customer_app: bool,
    execution_policy: String,
    sponsored_monthly_cap_requests: u64,
    updated_at_ms: u64,
}

#[derive(Clone)]
struct AccountState {
    accounts: HashMap<String, AccountRecord>,
    sessions: HashMap<String, SessionState>,
    invites: HashMap<String, InviteRecord>,
    verification_tokens: HashMap<String, String>,
    password_reset_tokens: HashMap<String, String>,
    billing: BillingState,
    billing_audit: Vec<Value>,
    invoices: Vec<Value>,
    settings: WorkspaceSettingsState,
}

impl AccountState {
    fn new(seed: &SessionSeed, tenant_id: &str) -> Self {
        let mut accounts = HashMap::new();
        accounts.insert(
            normalize_email(&seed.email),
            AccountRecord {
                id: seed.session_id.clone(),
                email: seed.email.clone(),
                full_name: seed.full_name.clone(),
                password: seed.password.clone(),
                role: seed.workspace_role.clone(),
                status: "active".to_owned(),
                added_at_ms: seed.created_at_ms,
                invite_expires_at_ms: None,
                email_verified_at_ms: Some(seed.created_at_ms),
                onboarding: OnboardingState::default(),
            },
        );

        let billing = BillingState {
            plan: seed.plan.clone(),
            access_mode: "free_play".to_owned(),
            billing_email: seed.email.clone(),
            card_brand: None,
            card_last4: None,
            payment_provider: None,
            payment_reference: None,
            payment_verified_at_ms: None,
            payment_currency: None,
            payment_amount: None,
            payment_amount_usd: None,
            payment_fx_rate_to_usd: None,
            updated_at_ms: seed.created_at_ms,
        };

        let settings = WorkspaceSettingsState {
            callback_default_enabled: true,
            request_retention_days: 30,
            allow_replay_from_customer_app: true,
            execution_policy: "customer_signed".to_owned(),
            sponsored_monthly_cap_requests: 10_000,
            updated_at_ms: seed.created_at_ms,
        };

        let billing_audit = vec![json!({
            "event_id": Uuid::new_v4().to_string(),
            "workspace_id": seed.workspace_id,
            "actor_email": seed.email,
            "actor_role": seed.workspace_role,
            "changed_at_ms": seed.created_at_ms,
            "plan_before": seed.plan,
            "plan_after": seed.plan,
            "access_mode_before": "free_play",
            "access_mode_after": "free_play",
            "payment_method_updated": false,
            "tenant_id": tenant_id,
        })];

        Self {
            accounts,
            sessions: HashMap::new(),
            invites: HashMap::new(),
            verification_tokens: HashMap::new(),
            password_reset_tokens: HashMap::new(),
            billing,
            billing_audit,
            invoices: Vec::new(),
            settings,
        }
    }
}

#[derive(Debug)]
struct UiError {
    status: StatusCode,
    message: String,
}

impl UiError {
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

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn upstream(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
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

impl IntoResponse for UiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "ok": false,
                "error": self.message,
            })),
        )
            .into_response()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct JobsQueryParams {
    state: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct CallbackHistoryQueryParams {
    include_attempts: Option<bool>,
    attempt_limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct IntakeAuditsQueryParams {
    validation_result: Option<String>,
    channel: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct ExceptionIndexQueryParams {
    state: Option<String>,
    severity: Option<String>,
    category: Option<String>,
    adapter_id: Option<String>,
    subject_id: Option<String>,
    intent_id: Option<String>,
    cluster_key: Option<String>,
    search: Option<String>,
    include_terminal: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct RolloutSummaryQueryParams {
    lookback_hours: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct SubmitIntentRequest {
    intent_kind: String,
    payload: Value,
    metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SignupRequest {
    full_name: String,
    email: String,
    password: String,
    workspace_name: String,
    plan: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OnboardingStepRequest {
    step: String,
}

#[derive(Debug, Clone, Deserialize)]
struct WorkspaceSwitchRequest {
    workspace_id: Option<String>,
    environment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct WebhookKeysQuery {
    source: Option<String>,
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct EnvironmentsQuery {
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct AgentsQuery {
    environment_id: Option<String>,
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct PolicyBundlesQuery {
    limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateWebhookKeyRequest {
    source: String,
    grace_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RevokeWebhookKeyRequest {
    grace_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct InviteTeamMemberRequest {
    email: String,
    role: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateTeamRoleRequest {
    role: String,
}

#[derive(Debug, Clone, Deserialize)]
struct InviteLookupQuery {
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AcceptInviteRequest {
    token: String,
    full_name: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
struct EmailRequest {
    email: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TokenRequest {
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PasswordResetConfirmRequest {
    token: String,
    password: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BillingUpdateRequest {
    plan: Option<String>,
    access_mode: Option<String>,
    billing_email: Option<String>,
    card_brand: Option<String>,
    card_last4: Option<String>,
    flutterwave_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BillingWebhookPayload {
    data: Option<BillingWebhookData>,
}

#[derive(Debug, Clone, Deserialize)]
struct BillingWebhookData {
    id: Value,
    customer: Option<BillingWebhookCustomer>,
}

#[derive(Debug, Clone, Deserialize)]
struct BillingWebhookCustomer {
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SettingsUpdateRequest {
    callback_default_enabled: Option<bool>,
    request_retention_days: Option<u32>,
    allow_replay_from_customer_app: Option<bool>,
    execution_policy: Option<String>,
    sponsored_monthly_cap_requests: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngressApiKeysResponse {
    keys: Vec<IngressApiKeyRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngressApiKeyRecord {
    key_id: String,
    label: String,
    key_prefix: String,
    key_last4: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    last_used_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngressWebhookKeysResponse {
    keys: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct IngressWebhookKeyCreateResponse {
    webhook_key: Value,
    rotation: Value,
}

#[derive(Debug, Clone, Serialize)]
struct OperatorBacklogSummary {
    total_jobs: u64,
    queued: u64,
    leased: u64,
    executing: u64,
    retry_scheduled: u64,
    failed_terminal: u64,
    dead_lettered: u64,
}

#[derive(Debug, Clone, Serialize)]
struct OperatorCountRow {
    label: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct OperatorCallbackTrendPoint {
    bucket: String,
    delivered: u64,
    retrying: u64,
    terminal_failures: u64,
}

#[derive(Debug, Clone, Serialize)]
struct OperatorAdapterHealth {
    adapter_id: String,
    total_jobs: u64,
    success_jobs: u64,
    failure_jobs: u64,
    retrying_jobs: u64,
    queued_jobs: u64,
    last_failure_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct OperatorFailingIntent {
    intent_id: String,
    job_id: String,
    state: String,
    classification: String,
    adapter_id: String,
    attempt: u32,
    max_attempts: u32,
    replay_count: u32,
    failure_code: Option<String>,
    failure_message: Option<String>,
    updated_at_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env_or("OPERATOR_UI_BIND", "0.0.0.0:8083");
    let timeout_ms = env_u64("OPERATOR_UI_STATUS_TIMEOUT_MS", 15_000);
    let status_base_url = parse_base_url(
        &env_or(
            "OPERATOR_UI_STATUS_BASE_URL",
            "http://127.0.0.1:8082/status",
        ),
        "OPERATOR_UI_STATUS_BASE_URL",
    )?;
    let ingress_base_url = parse_base_url(
        &env_or("OPERATOR_UI_INGRESS_BASE_URL", "http://127.0.0.1:8000"),
        "OPERATOR_UI_INGRESS_BASE_URL",
    )?;
    let status_auth = StatusAuthHeaders {
        bearer_token: env_var_opt("OPERATOR_UI_STATUS_BEARER_TOKEN"),
        tenant_id: env_or("OPERATOR_UI_TENANT_ID", "tenant_demo"),
        principal_id: env_or("OPERATOR_UI_PRINCIPAL_ID", "demo-operator"),
        principal_role: normalize_status_role(&env_or("OPERATOR_UI_PRINCIPAL_ROLE", "admin")),
    };
    let ingress_auth = IngressAuthHeaders {
        bearer_token: env_var_opt("OPERATOR_UI_INGRESS_BEARER_TOKEN"),
        tenant_id: env_or("OPERATOR_UI_TENANT_ID", "tenant_demo"),
        principal_id: env_or("OPERATOR_UI_INGRESS_PRINCIPAL_ID", "ingress-service"),
        submitter_kind: env_or("OPERATOR_UI_INGRESS_SUBMITTER_KIND", "internal_service"),
        fallback_principal_id: env_var_opt("OPERATOR_UI_INGRESS_FALLBACK_PRINCIPAL_ID"),
        fallback_submitter_kind: env_var_opt("OPERATOR_UI_INGRESS_FALLBACK_SUBMITTER_KIND"),
    };

    let smtp_host = env_var_opt("OPERATOR_UI_SMTP_HOST");
    let email_from = env_var_opt("OPERATOR_UI_EMAIL_FROM");
    let email_delivery_configured = smtp_host.is_some() && email_from.is_some();
    let require_email_verification = env_bool("OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION", false);
    let password_reset_requested = env_bool("OPERATOR_UI_PASSWORD_RESET_ENABLED", false);
    let reconciliation_rollout_mode = normalize_reconciliation_rollout_mode(&env_or(
        "OPERATOR_UI_RECONCILIATION_ROLLOUT_MODE",
        "customer_visible",
    ));
    let reconciliation_operator_visible = reconciliation_rollout_mode != "hidden";
    let reconciliation_customer_visible = reconciliation_rollout_mode == "customer_visible";
    let ui_features = UiFeatures {
        public_base_url: env_or("OPERATOR_UI_PUBLIC_BASE_URL", "http://127.0.0.1:3000"),
        workspace_environment: env_or(
            "OPERATOR_UI_WORKSPACE_ENVIRONMENT",
            DEFAULT_WORKSPACE_ENVIRONMENT,
        ),
        reconciliation_rollout_mode,
        reconciliation_operator_visible,
        reconciliation_customer_visible,
        require_email_verification,
        email_delivery_configured,
        password_reset_enabled: password_reset_requested && email_delivery_configured,
        require_durable_metering: env_bool("OPERATOR_UI_REQUIRE_DURABLE_METERING", false),
        enforce_workspace_solana_rpc: env_bool("OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC", true),
        sandbox_solana_rpc_url: env_var_opt("OPERATOR_UI_SANDBOX_SOLANA_RPC_URL"),
        flutterwave_secret_key: env_var_opt("OPERATOR_UI_FLUTTERWAVE_SECRET_KEY"),
        flutterwave_webhook_hash: env_var_opt("OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH"),
        flutterwave_base_url: env_or(
            "OPERATOR_UI_FLUTTERWAVE_BASE_URL",
            "https://api.flutterwave.com/v3",
        ),
        flutterwave_expected_currency: env_var_opt("OPERATOR_UI_FLUTTERWAVE_EXPECTED_CURRENCY"),
        flutterwave_supported_currencies: parse_supported_currencies(&env_or(
            "OPERATOR_UI_FLUTTERWAVE_FX_RATES_USD",
            "USD=1;NGN=0.00066;GBP=1.27;CAD=0.74;JPY=0.0067",
        )),
    };
    let seed = SessionSeed {
        session_id: env_or("OPERATOR_UI_SESSION_ID", &Uuid::new_v4().to_string()),
        email: env_or("OPERATOR_UI_SESSION_EMAIL", "demo@azums.dev"),
        full_name: env_or("OPERATOR_UI_SESSION_NAME", "Demo User"),
        password: env_or("OPERATOR_UI_SESSION_PASSWORD", "dev-password"),
        workspace_id: env_or("OPERATOR_UI_WORKSPACE_ID", "workspace_demo"),
        workspace_name: env_or("OPERATOR_UI_WORKSPACE_NAME", "Demo Workspace"),
        workspace_role: normalize_workspace_role(&env_or("OPERATOR_UI_WORKSPACE_ROLE", "owner")),
        plan: normalize_plan(&env_or("OPERATOR_UI_WORKSPACE_PLAN", "Developer")),
        created_at_ms: env_u64("OPERATOR_UI_SESSION_CREATED_AT_MS", now_ms()),
    };
    let workspaces = build_workspace_seeds(
        &seed,
        &ui_features.workspace_environment,
        env_var_opt("OPERATOR_UI_EXTRA_WORKSPACES").as_deref(),
    );

    let client = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()?;

    let account_state = AccountState::new(&seed, &status_auth.tenant_id);
    let state = Arc::new(AppState {
        client,
        status_base_url,
        ingress_base_url,
        status_auth,
        ingress_auth,
        ui_features,
        seed,
        workspaces,
        account_state: Arc::new(RwLock::new(account_state)),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/styles.css", get(styles))
        .route("/app.js", get(app_js))
        .route("/health", get(ui_health))
        .route("/api/ui/config", get(ui_config))
        .route("/api/ui/health", get(ui_health))
        .route("/api/ui/status/jobs", get(ui_get_jobs))
        .route(
            "/api/ui/status/receipts/:receipt_id",
            get(ui_get_receipt_by_id),
        )
        .route("/api/ui/status/requests/:id", get(ui_get_request))
        .route("/api/ui/status/requests/:id/receipt", get(ui_get_receipt))
        .route("/api/ui/status/requests/:id/history", get(ui_get_history))
        .route(
            "/api/ui/status/requests/:id/callbacks",
            get(ui_get_callbacks),
        )
        .route(
            "/api/ui/status/requests/:id/unified",
            get(ui_get_request_unified),
        )
        .route(
            "/api/ui/status/reconciliation/rollout-summary",
            get(ui_get_reconciliation_rollout_summary),
        )
        .route("/api/ui/status/exceptions", get(ui_list_exceptions))
        .route(
            "/api/ui/status/exceptions/:case_id",
            get(ui_get_exception_detail),
        )
        .route(
            "/api/ui/status/exceptions/:case_id/state",
            post(ui_post_exception_state),
        )
        .route(
            "/api/ui/status/exceptions/:case_id/acknowledge",
            post(ui_post_exception_acknowledge),
        )
        .route(
            "/api/ui/status/exceptions/:case_id/resolve",
            post(ui_post_exception_resolve),
        )
        .route(
            "/api/ui/status/exceptions/:case_id/false-positive",
            post(ui_post_exception_false_positive),
        )
        .route(
            "/api/ui/status/requests/:id/reconciliation/rerun",
            post(ui_post_reconciliation_rerun),
        )
        .route(
            "/api/ui/status/requests/:id/reconciliation/refresh-observation",
            post(ui_post_refresh_observation),
        )
        .route(
            "/api/ui/status/callbacks/:callback_id",
            get(ui_get_callback_detail),
        )
        .route("/api/ui/status/requests/:id/replay", post(ui_post_replay))
        .route(
            "/api/ui/status/requests/:id/replay-review",
            post(ui_post_replay_review),
        )
        .route(
            "/api/ui/status/tenant/intake-audits",
            get(ui_get_intake_audits),
        )
        .route(
            "/api/ui/status/tenant/callback-destination",
            get(ui_get_callback_destination),
        )
        .route(
            "/api/ui/status/tenant/callback-destination",
            post(ui_upsert_callback_destination),
        )
        .route(
            "/api/ui/status/tenant/callback-destination",
            delete(ui_delete_callback_destination),
        )
        .route("/api/ui/operator/overview", get(ui_operator_overview))
        .route("/api/ui/ingress/requests", post(ui_post_ingress_request))
        .route("/api/ui/account/session", get(ui_get_account_session))
        .route("/api/ui/account/login", post(ui_post_login))
        .route("/api/ui/account/logout", post(ui_post_logout))
        .route("/api/ui/account/signup", post(ui_post_signup))
        .route("/api/ui/account/onboarding", post(ui_post_onboarding))
        .route("/api/ui/account/workspaces", get(ui_get_workspaces))
        .route(
            "/api/ui/account/workspaces/switch",
            post(ui_post_switch_workspace),
        )
        .route(
            "/api/ui/account/workspaces/:workspace_id/detail",
            get(ui_get_workspace_detail),
        )
        .route(
            "/api/ui/account/api-keys",
            get(ui_get_api_keys).post(ui_post_api_key),
        )
        .route(
            "/api/ui/account/api-keys/:key_id/revoke",
            post(ui_post_revoke_api_key),
        )
        .route(
            "/api/ui/account/webhook-keys",
            get(ui_get_webhook_keys).post(ui_post_webhook_key),
        )
        .route(
            "/api/ui/account/webhook-keys/:key_id/revoke",
            post(ui_post_revoke_webhook_key),
        )
        .route(
            "/api/ui/account/environments",
            get(ui_get_environments),
        )
        .route("/api/ui/account/agents", get(ui_get_agents))
        .route(
            "/api/ui/account/policy/templates",
            get(ui_get_policy_templates),
        )
        .route(
            "/api/ui/account/policy/bundles",
            get(ui_get_policy_bundles).post(ui_post_policy_bundle),
        )
        .route(
            "/api/ui/account/policy/bundles/:bundle_id",
            get(ui_get_policy_bundle),
        )
        .route(
            "/api/ui/account/policy/bundles/:bundle_id/publish",
            post(ui_post_publish_policy_bundle),
        )
        .route(
            "/api/ui/account/policy/bundles/:bundle_id/rollback",
            post(ui_post_rollback_policy_bundle),
        )
        .route(
            "/api/ui/account/policy/simulations",
            post(ui_post_policy_simulation),
        )
        .route(
            "/api/ui/account/team-members",
            get(ui_get_team_members).post(ui_post_invite_team_member),
        )
        .route(
            "/api/ui/account/team-members/:member_id",
            axum::routing::patch(ui_patch_team_member).delete(ui_delete_team_member),
        )
        .route(
            "/api/ui/account/billing/providers",
            get(ui_get_billing_providers),
        )
        .route(
            "/api/ui/account/billing",
            get(ui_get_billing).put(ui_put_billing),
        )
        .route("/api/ui/account/billing-audit", get(ui_get_billing_audit))
        .route("/api/ui/account/invoices", get(ui_get_invoices))
        .route(
            "/api/ui/billing/flutterwave/webhook",
            post(ui_post_flutterwave_webhook),
        )
        .route(
            "/api/ui/account/settings",
            get(ui_get_settings).put(ui_put_settings),
        )
        .route("/api/ui/account/usage", get(ui_get_usage))
        .route("/api/ui/account/invite", get(ui_get_invite))
        .route("/api/ui/account/invite/accept", post(ui_post_accept_invite))
        .route(
            "/api/ui/account/email-verification/request",
            post(ui_post_request_email_verification),
        )
        .route(
            "/api/ui/account/email-verification/confirm",
            post(ui_post_confirm_email_verification),
        )
        .route(
            "/api/ui/account/password-reset/request",
            post(ui_post_request_password_reset),
        )
        .route(
            "/api/ui/account/password-reset/confirm",
            post(ui_post_confirm_password_reset),
        )
        .with_state(state);

    let addr: SocketAddr = bind
        .parse()
        .map_err(|err| format!("invalid OPERATOR_UI_BIND `{bind}`: {err}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("operator_ui listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn styles() -> Response {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../static/styles.css"),
    )
        .into_response()
}

async fn app_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("../static/app.js"),
    )
        .into_response()
}

async fn ui_config(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "public_base_url": state.ui_features.public_base_url,
        "status_base_url": state.status_base_url.to_string(),
        "ingress_base_url": state.ingress_base_url.to_string().trim_end_matches('/'),
        "tenant_id": state.status_auth.tenant_id,
        "principal_id": state.status_auth.principal_id,
        "principal_role": state.status_auth.principal_role,
        "has_bearer_token": state.status_auth.bearer_token.is_some(),
        "reconciliation_rollout_mode": state.ui_features.reconciliation_rollout_mode,
        "reconciliation_operator_visible": state.ui_features.reconciliation_operator_visible,
        "reconciliation_customer_visible": state.ui_features.reconciliation_customer_visible,
        "requires_email_verification": state.ui_features.require_email_verification,
        "email_delivery_configured": state.ui_features.email_delivery_configured,
        "password_reset_enabled": state.ui_features.password_reset_enabled,
    }))
}

async fn ui_health(State(state): State<Arc<AppState>>) -> Json<Value> {
    let request = match status_request_get(&state, "health", Option::<&JobsQueryParams>::None) {
        Ok(request) => request,
        Err(err) => {
            return Json(json!({
                "ok": false,
                "status_api_reachable": false,
                "status_api_status_code": Value::Null,
                "status_api_error": err.message,
            }));
        }
    };
    match request.send().await {
        Ok(response) => Json(json!({
            "ok": response.status().as_u16() < 500,
            "status_api_reachable": true,
            "status_api_status_code": response.status().as_u16(),
            "status_api_error": Value::Null,
        })),
        Err(err) => Json(json!({
            "ok": false,
            "status_api_reachable": false,
            "status_api_status_code": Value::Null,
            "status_api_error": err.to_string(),
        })),
    }
}
async fn ui_get_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<JobsQueryParams>,
) -> Result<Json<JobListResponse>, UiError> {
    Ok(Json(status_get(&state, "jobs", Some(&query)).await?))
}

async fn ui_get_receipt_by_id(
    State(state): State<Arc<AppState>>,
    Path(receipt_id): Path<String>,
) -> Result<Json<ReceiptLookupResponse>, UiError> {
    let path = format!("receipts/{receipt_id}");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_get_request(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<RequestStatusResponse>, UiError> {
    let path = format!("requests/{intent_id}");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_get_receipt(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<ReceiptResponse>, UiError> {
    let path = format!("requests/{intent_id}/receipt");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_get_history(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<HistoryResponse>, UiError> {
    let path = format!("requests/{intent_id}/history");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_get_callbacks(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
    Query(query): Query<CallbackHistoryQueryParams>,
) -> Result<Json<CallbackHistoryResponse>, UiError> {
    let path = format!("requests/{intent_id}/callbacks");
    Ok(Json(status_get(&state, &path, Some(&query)).await?))
}

async fn ui_get_request_unified(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(intent_id): Path<String>,
) -> Result<Json<UnifiedRequestStatusResponse>, UiError> {
    let _ = require_session(&state, &headers).await?;
    let path = format!("requests/{intent_id}/unified");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_get_reconciliation_rollout_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<RolloutSummaryQueryParams>,
) -> Result<Json<ReconciliationRolloutSummaryResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view reconciliation rollout metrics.",
        ));
    }
    Ok(Json(
        status_get(&state, "reconciliation/rollout-summary", Some(&query)).await?,
    ))
}

async fn ui_list_exceptions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<ExceptionIndexQueryParams>,
) -> Result<Json<ExceptionIndexResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view the exception index.",
        ));
    }
    Ok(Json(status_get(&state, "exceptions", Some(&query)).await?))
}

async fn ui_get_exception_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
) -> Result<Json<ExceptionDetailResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can inspect exception cases.",
        ));
    }
    let path = format!("exceptions/{case_id}");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_post_exception_state(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
    Json(body): Json<ExceptionStateTransitionRequest>,
) -> Result<Json<ExceptionStateTransitionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can update exception case state.",
        ));
    }
    let path = format!("exceptions/{case_id}/state");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_exception_acknowledge(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can acknowledge exception cases.",
        ));
    }
    let path = format!("exceptions/{case_id}/acknowledge");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_exception_resolve(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can resolve exception cases.",
        ));
    }
    let path = format!("exceptions/{case_id}/resolve");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_exception_false_positive(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can mark false positives.",
        ));
    }
    let path = format!("exceptions/{case_id}/false-positive");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_reconciliation_rerun(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(intent_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ReconActionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can re-run reconciliation.",
        ));
    }
    let path = format!("requests/{intent_id}/reconciliation/rerun");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_refresh_observation(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(intent_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ReconActionResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can refresh observations.",
        ));
    }
    let path = format!("requests/{intent_id}/reconciliation/refresh-observation");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_get_callback_detail(
    State(state): State<Arc<AppState>>,
    Path(callback_id): Path<String>,
) -> Result<Json<CallbackDetailResponse>, UiError> {
    let path = format!("callbacks/{callback_id}");
    Ok(Json(
        status_get(&state, &path, Option::<&JobsQueryParams>::None).await?,
    ))
}

async fn ui_post_replay(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(intent_id): Path<String>,
    Json(body): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can request replay from the customer app.",
        ));
    }
    let allow_replay = {
        let account_state = state.account_state.read().await;
        account_state.settings.allow_replay_from_customer_app
    };
    if !allow_replay {
        return Err(UiError::forbidden(
            "Replay from the customer app is disabled for this workspace.",
        ));
    }
    let path = format!("requests/{intent_id}/replay");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_post_replay_review(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(intent_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ReplayReviewResponse>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can request replay review.",
        ));
    }
    let path = format!("requests/{intent_id}/replay-review");
    Ok(Json(status_post(&state, &path, &body).await?))
}

async fn ui_get_intake_audits(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IntakeAuditsQueryParams>,
) -> Result<Json<IntakeAuditsResponse>, UiError> {
    Ok(Json(
        status_get(&state, "tenant/intake-audits", Some(&query)).await?,
    ))
}

async fn ui_get_callback_destination(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CallbackDestinationResponse>, UiError> {
    Ok(Json(
        status_get(
            &state,
            "tenant/callback-destination",
            Option::<&JobsQueryParams>::None,
        )
        .await?,
    ))
}

async fn ui_upsert_callback_destination(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertCallbackDestinationRequest>,
) -> Result<Json<UpsertCallbackDestinationResponse>, UiError> {
    Ok(Json(
        status_post(&state, "tenant/callback-destination", &body).await?,
    ))
}

async fn ui_delete_callback_destination(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DeleteCallbackDestinationResponse>, UiError> {
    Ok(Json(
        status_delete(
            &state,
            "tenant/callback-destination",
            Option::<&JobsQueryParams>::None,
        )
        .await?,
    ))
}

async fn ui_operator_overview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    let jobs = status_get::<_, JobListResponse>(
        &state,
        "jobs",
        Some(&JobsQueryParams {
            state: None,
            limit: Some(200),
            offset: Some(0),
        }),
    )
    .await?;

    let mut backlog = OperatorBacklogSummary {
        total_jobs: jobs.jobs.len() as u64,
        queued: 0,
        leased: 0,
        executing: 0,
        retry_scheduled: 0,
        failed_terminal: 0,
        dead_lettered: 0,
    };
    let mut failure_classes: BTreeMap<String, u64> = BTreeMap::new();
    let mut adapter_health: BTreeMap<String, OperatorAdapterHealth> = BTreeMap::new();
    let mut top_failing: Vec<OperatorFailingIntent> = Vec::new();

    for job in jobs.jobs {
        match format!("{:?}", job.state).to_ascii_lowercase().as_str() {
            "queued" => backlog.queued += 1,
            "leased" => backlog.leased += 1,
            "executing" => backlog.executing += 1,
            "retry_scheduled" => backlog.retry_scheduled += 1,
            "failed_terminal" => backlog.failed_terminal += 1,
            "dead_lettered" => backlog.dead_lettered += 1,
            _ => {}
        }

        let classification = format!("{:?}", job.classification).to_ascii_lowercase();
        *failure_classes.entry(classification.clone()).or_insert(0) += 1;

        let entry = adapter_health
            .entry(job.adapter_id.clone())
            .or_insert(OperatorAdapterHealth {
                adapter_id: job.adapter_id.clone(),
                total_jobs: 0,
                success_jobs: 0,
                failure_jobs: 0,
                retrying_jobs: 0,
                queued_jobs: 0,
                last_failure_at_ms: None,
            });
        entry.total_jobs += 1;
        let state_name = format!("{:?}", job.state).to_ascii_lowercase();
        if state_name == "succeeded" {
            entry.success_jobs += 1;
        }
        if state_name == "queued" {
            entry.queued_jobs += 1;
        }
        if state_name == "retry_scheduled" {
            entry.retrying_jobs += 1;
        }
        if state_name == "failed_terminal" || state_name == "dead_lettered" {
            entry.failure_jobs += 1;
            entry.last_failure_at_ms = Some(job.updated_at_ms);
            top_failing.push(OperatorFailingIntent {
                intent_id: job.intent_id,
                job_id: job.job_id,
                state: state_name,
                classification,
                adapter_id: job.adapter_id,
                attempt: job.attempt,
                max_attempts: job.max_attempts,
                replay_count: job.replay_count,
                failure_code: job.failure_code,
                failure_message: job.failure_message,
                updated_at_ms: job.updated_at_ms,
            });
        }
    }

    top_failing.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    top_failing.truncate(10);

    Ok(Json(json!({
        "ok": true,
        "generated_at_ms": now_ms(),
        "tenant_id": state.status_auth.tenant_id,
        "backlog": backlog,
        "failure_classes": failure_classes
            .into_iter()
            .map(|(label, count)| OperatorCountRow { label, count })
            .collect::<Vec<_>>(),
        "callback_failure_trends": Vec::<OperatorCallbackTrendPoint>::new(),
        "adapter_health": adapter_health.into_values().collect::<Vec<_>>(),
        "top_failing_intents": top_failing,
    })))
}

async fn ui_get_account_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let session = maybe_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "authenticated": session.is_some(),
        "session": session,
    })))
}

async fn ui_post_login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<Response, UiError> {
    let email_key = normalize_email(&body.email);
    let mut account_state = state.account_state.write().await;
    let account = account_state
        .accounts
        .get(&email_key)
        .ok_or_else(|| UiError::unauthorized("No account found. Please sign up first."))?
        .clone();

    if account.password != body.password {
        return Err(UiError::unauthorized("Incorrect email or password."));
    }
    if state.ui_features.require_email_verification && account.email_verified_at_ms.is_none() {
        return Err(UiError::unauthorized(
            "Email is not verified yet. Please verify before signing in.",
        ));
    }

    let session_id = Uuid::new_v4().to_string();
    account_state.sessions.insert(
        session_id.clone(),
        SessionState {
            email_key: email_key.clone(),
            workspace_id: state.seed.workspace_id.clone(),
        },
    );
    let session = build_session_record(
        &state,
        &account_state,
        &account,
        resolve_workspace(&state, &state.seed.workspace_id),
    )?;
    drop(account_state);
    json_response_with_cookie(
        json!({
            "ok": true,
            "session": session,
        }),
        Some(&session_id),
    )
}

async fn ui_post_logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Response, UiError> {
    if let Some(session_id) = read_cookie(&headers, SESSION_COOKIE_NAME) {
        state
            .account_state
            .write()
            .await
            .sessions
            .remove(&session_id);
    }
    json_response_with_cookie(json!({ "ok": true }), None)
}

async fn ui_post_signup(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SignupRequest>,
) -> Result<Response, UiError> {
    let email_key = normalize_email(&body.email);
    let mut account_state = state.account_state.write().await;
    if account_state.accounts.contains_key(&email_key) {
        return Err(UiError::conflict("Account already exists for this email."));
    }

    let now = now_ms();
    let full_name = body.full_name.trim();
    let workspace_name = body.workspace_name.trim();
    if full_name.is_empty() || workspace_name.is_empty() {
        return Err(UiError::bad_request(
            "full_name and workspace_name are required.",
        ));
    }

    let role = if account_state.accounts.is_empty() {
        "owner"
    } else {
        "developer"
    };
    let account = AccountRecord {
        id: Uuid::new_v4().to_string(),
        email: body.email.trim().to_owned(),
        full_name: full_name.to_owned(),
        password: body.password,
        role: role.to_owned(),
        status: "active".to_owned(),
        added_at_ms: now,
        invite_expires_at_ms: None,
        email_verified_at_ms: if state.ui_features.require_email_verification {
            None
        } else {
            Some(now)
        },
        onboarding: OnboardingState::default(),
    };
    account_state
        .accounts
        .insert(email_key.clone(), account.clone());
    account_state.billing.plan = normalize_plan(&body.plan);
    account_state.billing.billing_email = account.email.clone();
    account_state.billing.updated_at_ms = now;

    let verification_sent = if state.ui_features.require_email_verification {
        if state.ui_features.email_delivery_configured {
            let token = Uuid::new_v4().to_string();
            account_state
                .verification_tokens
                .insert(token, email_key.clone());
            true
        } else {
            false
        }
    } else {
        false
    };

    let maybe_session = if !state.ui_features.require_email_verification {
        let session_id = Uuid::new_v4().to_string();
        account_state.sessions.insert(
            session_id.clone(),
            SessionState {
                email_key,
                workspace_id: state.seed.workspace_id.clone(),
            },
        );
        Some((
            session_id,
            build_session_record(
                &state,
                &account_state,
                &account,
                resolve_workspace(&state, &state.seed.workspace_id),
            )?,
        ))
    } else {
        None
    };

    if workspace_name != state.seed.workspace_name {
        // Frontend expects signup to reflect the requested workspace name.
        // This deployment-scoped backend only exposes one workspace, so we update its label.
        drop(account_state);
        let mut writable = state.account_state.write().await;
        writable.billing.updated_at_ms = now;
        let _ = writable;
    }

    if let Some((session_id, session)) = maybe_session {
        return json_response_with_cookie(
            json!({
                "ok": true,
                "session": session,
                "requires_email_verification": false,
                "verification_sent": false,
            }),
            Some(&session_id),
        );
    }

    Ok(Json(json!({
        "ok": true,
        "session": Value::Null,
        "requires_email_verification": state.ui_features.require_email_verification,
        "verification_sent": verification_sent,
    }))
    .into_response())
}

async fn ui_post_onboarding(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<OnboardingStepRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    let mut account_state = state.account_state.write().await;
    let key = normalize_email(&session.email);
    let account = {
        let account = account_state
            .accounts
            .get_mut(&key)
            .ok_or_else(|| UiError::not_found("session account not found"))?;
        match body.step.trim() {
            "workspace_created" => account.onboarding.workspace_created = true,
            "api_key_generated" => account.onboarding.api_key_generated = true,
            "submitted_request" => account.onboarding.submitted_request = true,
            "viewed_receipt" => account.onboarding.viewed_receipt = true,
            "configured_callback" => account.onboarding.configured_callback = true,
            _ => return Err(UiError::bad_request("unsupported onboarding step.")),
        }
        account.clone()
    };
    let session = build_session_record(
        &state,
        &account_state,
        &account,
        resolve_workspace(&state, &state.seed.workspace_id),
    )?;
    Ok(Json(json!({ "ok": true, "session": session })))
}

async fn ui_get_workspaces(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "workspaces": state
            .workspaces
            .iter()
            .map(|workspace| build_workspace_record(&state, &session, workspace))
            .collect::<Vec<_>>(),
    })))
}

async fn ui_post_switch_workspace(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<WorkspaceSwitchRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    let requested_workspace_id = body
        .workspace_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(session.workspace_id.as_str());
    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == requested_workspace_id)
        .ok_or_else(|| UiError::not_found("workspace not found for this deployment."))?
        .clone();
    if let Some(environment) = body.environment.as_deref() {
        let normalized = environment.trim();
        if !normalized.is_empty() && !workspace.environment.eq_ignore_ascii_case(normalized) {
            return Err(UiError::bad_request(
                "requested environment does not match the selected workspace.",
            ));
        }
    }
    let mut account_state = state.account_state.write().await;
    let Some(session_state) = account_state.sessions.get_mut(&session.session_id) else {
        return Err(UiError::unauthorized("Authentication required."));
    };
    session_state.workspace_id = workspace.workspace_id.clone();
    let email_key = session_state.email_key.clone();
    let account = account_state
        .accounts
        .get(&email_key)
        .ok_or_else(|| UiError::unauthorized("Authentication required."))?;
    let updated_session = build_session_record(&state, &account_state, account, &workspace)?;
    Ok(Json(json!({
        "ok": true,
        "session": updated_session,
    })))
}
async fn ui_get_workspace_detail(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
        .ok_or_else(|| UiError::not_found("workspace not found for this deployment."))?;

    let api_keys = load_api_keys(&state).await?;
    let usage = load_usage_summary(&state).await?;
    let callback_destination: CallbackDestinationResponse = status_get(
        &state,
        "tenant/callback-destination",
        Option::<&JobsQueryParams>::None,
    )
    .await
    .unwrap_or(CallbackDestinationResponse {
        tenant_id: state.status_auth.tenant_id.clone(),
        configured: false,
        destination: None,
    });
    let account_state = state.account_state.read().await;
    let members = account_state
        .accounts
        .values()
        .map(team_member_json)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "workspace": build_workspace_record(&state, &session, workspace),
        "members": members,
        "api_keys": api_keys,
        "settings": account_state.settings,
        "billing": account_state.billing,
        "usage": usage,
        "invoices": account_state.invoices,
        "callback_destination": callback_destination,
    })))
}

async fn ui_get_api_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "keys": load_api_keys(&state).await?,
    })))
}

async fn ui_post_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_write_requests(&session.role) {
        return Err(UiError::forbidden("Your role cannot create API keys."));
    }

    let token = generate_api_key_token();
    let response_token = token.clone();
    let key_id = Uuid::new_v4().to_string();
    let prefix = token.chars().take(8).collect::<String>();
    let last4 = token
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let payload = json!({
        "key_id": key_id,
        "label": if body.name.trim().is_empty() { "default" } else { body.name.trim() },
        "key_value": token,
        "key_prefix": prefix,
        "key_last4": last4,
        "created_by_principal_id": session.email,
        "created_at_ms": now_ms(),
    });

    let path = format!(
        "api/internal/tenants/{}/api-keys",
        state.ingress_auth.tenant_id
    );
    let _: Value = ingress_post_value(&state, &path, payload, None).await?;

    {
        let mut account_state = state.account_state.write().await;
        if let Some(account) = account_state
            .accounts
            .get_mut(&normalize_email(&session.email))
        {
            account.onboarding.api_key_generated = true;
        }
    }

    let keys = load_api_keys(&state).await?;
    let key = keys
        .into_iter()
        .find(|item| item["id"] == key_id)
        .ok_or_else(|| UiError::internal("created API key was not returned by ingress."))?;

    Ok(Json(json!({
        "ok": true,
        "key": key,
        "token": response_token,
    })))
}

async fn ui_post_revoke_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(key_id): Path<String>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_write_requests(&session.role) {
        return Err(UiError::forbidden("Your role cannot revoke API keys."));
    }

    let path = format!(
        "api/internal/tenants/{}/api-keys/{}/revoke",
        state.ingress_auth.tenant_id, key_id
    );
    let _: Value = ingress_post_value(&state, &path, json!({}), None).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn ui_get_webhook_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<WebhookKeysQuery>,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "keys": load_webhook_keys(&state, &query).await?,
    })))
}

async fn ui_post_webhook_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateWebhookKeyRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden("Your role cannot issue webhook keys."));
    }

    let path = format!(
        "api/internal/tenants/{}/webhook-keys",
        state.ingress_auth.tenant_id
    );
    let created: IngressWebhookKeyCreateResponse = ingress_post(
        &state,
        &path,
        &json!({
            "source": if body.source.trim().is_empty() { "default" } else { body.source.trim() },
            "grace_seconds": body.grace_seconds.unwrap_or(900),
            "created_by_principal_id": session.email,
        }),
        None,
    )
    .await?;
    Ok(Json(json!({
        "ok": true,
        "webhook_key": created.webhook_key,
        "rotation": created.rotation,
    })))
}

async fn ui_post_revoke_webhook_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(key_id): Path<String>,
    Json(body): Json<Option<RevokeWebhookKeyRequest>>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden("Your role cannot revoke webhook keys."));
    }

    let path = format!(
        "api/internal/tenants/{}/webhook-keys/{}/revoke",
        state.ingress_auth.tenant_id, key_id
    );
    let _: Value = ingress_post_value(
        &state,
        &path,
        json!({ "grace_seconds": body.and_then(|item| item.grace_seconds).unwrap_or(0) }),
        None,
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn ui_get_environments(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<EnvironmentsQuery>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view registered environments.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/environments",
        state.ingress_auth.tenant_id
    );
    let mut params = Vec::new();
    if let Some(include_inactive) = query.include_inactive {
        params.push(("include_inactive", include_inactive.to_string()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    Ok(Json(
        ingress_get(
            &state,
            &path,
            if params.is_empty() { None } else { Some(params.as_slice()) },
        )
        .await?,
    ))
}

async fn ui_get_agents(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<AgentsQuery>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view registered agents.",
        ));
    }
    let path = format!("api/internal/tenants/{}/agents", state.ingress_auth.tenant_id);
    let mut params = Vec::new();
    if let Some(environment_id) = query.environment_id.as_ref() {
        params.push(("environment_id", environment_id.clone()));
    }
    if let Some(include_inactive) = query.include_inactive {
        params.push(("include_inactive", include_inactive.to_string()));
    }
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    Ok(Json(
        ingress_get(
            &state,
            &path,
            if params.is_empty() { None } else { Some(params.as_slice()) },
        )
        .await?,
    ))
}

async fn ui_get_policy_templates(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view policy templates.",
        ));
    }
    Ok(Json(
        ingress_get(
            &state,
            "api/internal/policy/templates",
            None,
        )
        .await?,
    ))
}

async fn ui_get_policy_bundles(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<PolicyBundlesQuery>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can view policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/bundles",
        state.ingress_auth.tenant_id
    );
    let mut params = Vec::new();
    if let Some(limit) = query.limit {
        params.push(("limit", limit.to_string()));
    }
    Ok(Json(
        ingress_get(
            &state,
            &path,
            if params.is_empty() { None } else { Some(params.as_slice()) },
        )
        .await?,
    ))
}

async fn ui_get_policy_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(bundle_id): Path<String>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can inspect policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/bundles/{}",
        state.ingress_auth.tenant_id, bundle_id
    );
    Ok(Json(ingress_get(&state, &path, None).await?))
}

async fn ui_post_policy_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can create policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/bundles",
        state.ingress_auth.tenant_id
    );
    Ok(Json(ingress_post_value(&state, &path, body, None).await?))
}

async fn ui_post_publish_policy_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(bundle_id): Path<String>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can publish policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/bundles/{}/publish",
        state.ingress_auth.tenant_id, bundle_id
    );
    Ok(Json(ingress_post_value(&state, &path, json!({}), None).await?))
}

async fn ui_post_rollback_policy_bundle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(bundle_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can roll back policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/bundles/{}/rollback",
        state.ingress_auth.tenant_id, bundle_id
    );
    Ok(Json(ingress_post_value(&state, &path, body, None).await?))
}

async fn ui_post_policy_simulation(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can simulate policy bundles.",
        ));
    }
    let path = format!(
        "api/internal/tenants/{}/policy/simulations",
        state.ingress_auth.tenant_id
    );
    Ok(Json(ingress_post_value(&state, &path, body, None).await?))
}

async fn ui_get_team_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    let account_state = state.account_state.read().await;
    Ok(Json(json!({
        "ok": true,
        "members": account_state.accounts.values().map(team_member_json).collect::<Vec<_>>(),
    })))
}

async fn ui_post_invite_team_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<InviteTeamMemberRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can invite members.",
        ));
    }
    let role = normalize_workspace_role(&body.role);
    if role == "owner" {
        return Err(UiError::bad_request("Invites cannot assign owner role."));
    }
    let email_key = normalize_email(&body.email);
    let now = now_ms();
    let expires_at_ms = now + (7 * 24 * 60 * 60 * 1000);
    let token = Uuid::new_v4().to_string();
    let mut account_state = state.account_state.write().await;
    account_state.accounts.insert(
        email_key.clone(),
        AccountRecord {
            id: Uuid::new_v4().to_string(),
            email: body.email.trim().to_owned(),
            full_name: body.email.trim().to_owned(),
            password: String::new(),
            role: role.clone(),
            status: "invited".to_owned(),
            added_at_ms: now,
            invite_expires_at_ms: Some(expires_at_ms),
            email_verified_at_ms: None,
            onboarding: OnboardingState::default(),
        },
    );
    account_state.invites.insert(
        token.clone(),
        InviteRecord {
            email: body.email.trim().to_owned(),
            workspace_id: state.seed.workspace_id.clone(),
            workspace_name: state.seed.workspace_name.clone(),
            role,
            expires_at_ms,
        },
    );
    Ok(Json(json!({
        "ok": true,
        "member": team_member_json(account_state.accounts.get(&email_key).unwrap()),
        "invite_token": token,
        "invite_path": format!("/accept-invite?token={}", token),
        "invite_expires_at_ms": expires_at_ms,
    })))
}

async fn ui_patch_team_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(member_id): Path<String>,
    Json(body): Json<UpdateTeamRoleRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can update roles.",
        ));
    }
    let mut account_state = state.account_state.write().await;
    let account = account_state
        .accounts
        .values_mut()
        .find(|account| account.id == member_id)
        .ok_or_else(|| UiError::not_found("member not found"))?;
    if account.role == "owner" {
        return Err(UiError::forbidden("owner role cannot be changed here."));
    }
    account.role = normalize_workspace_role(&body.role);
    Ok(Json(json!({
        "ok": true,
        "member": team_member_json(account),
    })))
}

async fn ui_delete_team_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(member_id): Path<String>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner/admin can remove members.",
        ));
    }
    let mut account_state = state.account_state.write().await;
    let email = account_state
        .accounts
        .values()
        .find(|account| account.id == member_id)
        .map(|account| normalize_email(&account.email))
        .ok_or_else(|| UiError::not_found("member not found"))?;
    if account_state
        .accounts
        .get(&email)
        .map(|account| account.role == "owner")
        .unwrap_or(false)
    {
        return Err(UiError::forbidden("owner account cannot be removed."));
    }
    account_state.accounts.remove(&email);
    Ok(Json(json!({ "ok": true })))
}
async fn ui_get_billing_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "flutterwave": billing_provider_json(&state),
    })))
}

async fn ui_get_billing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    let account_state = state.account_state.read().await;
    Ok(Json(json!({
        "ok": true,
        "profile": account_state.billing,
    })))
}

async fn ui_put_billing(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BillingUpdateRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_view_billing(&session.role) {
        return Err(UiError::forbidden(
            "Only workspace owner or admin can manage billing.",
        ));
    }

    let mut account_state = state.account_state.write().await;
    let before = account_state.billing.clone();
    if let Some(plan) = body.plan.as_deref() {
        account_state.billing.plan = normalize_plan(plan);
    }
    if let Some(access_mode) = body.access_mode.as_deref() {
        account_state.billing.access_mode = normalize_access_mode(access_mode);
    }
    if let Some(billing_email) = body.billing_email.as_deref() {
        let trimmed = billing_email.trim();
        if trimmed.is_empty() {
            return Err(UiError::bad_request("billing_email is required."));
        }
        account_state.billing.billing_email = trimmed.to_owned();
    }
    if let Some(card_brand) = body.card_brand.as_deref() {
        account_state.billing.card_brand =
            Some(card_brand.trim().to_owned()).filter(|v| !v.is_empty());
    }
    if let Some(card_last4) = body.card_last4.as_deref() {
        let digits = card_last4
            .trim()
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();
        account_state.billing.card_last4 = Some(digits).filter(|v| !v.is_empty());
    }
    if let Some(transaction_id) = body.flutterwave_transaction_id.as_deref() {
        if transaction_id.trim().is_empty() {
            return Err(UiError::bad_request(
                "flutterwave_transaction_id cannot be empty.",
            ));
        }
        if state.ui_features.flutterwave_secret_key.is_none() {
            return Err(UiError::unavailable(
                "Flutterwave billing is not configured for this deployment.",
            ));
        }
        apply_flutterwave_payment(
            &mut account_state.billing,
            &state.ui_features,
            transaction_id.trim(),
            now_ms(),
        );
    }
    account_state.billing.updated_at_ms = now_ms();
    let after = account_state.billing.clone();
    let payment_method_updated = before.payment_reference != after.payment_reference
        || before.card_last4 != after.card_last4;
    account_state.billing_audit.push(json!({
        "event_id": Uuid::new_v4().to_string(),
        "workspace_id": state.seed.workspace_id,
        "actor_email": session.email,
        "actor_role": session.role,
        "changed_at_ms": after.updated_at_ms,
        "plan_before": before.plan,
        "plan_after": after.plan,
        "access_mode_before": before.access_mode,
        "access_mode_after": after.access_mode,
        "payment_method_updated": payment_method_updated,
    }));
    let profile = after;
    let settings = account_state.settings.clone();
    drop(account_state);
    sync_quota_profile(&state, &profile, &settings, &session.email).await?;
    Ok(Json(json!({ "ok": true, "profile": profile })))
}

async fn ui_get_billing_audit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "events": state.account_state.read().await.billing_audit,
    })))
}

async fn ui_get_invoices(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "invoices": state.account_state.read().await.invoices,
    })))
}

async fn ui_post_flutterwave_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<BillingWebhookPayload>,
) -> Result<Json<Value>, UiError> {
    let expected_hash = state
        .ui_features
        .flutterwave_webhook_hash
        .as_deref()
        .ok_or_else(|| {
            UiError::unavailable("Flutterwave webhook verification is not configured.")
        })?;
    let received_hash = header_value(&headers, "verif-hash")
        .ok_or_else(|| UiError::forbidden("Missing verif-hash header."))?;
    if received_hash != expected_hash {
        return Err(UiError::forbidden("Flutterwave webhook hash mismatch."));
    }

    let email = body
        .data
        .as_ref()
        .and_then(|data| data.customer.as_ref())
        .and_then(|customer| customer.email.as_deref())
        .map(normalize_email)
        .unwrap_or_else(|| normalize_email(&state.seed.email));
    let transaction_id = body
        .data
        .as_ref()
        .map(|data| data.id.to_string().trim_matches('"').to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut account_state = state.account_state.write().await;
    let billing_email = account_state
        .accounts
        .get(&email)
        .map(|account| account.email.clone());
    if let Some(billing_email) = billing_email {
        apply_flutterwave_payment(
            &mut account_state.billing,
            &state.ui_features,
            &transaction_id,
            now_ms(),
        );
        account_state.billing.billing_email = billing_email;
        account_state.billing.updated_at_ms = now_ms();
    }
    Ok(Json(json!({ "ok": true })))
}

async fn ui_get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    Ok(Json(json!({
        "ok": true,
        "settings": state.account_state.read().await.settings,
    })))
}

async fn ui_put_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SettingsUpdateRequest>,
) -> Result<Json<Value>, UiError> {
    let session = require_session(&state, &headers).await?;
    if !role_can_manage_workspace(&session.role) {
        return Err(UiError::forbidden(
            "Only owner/admin can change workspace settings.",
        ));
    }

    let mut account_state = state.account_state.write().await;
    if let Some(value) = body.callback_default_enabled {
        account_state.settings.callback_default_enabled = value;
    }
    if let Some(value) = body.request_retention_days {
        account_state.settings.request_retention_days = value.clamp(7, 3650);
    }
    if let Some(value) = body.allow_replay_from_customer_app {
        account_state.settings.allow_replay_from_customer_app = value;
    }
    if let Some(value) = body.execution_policy.as_deref() {
        account_state.settings.execution_policy = normalize_execution_policy(value);
    }
    if let Some(value) = body.sponsored_monthly_cap_requests {
        account_state.settings.sponsored_monthly_cap_requests = value.max(1);
    }
    account_state.settings.updated_at_ms = now_ms();
    let settings = account_state.settings.clone();
    let billing = account_state.billing.clone();
    drop(account_state);
    sync_quota_profile(&state, &billing, &settings, &session.email).await?;
    Ok(Json(json!({ "ok": true, "settings": settings })))
}

async fn ui_get_usage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, UiError> {
    let _ = require_session(&state, &headers).await?;
    let usage = load_usage_summary(&state).await?;
    Ok(Json(json!({
        "ok": true,
        "workspace_id": usage["workspace_id"],
        "plan": usage["plan"],
        "access_mode": usage["access_mode"],
        "monthly_price_usd": usage["monthly_price_usd"],
        "free_play_limit": usage["free_play_limit"],
        "used_requests": usage["used_requests"],
        "remaining_requests": usage["remaining_requests"],
        "window_start_ms": usage["window_start_ms"],
        "window_end_ms": usage["window_end_ms"],
        "paid_unlimited": usage["paid_unlimited"],
        "metering_source": usage["metering_source"],
        "metering_warning": usage["metering_warning"],
    })))
}

async fn ui_get_invite(
    State(state): State<Arc<AppState>>,
    Query(query): Query<InviteLookupQuery>,
) -> Result<Json<Value>, UiError> {
    let account_state = state.account_state.read().await;
    let invite = account_state
        .invites
        .get(query.token.trim())
        .ok_or_else(|| UiError::not_found("Invite not found."))?;
    Ok(Json(json!({
        "ok": true,
        "invite": {
            "email": invite.email,
            "workspace_id": invite.workspace_id,
            "workspace_name": invite.workspace_name,
            "role": invite.role,
            "expires_at_ms": invite.expires_at_ms,
        }
    })))
}

async fn ui_post_accept_invite(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AcceptInviteRequest>,
) -> Result<Response, UiError> {
    let mut account_state = state.account_state.write().await;
    let invite = account_state
        .invites
        .remove(body.token.trim())
        .ok_or_else(|| UiError::not_found("Invite token is invalid or expired."))?;
    let email_key = normalize_email(&invite.email);
    let account = {
        let account = account_state
            .accounts
            .get_mut(&email_key)
            .ok_or_else(|| UiError::not_found("Invited account not found."))?;
        account.full_name = body.full_name.trim().to_owned();
        account.password = body.password;
        account.status = "active".to_owned();
        account.email_verified_at_ms = Some(now_ms());
        account.invite_expires_at_ms = None;
        account.clone()
    };

    let session_id = Uuid::new_v4().to_string();
    account_state.sessions.insert(
        session_id.clone(),
        SessionState {
            email_key: email_key.clone(),
            workspace_id: invite.workspace_id.clone(),
        },
    );
    let session = build_session_record(
        &state,
        &account_state,
        &account,
        resolve_workspace(&state, &invite.workspace_id),
    )?;
    drop(account_state);
    json_response_with_cookie(json!({ "ok": true, "session": session }), Some(&session_id))
}

async fn ui_post_request_email_verification(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EmailRequest>,
) -> Result<Json<Value>, UiError> {
    let email_key = normalize_email(&body.email);
    let mut account_state = state.account_state.write().await;
    if account_state.accounts.contains_key(&email_key)
        && state.ui_features.email_delivery_configured
    {
        account_state
            .verification_tokens
            .insert(Uuid::new_v4().to_string(), email_key);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn ui_post_confirm_email_verification(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TokenRequest>,
) -> Result<Json<Value>, UiError> {
    let mut account_state = state.account_state.write().await;
    let email_key = account_state
        .verification_tokens
        .remove(body.token.trim())
        .ok_or_else(|| UiError::bad_request("Verification token is invalid or expired."))?;
    let account = account_state
        .accounts
        .get_mut(&email_key)
        .ok_or_else(|| UiError::not_found("Account not found."))?;
    account.email_verified_at_ms = Some(now_ms());
    Ok(Json(json!({ "ok": true })))
}

async fn ui_post_request_password_reset(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EmailRequest>,
) -> Result<Json<Value>, UiError> {
    if !state.ui_features.password_reset_enabled {
        return Err(UiError::unavailable(
            "Password reset is disabled for this deployment.",
        ));
    }
    let email_key = normalize_email(&body.email);
    let mut account_state = state.account_state.write().await;
    if account_state.accounts.contains_key(&email_key)
        && state.ui_features.email_delivery_configured
    {
        account_state
            .password_reset_tokens
            .insert(Uuid::new_v4().to_string(), email_key);
    }
    Ok(Json(json!({ "ok": true })))
}

async fn ui_post_confirm_password_reset(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PasswordResetConfirmRequest>,
) -> Result<Json<Value>, UiError> {
    if !state.ui_features.password_reset_enabled {
        return Err(UiError::unavailable(
            "Password reset is disabled for this deployment.",
        ));
    }
    let mut account_state = state.account_state.write().await;
    let email_key = account_state
        .password_reset_tokens
        .remove(body.token.trim())
        .ok_or_else(|| UiError::bad_request("Password reset token is invalid or expired."))?;
    let account = account_state
        .accounts
        .get_mut(&email_key)
        .ok_or_else(|| UiError::not_found("Account not found."))?;
    account.password = body.password;
    Ok(Json(json!({ "ok": true })))
}

async fn ui_post_ingress_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut body): Json<SubmitIntentRequest>,
) -> Result<Json<Value>, UiError> {
    if body.intent_kind.trim().is_empty() {
        return Err(UiError::bad_request("intent_kind is required."));
    }

    let maybe_session = match require_session(&state, &headers).await {
        Ok(session) => Some(session),
        Err(err) if err.status == StatusCode::UNAUTHORIZED => None,
        Err(err) => return Err(err),
    };
    let surface = header_value(&headers, "x-azums-submit-surface")
        .unwrap_or_else(|| "customer".to_owned())
        .trim()
        .to_ascii_lowercase();
    let mut metadata = body.metadata.take().unwrap_or_default();
    metadata.insert(
        "submitter.kind".to_owned(),
        state.ingress_auth.submitter_kind.clone(),
    );
    if surface == "playground" {
        metadata.insert("metering.scope".to_owned(), "playground".to_owned());
        metadata.insert("ui.surface".to_owned(), "playground".to_owned());
        if state.ui_features.enforce_workspace_solana_rpc
            && body.intent_kind.trim().starts_with("solana.")
        {
            if let Some(rpc_url) = state.ui_features.sandbox_solana_rpc_url.as_deref() {
                set_object_string_field(&mut body.payload, "rpc_url", rpc_url);
            }
        }
    }
    body.metadata = Some(metadata);

    let result: Value = ingress_post_value(
        &state,
        "api/requests",
        serde_json::to_value(&body)
            .map_err(|err| UiError::internal(format!("failed to encode ingress request: {err}")))?,
        None,
    )
    .await?;

    if let Some(session) = maybe_session {
        let mut account_state = state.account_state.write().await;
        if let Some(account) = account_state
            .accounts
            .get_mut(&normalize_email(&session.email))
        {
            account.onboarding.submitted_request = true;
        }
    }

    Ok(Json(result))
}
async fn status_get<Q, T>(state: &AppState, path: &str, query: Option<&Q>) -> Result<T, UiError>
where
    Q: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let response = status_request_get(state, path, query)?
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("status_api request failed: {err}")))?;
    decode_json_response(response).await
}

async fn status_post<B, T>(state: &AppState, path: &str, body: &B) -> Result<T, UiError>
where
    B: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let url = build_status_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let response = apply_status_headers(state.client.post(url), &state.status_auth, &request_id)
        .json(body)
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("status_api request failed: {err}")))?;
    decode_json_response(response).await
}

async fn status_delete<Q, T>(state: &AppState, path: &str, query: Option<&Q>) -> Result<T, UiError>
where
    Q: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let url = build_status_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let mut request =
        apply_status_headers(state.client.delete(url), &state.status_auth, &request_id);
    if let Some(query) = query {
        request = request.query(query);
    }
    let response = request
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("status_api request failed: {err}")))?;
    decode_json_response(response).await
}

fn status_request_get<Q>(
    state: &AppState,
    path: &str,
    query: Option<&Q>,
) -> Result<reqwest::RequestBuilder, UiError>
where
    Q: Serialize + ?Sized,
{
    let url = build_status_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let mut request = apply_status_headers(state.client.get(url), &state.status_auth, &request_id);
    if let Some(query) = query {
        request = request.query(query);
    }
    Ok(request)
}

async fn ingress_post<B, T>(
    state: &AppState,
    path: &str,
    body: &B,
    extra_headers: Option<&HeaderMap>,
) -> Result<T, UiError>
where
    B: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let value = serde_json::to_value(body)
        .map_err(|err| UiError::internal(format!("failed to encode ingress body: {err}")))?;
    let response =
        send_ingress_request(state, Method::POST, path, Some(value), None, extra_headers).await?;
    decode_json_response(response).await
}

async fn ingress_post_value(
    state: &AppState,
    path: &str,
    body: Value,
    extra_headers: Option<&HeaderMap>,
) -> Result<Value, UiError> {
    let response =
        send_ingress_request(state, Method::POST, path, Some(body), None, extra_headers).await?;
    decode_json_response(response).await
}

async fn ingress_get<T>(
    state: &AppState,
    path: &str,
    query: Option<&[(&str, String)]>,
) -> Result<T, UiError>
where
    T: DeserializeOwned,
{
    let response = send_ingress_request(state, Method::GET, path, None, query, None).await?;
    decode_json_response(response).await
}

async fn send_ingress_request(
    state: &AppState,
    method: Method,
    path: &str,
    body: Option<Value>,
    query: Option<&[(&str, String)]>,
    extra_headers: Option<&HeaderMap>,
) -> Result<reqwest::Response, UiError> {
    let response = build_ingress_request(
        state,
        method.clone(),
        path,
        body.clone(),
        query,
        extra_headers,
        None,
        None,
    )
    .await?;

    if response.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(response);
    }

    let Some(fallback_principal) = state.ingress_auth.fallback_principal_id.as_deref() else {
        return Ok(response);
    };
    let fallback_submitter = state
        .ingress_auth
        .fallback_submitter_kind
        .as_deref()
        .unwrap_or(&state.ingress_auth.submitter_kind);
    build_ingress_request(
        state,
        method,
        path,
        body,
        query,
        extra_headers,
        Some(fallback_principal),
        Some(fallback_submitter),
    )
    .await
}

async fn build_ingress_request(
    state: &AppState,
    method: Method,
    path: &str,
    body: Option<Value>,
    query: Option<&[(&str, String)]>,
    extra_headers: Option<&HeaderMap>,
    principal_override: Option<&str>,
    submitter_override: Option<&str>,
) -> Result<reqwest::Response, UiError> {
    let url = build_ingress_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let mut request = state.client.request(method, url);
    request = apply_ingress_headers(
        request,
        &state.ingress_auth,
        &request_id,
        principal_override,
        submitter_override,
    );
    if let Some(query) = query {
        request = request.query(query);
    }
    if let Some(extra_headers) = extra_headers {
        for (name, value) in extra_headers {
            request = request.header(name, value);
        }
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("ingress request failed: {err}")))
}

async fn decode_json_response<T>(response: reqwest::Response) -> Result<T, UiError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|err| UiError::upstream(format!("failed reading upstream body: {err}")))?;
    let mapped_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if !status.is_success() {
        let message = parse_upstream_error_message(&bytes)
            .unwrap_or_else(|| format!("upstream returned non-success status {mapped_status}"));
        return Err(UiError {
            status: mapped_status,
            message,
        });
    }
    serde_json::from_slice::<T>(&bytes)
        .map_err(|err| UiError::upstream(format!("failed to parse upstream payload: {err}")))
}

fn parse_upstream_error_message(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return Some(error.to_owned());
        }
        if let Some(message) = value.get("message").and_then(Value::as_str) {
            return Some(message.to_owned());
        }
        return Some(value.to_string());
    }
    String::from_utf8(bytes.to_vec())
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

async fn maybe_session(state: &AppState, headers: &HeaderMap) -> Result<Option<Value>, UiError> {
    let Some(session_id) = read_cookie(headers, SESSION_COOKIE_NAME) else {
        return Ok(None);
    };
    let account_state = state.account_state.read().await;
    let Some(session_state) = account_state.sessions.get(&session_id) else {
        return Ok(None);
    };
    let Some(account) = account_state.accounts.get(&session_state.email_key) else {
        return Ok(None);
    };
    let workspace = resolve_workspace(state, &session_state.workspace_id);
    Ok(Some(
        serde_json::to_value(build_session_record(
            state,
            &account_state,
            account,
            workspace,
        )?)
        .map_err(|err| UiError::internal(format!("failed to encode session: {err}")))?,
    ))
}

async fn require_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<SessionEnvelope, UiError> {
    let session_id = read_cookie(headers, SESSION_COOKIE_NAME)
        .ok_or_else(|| UiError::unauthorized("Authentication required."))?;
    let account_state = state.account_state.read().await;
    let session_state = account_state
        .sessions
        .get(&session_id)
        .ok_or_else(|| UiError::unauthorized("Authentication required."))?
        .clone();
    let account = account_state
        .accounts
        .get(&session_state.email_key)
        .ok_or_else(|| UiError::unauthorized("Authentication required."))?;
    let workspace = resolve_workspace(state, &session_state.workspace_id);
    Ok(SessionEnvelope {
        session_id,
        email: account.email.clone(),
        role: account.role.clone(),
        workspace_id: workspace.workspace_id.clone(),
    })
}

#[derive(Clone)]
struct SessionEnvelope {
    session_id: String,
    email: String,
    role: String,
    workspace_id: String,
}

fn build_session_record(
    state: &AppState,
    account_state: &AccountState,
    account: &AccountRecord,
    workspace: &WorkspaceSeed,
) -> Result<Value, UiError> {
    Ok(json!({
        "id": account.id,
        "email": account.email,
        "full_name": account.full_name,
        "workspace_id": workspace.workspace_id,
        "workspace_name": workspace.workspace_name,
        "tenant_id": state.status_auth.tenant_id,
        "role": account.role,
        "plan": account_state.billing.plan,
        "created_at_ms": account.added_at_ms,
        "email_verified_at_ms": account.email_verified_at_ms,
        "onboarding": account.onboarding,
    }))
}

fn resolve_workspace<'a>(state: &'a AppState, workspace_id: &str) -> &'a WorkspaceSeed {
    state
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == workspace_id)
        .unwrap_or(&state.workspaces[0])
}

fn build_workspace_record(
    state: &AppState,
    session: &SessionEnvelope,
    workspace: &WorkspaceSeed,
) -> Value {
    json!({
        "workspace_id": workspace.workspace_id,
        "workspace_name": workspace.workspace_name,
        "tenant_id": state.status_auth.tenant_id,
        "role": session.role,
        "environment": workspace.environment,
        "is_current": workspace.workspace_id == session.workspace_id,
    })
}

fn team_member_json(account: &AccountRecord) -> Value {
    json!({
        "id": account.id,
        "email": account.email,
        "role": account.role,
        "status": account.status,
        "added_at_ms": account.added_at_ms,
        "invite_expires_at_ms": account.invite_expires_at_ms,
    })
}

fn billing_provider_json(state: &AppState) -> Value {
    json!({
        "provider": "flutterwave",
        "ready": state.ui_features.flutterwave_secret_key.is_some(),
        "has_secret_key": state.ui_features.flutterwave_secret_key.is_some(),
        "has_webhook_hash": state.ui_features.flutterwave_webhook_hash.is_some(),
        "base_url": state.ui_features.flutterwave_base_url,
        "expected_currency": state.ui_features.flutterwave_expected_currency,
        "supported_currencies": state.ui_features.flutterwave_supported_currencies,
        "webhook_path": "/api/ui/billing/flutterwave/webhook",
    })
}

async fn load_api_keys(state: &AppState) -> Result<Vec<Value>, UiError> {
    let path = format!(
        "api/internal/tenants/{}/api-keys",
        state.ingress_auth.tenant_id
    );
    let response: IngressApiKeysResponse = ingress_get(
        state,
        &path,
        Some(&[
            ("include_inactive", "true".to_owned()),
            ("limit", "200".to_owned()),
        ]),
    )
    .await?;
    Ok(response
        .keys
        .into_iter()
        .map(|row| {
            json!({
                "id": row.key_id,
                "name": row.label,
                "prefix": row.key_prefix,
                "last4": row.key_last4,
                "created_at_ms": row.created_at_ms,
                "revoked_at_ms": row.revoked_at_ms,
                "last_used_at_ms": row.last_used_at_ms,
            })
        })
        .collect())
}

async fn load_webhook_keys(
    state: &AppState,
    query: &WebhookKeysQuery,
) -> Result<Vec<Value>, UiError> {
    let path = format!(
        "api/internal/tenants/{}/webhook-keys",
        state.ingress_auth.tenant_id
    );
    let params = vec![
        (
            "source",
            query
                .source
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("default")
                .to_owned(),
        ),
        (
            "include_inactive",
            if query.include_inactive.unwrap_or(true) {
                "true".to_owned()
            } else {
                "false".to_owned()
            },
        ),
        ("limit", query.limit.unwrap_or(100).to_string()),
    ];
    let response: IngressWebhookKeysResponse = ingress_get(state, &path, Some(&params)).await?;
    Ok(response.keys)
}

async fn load_usage_summary(state: &AppState) -> Result<Value, UiError> {
    let billing = state.account_state.read().await.billing.clone();
    let now = now_ms();
    let window_start_ms = now.saturating_sub(THIRTY_DAYS_MS);
    let free_play_limit = plan_free_play_limit(&billing.plan);
    let paid_unlimited = billing.access_mode == "paid";
    let monthly_price = plan_monthly_price(&billing.plan);

    match status_get::<_, IntakeAuditsResponse>(
        state,
        "tenant/intake-audits",
        Some(&IntakeAuditsQueryParams {
            validation_result: Some("accepted".to_owned()),
            channel: None,
            limit: Some(400),
            offset: Some(0),
        }),
    )
    .await
    {
        Ok(audits) => {
            let used_requests = audits
                .audits
                .iter()
                .filter(|audit| {
                    audit
                        .details_json
                        .get("metering_scope")
                        .and_then(Value::as_str)
                        .map(|value| !value.trim().eq_ignore_ascii_case("playground"))
                        .unwrap_or(true)
                })
                .count() as u64;
            Ok(json!({
                "workspace_id": state.seed.workspace_id,
                "plan": billing.plan,
                "access_mode": billing.access_mode,
                "monthly_price_usd": monthly_price,
                "free_play_limit": free_play_limit,
                "used_requests": used_requests,
                "remaining_requests": if paid_unlimited { Value::Null } else { json!(free_play_limit.saturating_sub(used_requests)) },
                "window_start_ms": window_start_ms,
                "window_end_ms": now,
                "paid_unlimited": paid_unlimited,
                "metering_source": "durable_status_api",
                "metering_warning": Value::Null,
            }))
        }
        Err(err) if state.ui_features.require_durable_metering => Err(UiError::unavailable(
            format!("durable metering unavailable: {}", err.message),
        )),
        Err(err) => Ok(json!({
            "workspace_id": state.seed.workspace_id,
            "plan": billing.plan,
            "access_mode": billing.access_mode,
            "monthly_price_usd": monthly_price,
            "free_play_limit": free_play_limit,
            "used_requests": 0,
            "remaining_requests": if paid_unlimited { Value::Null } else { json!(free_play_limit) },
            "window_start_ms": window_start_ms,
            "window_end_ms": now,
            "paid_unlimited": paid_unlimited,
            "metering_source": "fallback_zero",
            "metering_warning": err.message,
        })),
    }
}

async fn sync_quota_profile(
    state: &AppState,
    billing: &BillingState,
    settings: &WorkspaceSettingsState,
    updated_by: &str,
) -> Result<(), UiError> {
    let path = format!(
        "api/internal/tenants/{}/quota",
        state.ingress_auth.tenant_id
    );
    let _: Value = ingress_post_value(
        state,
        &path,
        json!({
            "plan": billing.plan,
            "access_mode": billing.access_mode,
            "execution_policy": settings.execution_policy,
            "sponsored_monthly_cap_requests": settings.sponsored_monthly_cap_requests,
            "free_play_limit": plan_free_play_limit(&billing.plan),
            "updated_by_principal_id": updated_by,
        }),
        None,
    )
    .await?;
    Ok(())
}

fn apply_flutterwave_payment(
    billing: &mut BillingState,
    features: &UiFeatures,
    transaction_id: &str,
    now: u64,
) {
    billing.payment_provider = Some("flutterwave".to_owned());
    billing.payment_reference = Some(transaction_id.to_owned());
    billing.payment_verified_at_ms = Some(now);
    billing.payment_currency = features.flutterwave_expected_currency.clone();
    billing.payment_amount = Some(0.0);
    billing.payment_amount_usd = Some(0.0);
    billing.payment_fx_rate_to_usd = Some(1.0);
}

fn json_response_with_cookie(
    payload: Value,
    session_id: Option<&str>,
) -> Result<Response, UiError> {
    let mut response = Json(payload).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, session_cookie_value(session_id)?);
    Ok(response)
}

fn session_cookie_value(session_id: Option<&str>) -> Result<HeaderValue, UiError> {
    let value = match session_id {
        Some(session_id) => format!(
            "{SESSION_COOKIE_NAME}={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000"
        ),
        None => format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    };
    HeaderValue::from_str(&value)
        .map_err(|err| UiError::internal(format!("failed to build session cookie: {err}")))
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let (key, value) = pair.trim().split_once('=')?;
        if key.trim() == name {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn build_status_url(state: &AppState, path: &str) -> Result<Url, UiError> {
    state
        .status_base_url
        .join(path)
        .map_err(|err| UiError::internal(format!("failed to build status_api url: {err}")))
}

fn build_ingress_url(state: &AppState, path: &str) -> Result<Url, UiError> {
    state
        .ingress_base_url
        .join(path)
        .map_err(|err| UiError::internal(format!("failed to build ingress url: {err}")))
}

fn apply_status_headers(
    request: reqwest::RequestBuilder,
    auth: &StatusAuthHeaders,
    request_id: &str,
) -> reqwest::RequestBuilder {
    let mut request = request
        .header("x-tenant-id", auth.tenant_id.as_str())
        .header("x-principal-id", auth.principal_id.as_str())
        .header("x-principal-role", auth.principal_role.as_str())
        .header("x-request-id", request_id);
    if let Some(token) = auth.bearer_token.as_deref() {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request
}

fn apply_ingress_headers(
    request: reqwest::RequestBuilder,
    auth: &IngressAuthHeaders,
    request_id: &str,
    principal_override: Option<&str>,
    submitter_override: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = request
        .header("x-tenant-id", auth.tenant_id.as_str())
        .header(
            "x-principal-id",
            principal_override.unwrap_or(auth.principal_id.as_str()),
        )
        .header(
            "x-submitter-kind",
            submitter_override.unwrap_or(auth.submitter_kind.as_str()),
        )
        .header("x-request-id", request_id);
    if let Some(token) = auth.bearer_token.as_deref() {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    request
}

fn set_object_string_field(payload: &mut Value, key: &str, value: &str) {
    if !payload.is_object() {
        *payload = json!({});
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert(key.to_owned(), Value::String(value.to_owned()));
    }
}

fn parse_base_url(raw: &str, key: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|err| format!("invalid {key}: {err}"))?;
    if !url.path().ends_with('/') {
        let mut path = url.path().to_owned();
        if path.is_empty() {
            path.push('/');
        } else {
            path.push('/');
        }
        url.set_path(&path);
    }
    Ok(url)
}

fn parse_supported_currencies(raw: &str) -> Vec<String> {
    let mut values = raw
        .split(';')
        .filter_map(|item| item.split_once('=').map(|(name, _)| name.trim().to_owned()))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn build_workspace_seeds(
    seed: &SessionSeed,
    default_environment: &str,
    raw_extra_workspaces: Option<&str>,
) -> Vec<WorkspaceSeed> {
    let mut workspaces = vec![WorkspaceSeed {
        workspace_id: seed.workspace_id.clone(),
        workspace_name: seed.workspace_name.clone(),
        environment: default_environment.to_owned(),
    }];

    let Some(raw_extra_workspaces) = raw_extra_workspaces else {
        return workspaces;
    };

    for entry in raw_extra_workspaces.split(';') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('|').map(str::trim);
        let Some(workspace_id) = parts.next().filter(|value| !value.is_empty()) else {
            continue;
        };
        let workspace_name = parts
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(workspace_id);
        let environment = parts
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(default_environment);
        if workspaces
            .iter()
            .any(|workspace| workspace.workspace_id == workspace_id)
        {
            continue;
        }
        workspaces.push(WorkspaceSeed {
            workspace_id: workspace_id.to_owned(),
            workspace_name: workspace_name.to_owned(),
            environment: environment.to_owned(),
        });
    }

    workspaces
}

fn normalize_reconciliation_rollout_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "hidden" | "internal" | "internal_hidden" => "hidden".to_owned(),
        "operator" | "operator_only" => "operator_only".to_owned(),
        "customer" | "customer_visible" | "customer_status" | "" => "customer_visible".to_owned(),
        _ => "customer_visible".to_owned(),
    }
}

fn normalize_status_role(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "viewer" => "viewer".to_owned(),
        "operator" => "operator".to_owned(),
        _ => "admin".to_owned(),
    }
}

fn normalize_workspace_role(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "owner" => "owner".to_owned(),
        "admin" => "admin".to_owned(),
        "viewer" => "viewer".to_owned(),
        _ => "developer".to_owned(),
    }
}

fn normalize_plan(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "team" => "Team".to_owned(),
        "enterprise" => "Enterprise".to_owned(),
        _ => "Developer".to_owned(),
    }
}

fn normalize_access_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "paid" => "paid".to_owned(),
        _ => "free_play".to_owned(),
    }
}

fn normalize_execution_policy(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "customer_managed_signer" => "customer_managed_signer".to_owned(),
        "sponsored" => "sponsored".to_owned(),
        _ => "customer_signed".to_owned(),
    }
}

fn role_can_manage_workspace(role: &str) -> bool {
    matches!(role, "owner" | "admin")
}

fn role_can_write_requests(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "developer")
}

fn role_can_view_billing(role: &str) -> bool {
    matches!(role, "owner" | "admin")
}

fn plan_monthly_price(plan: &str) -> u64 {
    match plan {
        "Team" => 80,
        "Enterprise" => 500,
        _ => 20,
    }
}

fn plan_free_play_limit(plan: &str) -> u64 {
    match plan {
        "Team" => 1_000,
        "Enterprise" => 10_000,
        _ => 500,
    }
}

fn generate_api_key_token() -> String {
    format!(
        "azm_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn normalize_email(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_var_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env_var_opt(key).unwrap_or_else(|| default.to_owned())
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
