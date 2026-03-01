use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared_types::status_api::{
    CallbackDestinationResponse, CallbackHistoryResponse, DeleteCallbackDestinationResponse,
    HistoryResponse, IntakeAuditsResponse, JobListResponse, ReceiptResponse, ReplayRequest,
    ReplayResponse, RequestStatusResponse, UpsertCallbackDestinationRequest,
    UpsertCallbackDestinationResponse,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    client: Client,
    status_base_url: Url,
    auth_headers: AuthHeaders,
}

#[derive(Clone)]
struct AuthHeaders {
    bearer_token: Option<String>,
    tenant_id: String,
    principal_id: String,
    principal_role: String,
}

#[derive(Debug)]
struct UiError {
    status: StatusCode,
    message: String,
}

impl UiError {
    fn upstream(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
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

#[derive(Debug, Serialize)]
struct UiConfigResponse {
    ok: bool,
    status_base_url: String,
    tenant_id: String,
    principal_id: String,
    principal_role: String,
    has_bearer_token: bool,
}

#[derive(Debug, Serialize)]
struct UiHealthResponse {
    ok: bool,
    status_api_reachable: bool,
    status_api_status_code: Option<u16>,
    status_api_error: Option<String>,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = env_or("OPERATOR_UI_BIND", "0.0.0.0:8083");
    let timeout_ms = env_u64("OPERATOR_UI_STATUS_TIMEOUT_MS", 15_000);
    let status_base_url = parse_status_base_url(&env_or(
        "OPERATOR_UI_STATUS_BASE_URL",
        "http://127.0.0.1:8000/status",
    ))?;
    let auth_headers = AuthHeaders {
        bearer_token: env_var_opt("OPERATOR_UI_STATUS_BEARER_TOKEN"),
        tenant_id: env_or("OPERATOR_UI_TENANT_ID", "tenant_demo"),
        principal_id: env_or("OPERATOR_UI_PRINCIPAL_ID", "demo-operator"),
        principal_role: normalize_role(&env_or("OPERATOR_UI_PRINCIPAL_ROLE", "admin")),
    };

    let client = Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()?;

    let state = Arc::new(AppState {
        client,
        status_base_url,
        auth_headers,
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
        .route("/api/ui/status/requests/:id", get(ui_get_request))
        .route("/api/ui/status/requests/:id/receipt", get(ui_get_receipt))
        .route("/api/ui/status/requests/:id/history", get(ui_get_history))
        .route(
            "/api/ui/status/requests/:id/callbacks",
            get(ui_get_callbacks),
        )
        .route("/api/ui/status/requests/:id/replay", post(ui_post_replay))
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

async fn ui_config(State(state): State<Arc<AppState>>) -> Json<UiConfigResponse> {
    Json(UiConfigResponse {
        ok: true,
        status_base_url: state.status_base_url.to_string(),
        tenant_id: state.auth_headers.tenant_id.clone(),
        principal_id: state.auth_headers.principal_id.clone(),
        principal_role: state.auth_headers.principal_role.clone(),
        has_bearer_token: state.auth_headers.bearer_token.is_some(),
    })
}

async fn ui_health(State(state): State<Arc<AppState>>) -> Json<UiHealthResponse> {
    let request = match status_request_get(&state, "health", Option::<&JobsQueryParams>::None) {
        Ok(request) => request,
        Err(err) => {
            return Json(UiHealthResponse {
                ok: false,
                status_api_reachable: false,
                status_api_status_code: None,
                status_api_error: Some(err.message),
            });
        }
    };
    match request.send().await {
        Ok(response) => {
            let code = response.status().as_u16();
            Json(UiHealthResponse {
                ok: code < 500,
                status_api_reachable: true,
                status_api_status_code: Some(code),
                status_api_error: None,
            })
        }
        Err(err) => Json(UiHealthResponse {
            ok: false,
            status_api_reachable: false,
            status_api_status_code: None,
            status_api_error: Some(err.to_string()),
        }),
    }
}

async fn ui_get_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<JobsQueryParams>,
) -> Result<Json<JobListResponse>, UiError> {
    let result = status_get(&state, "jobs", Some(&query)).await?;
    Ok(Json(result))
}

async fn ui_get_request(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<RequestStatusResponse>, UiError> {
    let path = format!("requests/{intent_id}");
    let result = status_get(&state, &path, Option::<&JobsQueryParams>::None).await?;
    Ok(Json(result))
}

async fn ui_get_receipt(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<ReceiptResponse>, UiError> {
    let path = format!("requests/{intent_id}/receipt");
    let result = status_get(&state, &path, Option::<&JobsQueryParams>::None).await?;
    Ok(Json(result))
}

async fn ui_get_history(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
) -> Result<Json<HistoryResponse>, UiError> {
    let path = format!("requests/{intent_id}/history");
    let result = status_get(&state, &path, Option::<&JobsQueryParams>::None).await?;
    Ok(Json(result))
}

async fn ui_get_callbacks(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
    Query(query): Query<CallbackHistoryQueryParams>,
) -> Result<Json<CallbackHistoryResponse>, UiError> {
    let path = format!("requests/{intent_id}/callbacks");
    let result = status_get(&state, &path, Some(&query)).await?;
    Ok(Json(result))
}

async fn ui_post_replay(
    State(state): State<Arc<AppState>>,
    Path(intent_id): Path<String>,
    Json(body): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, UiError> {
    let path = format!("requests/{intent_id}/replay");
    let result = status_post(&state, &path, &body).await?;
    Ok(Json(result))
}

async fn ui_get_intake_audits(
    State(state): State<Arc<AppState>>,
    Query(query): Query<IntakeAuditsQueryParams>,
) -> Result<Json<IntakeAuditsResponse>, UiError> {
    let result = status_get(&state, "tenant/intake-audits", Some(&query)).await?;
    Ok(Json(result))
}

async fn ui_get_callback_destination(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CallbackDestinationResponse>, UiError> {
    let result = status_get(
        &state,
        "tenant/callback-destination",
        Option::<&JobsQueryParams>::None,
    )
    .await?;
    Ok(Json(result))
}

async fn ui_upsert_callback_destination(
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertCallbackDestinationRequest>,
) -> Result<Json<UpsertCallbackDestinationResponse>, UiError> {
    let result = status_post(&state, "tenant/callback-destination", &body).await?;
    Ok(Json(result))
}

async fn ui_delete_callback_destination(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DeleteCallbackDestinationResponse>, UiError> {
    let result = status_delete(
        &state,
        "tenant/callback-destination",
        Option::<&JobsQueryParams>::None,
    )
    .await?;
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
    decode_status_response(response).await
}

async fn status_post<B, T>(state: &AppState, path: &str, body: &B) -> Result<T, UiError>
where
    B: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let url = build_status_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let request = apply_proxy_headers(
        state.client.post(url),
        &state.auth_headers,
        Some(&request_id),
    )
    .json(body);
    let response = request
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("status_api request failed: {err}")))?;
    decode_status_response(response).await
}

async fn status_delete<Q, T>(state: &AppState, path: &str, query: Option<&Q>) -> Result<T, UiError>
where
    Q: Serialize + ?Sized,
    T: DeserializeOwned,
{
    let url = build_status_url(state, path)?;
    let request_id = Uuid::new_v4().to_string();
    let mut request = apply_proxy_headers(
        state.client.delete(url),
        &state.auth_headers,
        Some(&request_id),
    );
    if let Some(query) = query {
        request = request.query(query);
    }
    let response = request
        .send()
        .await
        .map_err(|err| UiError::upstream(format!("status_api request failed: {err}")))?;
    decode_status_response(response).await
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
    let mut request = apply_proxy_headers(
        state.client.get(url),
        &state.auth_headers,
        Some(&request_id),
    );
    if let Some(query) = query {
        request = request.query(query);
    }
    Ok(request)
}

async fn decode_status_response<T>(response: reqwest::Response) -> Result<T, UiError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let bytes = response.bytes().await.map_err(|err| {
        UiError::upstream(format!("failed reading status_api response body: {err}"))
    })?;
    let mapped_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    if !status.is_success() {
        let message = parse_upstream_error_message(&bytes)
            .unwrap_or_else(|| format!("status_api returned non-success status {mapped_status}"));
        return Err(UiError {
            status: mapped_status,
            message,
        });
    }

    serde_json::from_slice::<T>(&bytes).map_err(|err| {
        UiError::upstream(format!(
            "failed to parse typed status_api response payload: {err}"
        ))
    })
}

fn parse_upstream_error_message(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) {
        if let Some(error) = value.get("error").and_then(|value| value.as_str()) {
            return Some(error.to_owned());
        }
        if let Some(message) = value.get("message").and_then(|value| value.as_str()) {
            return Some(message.to_owned());
        }
        return Some(value.to_string());
    }

    String::from_utf8(bytes.to_vec())
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn build_status_url(state: &AppState, path: &str) -> Result<Url, UiError> {
    state
        .status_base_url
        .join(path)
        .map_err(|err| UiError::internal(format!("failed to build status_api url: {err}")))
}

fn apply_proxy_headers(
    request: reqwest::RequestBuilder,
    auth: &AuthHeaders,
    request_id: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = request
        .header("x-tenant-id", auth.tenant_id.as_str())
        .header("x-principal-id", auth.principal_id.as_str())
        .header("x-principal-role", auth.principal_role.as_str());

    if let Some(token) = auth.bearer_token.as_deref() {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(request_id) = request_id {
        request = request.header("x-request-id", request_id);
    }
    request
}

fn parse_status_base_url(raw: &str) -> Result<Url, String> {
    let mut url =
        Url::parse(raw).map_err(|err| format!("invalid OPERATOR_UI_STATUS_BASE_URL: {err}"))?;
    if !url.path().ends_with('/') {
        let mut normalized = url.path().to_owned();
        normalized.push('/');
        url.set_path(&normalized);
    }
    Ok(url)
}

fn normalize_role(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "viewer" => "viewer".to_owned(),
        "operator" => "operator".to_owned(),
        "admin" => "admin".to_owned(),
        _ => "viewer".to_owned(),
    }
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

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}
