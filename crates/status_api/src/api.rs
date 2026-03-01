use crate::auth::{RequestIdentity, StatusAuthConfig, StatusAuthorizer};
use crate::error::StatusApiError;
use crate::model::{
    CallbackDeliveryRecord, CallbackDestinationRecord, CallbackDestinationResponse,
    CallbackHistoryQuery, CallbackHistoryResponse, DeleteCallbackDestinationResponse,
    HistoryResponse, IntakeAuditsQuery, IntakeAuditsResponse, JobListResponse, JobsQuery,
    ReceiptResponse, ReplayRequest, ReplayResponse, RequestStatusResponse,
    UpsertCallbackDestinationRequest, UpsertCallbackDestinationResponse,
};
use crate::replay::ReplayGateway;
use crate::store::{
    normalize_state_filter, role_label, OperatorActionAuditEntry, PostgresStatusStore,
    QueryAuditEntry, StoredCallbackDestination, UpsertCallbackDestinationStoreInput,
};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use execution_core::{CoreError, IntentId, OperatorRole, ReplayCommand};
use serde_json::json;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use tracing::warn;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct StatusApiState {
    pub store: Arc<PostgresStatusStore>,
    pub authorizer: Arc<dyn StatusAuthorizer>,
    pub replay_gateway: Option<Arc<dyn ReplayGateway>>,
    pub auth: Arc<StatusAuthConfig>,
}

impl StatusApiState {
    pub fn new(
        store: Arc<PostgresStatusStore>,
        authorizer: Arc<dyn StatusAuthorizer>,
        replay_gateway: Option<Arc<dyn ReplayGateway>>,
        auth: Arc<StatusAuthConfig>,
    ) -> Self {
        Self {
            store,
            authorizer,
            replay_gateway,
            auth,
        }
    }
}

pub fn router(state: StatusApiState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/requests/:id", get(get_request))
        .route("/requests/:id/receipt", get(get_receipt))
        .route("/requests/:id/history", get(get_history))
        .route("/requests/:id/callbacks", get(get_callbacks))
        .route("/requests/:id/replay", post(post_replay))
        .route(
            "/tenant/callback-destination",
            get(get_callback_destination),
        )
        .route(
            "/tenant/callback-destination",
            post(upsert_callback_destination),
        )
        .route(
            "/tenant/callback-destination",
            axum::routing::delete(delete_callback_destination),
        )
        .route("/tenant/intake-audits", get(get_intake_audits))
        .route("/jobs", get(get_jobs))
        // Reverse proxy routes /status/* to the status upstream pool.
        // Expose the same API surface under /status to avoid path rewrite coupling.
        .route("/status/health", get(health))
        .route("/status/requests/:id", get(get_request))
        .route("/status/requests/:id/receipt", get(get_receipt))
        .route("/status/requests/:id/history", get(get_history))
        .route("/status/requests/:id/callbacks", get(get_callbacks))
        .route("/status/requests/:id/replay", post(post_replay))
        .route(
            "/status/tenant/callback-destination",
            get(get_callback_destination),
        )
        .route(
            "/status/tenant/callback-destination",
            post(upsert_callback_destination),
        )
        .route(
            "/status/tenant/callback-destination",
            axum::routing::delete(delete_callback_destination),
        )
        .route("/status/tenant/intake-audits", get(get_intake_audits))
        .route("/status/jobs", get(get_jobs))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn get_request(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<RequestStatusResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id",
        Some(intent_id.as_str()),
    )
    .await?;
    let mut response = state
        .store
        .load_request_status(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?
        .ok_or_else(|| StatusApiError::NotFound(format!("request `{intent_id}` not found")))?;
    redact_request_status(&state, &identity, &mut response);

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id",
        Some(intent_id),
        true,
        json!({ "result": "ok" }),
    )
    .await;

    Ok(Json(response))
}

async fn get_receipt(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<ReceiptResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/receipt",
        Some(intent_id.as_str()),
    )
    .await?;

    let entries = state
        .store
        .load_receipts(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/receipt",
        Some(intent_id.clone()),
        true,
        json!({ "entries": entries.len() }),
    )
    .await;

    Ok(Json(ReceiptResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id,
        entries,
    }))
}

async fn get_history(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<HistoryResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/history",
        Some(intent_id.as_str()),
    )
    .await?;

    let transitions = state
        .store
        .load_history(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/history",
        Some(intent_id.clone()),
        true,
        json!({ "transitions": transitions.len() }),
    )
    .await;

    Ok(Json(HistoryResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id,
        transitions,
    }))
}

async fn get_callbacks(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
    Query(query): Query<CallbackHistoryQuery>,
) -> Result<Json<CallbackHistoryResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/callbacks",
        Some(intent_id.as_str()),
    )
    .await?;

    let mut callbacks = state
        .store
        .load_callback_history(
            &identity.tenant_id,
            &IntentId::from(intent_id.clone()),
            query.include_attempts(),
            query.normalized_attempt_limit(),
        )
        .await?;
    redact_callback_history(&state, &identity, &mut callbacks);

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/callbacks",
        Some(intent_id.clone()),
        true,
        json!({ "callbacks": callbacks.len() }),
    )
    .await;

    Ok(Json(CallbackHistoryResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id,
        callbacks,
    }))
}

async fn get_jobs(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Query(query): Query<JobsQuery>,
) -> Result<Json<JobListResponse>, StatusApiError> {
    ensure_view_allowed(&state, &headers, &identity, "GET", "/jobs", None).await?;

    let state_filter = normalize_state_filter(query.state.clone())?;
    let limit = query.normalized_limit();
    let offset = query.normalized_offset();

    let jobs = state
        .store
        .list_jobs(&identity.tenant_id, state_filter.as_deref(), limit, offset)
        .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/jobs",
        None,
        true,
        json!({ "jobs": jobs.len(), "limit": limit, "offset": offset, "state_filter": state_filter }),
    )
    .await;

    Ok(Json(JobListResponse {
        tenant_id: identity.tenant_id.to_string(),
        jobs,
        limit,
        offset,
    }))
}

async fn get_intake_audits(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Query(query): Query<IntakeAuditsQuery>,
) -> Result<Json<IntakeAuditsResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/tenant/intake-audits",
        None,
    )
    .await?;

    let validation_result =
        normalize_intake_validation_result_filter(query.normalized_validation_result())?;
    let channel = normalize_intake_channel_filter(query.normalized_channel())?;
    let limit = query.normalized_limit();
    let offset = query.normalized_offset();

    let audits = state
        .store
        .load_intake_audits(
            &identity.tenant_id,
            validation_result.as_deref(),
            channel.as_deref(),
            limit,
            offset,
        )
        .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/tenant/intake-audits",
        None,
        true,
        json!({
            "audits": audits.len(),
            "limit": limit,
            "offset": offset,
            "validation_result": validation_result,
            "channel": channel
        }),
    )
    .await;

    Ok(Json(IntakeAuditsResponse {
        tenant_id: identity.tenant_id.to_string(),
        audits,
        limit,
        offset,
    }))
}

async fn post_replay(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
    Json(body): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "POST",
        "/requests/:id/replay",
        Some(intent_id.as_str()),
    )
    .await?;

    if !state
        .authorizer
        .can_replay(&identity.principal, &identity.tenant_id)
    {
        let reason = body
            .reason
            .unwrap_or_else(|| "replay denied by policy".to_owned());
        audit_operator_action(
            &state,
            &identity,
            "request_replay",
            &intent_id,
            false,
            &reason,
            None,
        )
        .await;
        return Err(StatusApiError::Forbidden(
            "principal is not authorized to request replay".to_owned(),
        ));
    }

    let replay_gateway = state.replay_gateway.as_ref().ok_or_else(|| {
        StatusApiError::Unavailable("replay gateway is not configured".to_owned())
    })?;

    let reason = body
        .reason
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "replay requested via status api".to_owned());

    let command = ReplayCommand {
        tenant_id: identity.tenant_id.clone(),
        intent_id: IntentId::from(intent_id.clone()),
        requested_by: identity.principal.clone(),
        reason: reason.clone(),
    };

    match replay_gateway.request_replay(command).await {
        Ok(replay) => {
            let result_json = json!({
                "source_job_id": replay.source_job_id.to_string(),
                "replay_job_id": replay.replay_job.job_id.to_string(),
                "state": format!("{:?}", replay.replay_job.state),
            });
            audit_operator_action(
                &state,
                &identity,
                "request_replay",
                &intent_id,
                true,
                &reason,
                Some(result_json.clone()),
            )
            .await;

            let mut details = BTreeMap::new();
            details.insert("reason".to_owned(), reason);
            details.insert(
                "result".to_owned(),
                "replay job created through execution core".to_owned(),
            );

            Ok(Json(ReplayResponse {
                source_job_id: replay.source_job_id.to_string(),
                replay_job_id: replay.replay_job.job_id.to_string(),
                replay_count: replay.replay_job.replay_count,
                state: replay.replay_job.state,
                route_adapter_id: replay.replay_job.adapter_id.to_string(),
                details,
            }))
        }
        Err(err) => {
            audit_operator_action(
                &state,
                &identity,
                "request_replay",
                &intent_id,
                false,
                &reason,
                Some(json!({ "error": err.to_string() })),
            )
            .await;
            Err(map_core_error(err))
        }
    }
}

async fn get_callback_destination(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
) -> Result<Json<CallbackDestinationResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/tenant/callback-destination",
        None,
    )
    .await?;

    let destination = state
        .store
        .load_callback_destination(&identity.tenant_id)
        .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/tenant/callback-destination",
        None,
        true,
        json!({ "configured": destination.is_some() }),
    )
    .await;

    Ok(Json(CallbackDestinationResponse {
        tenant_id: identity.tenant_id.to_string(),
        configured: destination.is_some(),
        destination: destination.map(map_callback_destination_record),
    }))
}

async fn upsert_callback_destination(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Json(body): Json<UpsertCallbackDestinationRequest>,
) -> Result<Json<UpsertCallbackDestinationResponse>, StatusApiError> {
    ensure_admin_allowed(
        &state,
        &headers,
        &identity,
        "POST",
        "/tenant/callback-destination",
    )
    .await?;

    let validated = validate_callback_destination_request(&body)?;
    let normalized_hosts = normalize_allowed_hosts_list(body.allowed_hosts);
    let input = UpsertCallbackDestinationStoreInput {
        tenant_id: identity.tenant_id.to_string(),
        delivery_url: validated.delivery_url,
        bearer_token: normalize_secret(body.bearer_token),
        signature_secret: normalize_secret(body.signature_secret),
        signature_key_id: normalize_secret(body.signature_key_id),
        timeout_ms: body.timeout_ms.unwrap_or(10_000).clamp(100, 120_000),
        allow_private_destinations: body.allow_private_destinations.unwrap_or(false),
        allowed_hosts: normalized_hosts.as_ref().map(|hosts| hosts.join(",")),
        enabled: body.enabled.unwrap_or(true),
        updated_by_principal_id: identity.principal.principal_id.clone(),
    };
    let stored = state.store.upsert_callback_destination(&input).await?;
    let record = map_callback_destination_record(stored);

    audit_operator_action(
        &state,
        &identity,
        "upsert_callback_destination",
        "__tenant_callback_destination__",
        true,
        "tenant callback destination upserted",
        Some(json!({
            "enabled": record.enabled,
            "delivery_url": record.delivery_url,
            "allow_private_destinations": record.allow_private_destinations,
        })),
    )
    .await;

    Ok(Json(UpsertCallbackDestinationResponse {
        tenant_id: identity.tenant_id.to_string(),
        updated: true,
        destination: record,
    }))
}

async fn delete_callback_destination(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
) -> Result<Json<DeleteCallbackDestinationResponse>, StatusApiError> {
    ensure_admin_allowed(
        &state,
        &headers,
        &identity,
        "DELETE",
        "/tenant/callback-destination",
    )
    .await?;

    let deleted = state
        .store
        .delete_callback_destination(&identity.tenant_id)
        .await?;

    audit_operator_action(
        &state,
        &identity,
        "delete_callback_destination",
        "__tenant_callback_destination__",
        true,
        "tenant callback destination deleted",
        Some(json!({ "deleted": deleted })),
    )
    .await;

    Ok(Json(DeleteCallbackDestinationResponse {
        tenant_id: identity.tenant_id.to_string(),
        deleted,
    }))
}

async fn ensure_view_allowed(
    state: &StatusApiState,
    headers: &HeaderMap,
    identity: &RequestIdentity,
    method: &str,
    endpoint: &str,
    resource_id: Option<&str>,
) -> Result<(), StatusApiError> {
    if let Err(err) = state.auth.authenticate(identity, headers) {
        audit_query(
            state,
            identity,
            method,
            endpoint,
            resource_id.map(ToOwned::to_owned),
            false,
            json!({ "error": err.to_string(), "stage": "auth" }),
        )
        .await;
        return Err(err);
    }

    if state
        .authorizer
        .can_view_tenant(&identity.principal, &identity.tenant_id)
    {
        return Ok(());
    }

    audit_query(
        state,
        identity,
        method,
        endpoint,
        resource_id.map(ToOwned::to_owned),
        false,
        json!({ "error": "forbidden" }),
    )
    .await;
    Err(StatusApiError::Forbidden(format!(
        "principal `{}` is not allowed to read tenant `{}`",
        identity.principal.principal_id, identity.tenant_id
    )))
}

async fn ensure_admin_allowed(
    state: &StatusApiState,
    headers: &HeaderMap,
    identity: &RequestIdentity,
    method: &str,
    endpoint: &str,
) -> Result<(), StatusApiError> {
    ensure_view_allowed(state, headers, identity, method, endpoint, None).await?;

    if matches!(identity.principal.role, OperatorRole::Admin) {
        return Ok(());
    }

    audit_operator_action(
        state,
        identity,
        "tenant_callback_destination_denied",
        "__tenant_callback_destination__",
        false,
        "principal role must be admin",
        Some(json!({
            "required_role": "admin",
            "actual_role": role_label(identity.principal.role),
        })),
    )
    .await;
    Err(StatusApiError::Forbidden(
        "principal is not authorized to manage tenant callback destination".to_owned(),
    ))
}

fn redact_request_status(
    state: &StatusApiState,
    identity: &RequestIdentity,
    response: &mut RequestStatusResponse,
) {
    if state
        .auth
        .should_redact_failure_provider_details(identity.principal.role)
    {
        if let Some(failure) = response.last_failure.as_mut() {
            failure.provider_details = None;
        }
    }
}

fn map_callback_destination_record(value: StoredCallbackDestination) -> CallbackDestinationRecord {
    CallbackDestinationRecord {
        delivery_url: value.delivery_url,
        timeout_ms: value.timeout_ms,
        allow_private_destinations: value.allow_private_destinations,
        allowed_hosts: parse_allowed_hosts_csv(value.allowed_hosts.as_deref()),
        enabled: value.enabled,
        has_bearer_token: value
            .bearer_token
            .as_ref()
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false),
        has_signature_secret: value
            .signature_secret
            .as_ref()
            .map(|token| !token.trim().is_empty())
            .unwrap_or(false),
        signature_key_id: value.signature_key_id,
        updated_by_principal_id: value.updated_by_principal_id,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

fn validate_callback_destination_request(
    request: &UpsertCallbackDestinationRequest,
) -> Result<ValidatedCallbackDestination, StatusApiError> {
    let delivery_url = request.delivery_url.trim();
    if delivery_url.is_empty() {
        return Err(StatusApiError::BadRequest(
            "delivery_url is required".to_owned(),
        ));
    }

    let parsed = Url::parse(delivery_url)
        .map_err(|err| StatusApiError::BadRequest(format!("invalid delivery_url: {err}")))?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(StatusApiError::BadRequest(
            "delivery_url must use http or https".to_owned(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| StatusApiError::BadRequest("delivery_url must include a host".to_owned()))?;
    let host_lower = host.to_ascii_lowercase();
    let allow_private_destinations = request.allow_private_destinations.unwrap_or(false);
    if !allow_private_destinations {
        if host_lower == "localhost" || host_lower.ends_with(".local") {
            return Err(StatusApiError::BadRequest(format!(
                "delivery_url host `{host}` is not allowed"
            )));
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_or_local_ip(ip) {
                return Err(StatusApiError::BadRequest(format!(
                    "delivery_url ip `{host}` is not allowed"
                )));
            }
        }
    }

    let normalized_hosts = normalize_allowed_hosts_list(request.allowed_hosts.clone());
    if let Some(allowed_hosts) = normalized_hosts.as_ref() {
        if !allowed_hosts.contains(&host_lower) {
            return Err(StatusApiError::BadRequest(format!(
                "delivery_url host `{host}` is not in allowed_hosts"
            )));
        }
    }

    let timeout_ms = request.timeout_ms.unwrap_or(10_000).clamp(100, 120_000);
    if timeout_ms == 0 {
        return Err(StatusApiError::BadRequest(
            "timeout_ms must be greater than zero".to_owned(),
        ));
    }

    Ok(ValidatedCallbackDestination {
        delivery_url: delivery_url.to_owned(),
    })
}

fn normalize_intake_validation_result_filter(
    value: Option<String>,
) -> Result<Option<String>, StatusApiError> {
    match value.as_deref() {
        None => Ok(None),
        Some("accepted") => Ok(Some("accepted".to_owned())),
        Some("rejected") => Ok(Some("rejected".to_owned())),
        Some(raw) => Err(StatusApiError::BadRequest(format!(
            "unsupported validation_result filter `{raw}`"
        ))),
    }
}

fn normalize_intake_channel_filter(
    value: Option<String>,
) -> Result<Option<String>, StatusApiError> {
    match value.as_deref() {
        None => Ok(None),
        Some("api") => Ok(Some("api".to_owned())),
        Some("webhook") => Ok(Some("webhook".to_owned())),
        Some(raw) => Err(StatusApiError::BadRequest(format!(
            "unsupported channel filter `{raw}`"
        ))),
    }
}

fn normalize_secret(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

fn normalize_allowed_hosts_list(raw: Option<Vec<String>>) -> Option<Vec<String>> {
    let Some(raw) = raw else {
        return None;
    };

    let mut out: Vec<String> = raw
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_allowed_hosts_csv(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };

    let mut out: Vec<String> = raw
        .split(|ch| ch == ',' || ch == ';' || ch == '|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    out.sort();
    out.dedup();
    out
}

fn is_private_or_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private()
                || ipv4.is_loopback()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_documentation()
                || ipv4.is_unspecified()
                || ipv4.is_multicast()
        }
        IpAddr::V6(ipv6) => {
            let seg0 = ipv6.segments()[0];
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                || ipv6.is_multicast()
                || is_unique_local
                || is_link_local
        }
    }
}

struct ValidatedCallbackDestination {
    delivery_url: String,
}

fn redact_callback_history(
    state: &StatusApiState,
    identity: &RequestIdentity,
    callbacks: &mut [CallbackDeliveryRecord],
) {
    if !state
        .auth
        .should_redact_callback_error_details(identity.principal.role)
    {
        return;
    }

    for callback in callbacks.iter_mut() {
        callback.last_error_message = None;
        for attempt in &mut callback.attempt_history {
            attempt.error_message = None;
            attempt.response_excerpt = None;
        }
    }
}

async fn audit_query(
    state: &StatusApiState,
    identity: &RequestIdentity,
    method: &str,
    endpoint: &str,
    resource_id: Option<String>,
    allowed: bool,
    details_json: serde_json::Value,
) {
    let entry = QueryAuditEntry {
        audit_id: Uuid::new_v4(),
        tenant_id: identity.tenant_id.to_string(),
        principal_id: identity.principal.principal_id.clone(),
        principal_role: role_label(identity.principal.role).to_owned(),
        method: method.to_owned(),
        endpoint: endpoint.to_owned(),
        resource_id,
        request_id: identity.request_id.clone(),
        allowed,
        details_json,
    };

    if let Err(err) = state.store.record_query_audit(&entry).await {
        warn!(error = %err, "failed to persist status_api query audit row");
    }
}

async fn audit_operator_action(
    state: &StatusApiState,
    identity: &RequestIdentity,
    action_type: &str,
    target_intent_id: &str,
    allowed: bool,
    reason: &str,
    result_json: Option<serde_json::Value>,
) {
    let entry = OperatorActionAuditEntry {
        action_id: Uuid::new_v4(),
        tenant_id: identity.tenant_id.to_string(),
        principal_id: identity.principal.principal_id.clone(),
        principal_role: role_label(identity.principal.role).to_owned(),
        action_type: action_type.to_owned(),
        target_intent_id: target_intent_id.to_owned(),
        allowed,
        reason: reason.to_owned(),
        result_json,
    };
    if let Err(err) = state.store.record_operator_action(&entry).await {
        warn!(
            error = %err,
            "failed to persist status_api operator action audit row"
        );
    }
}

fn map_core_error(err: CoreError) -> StatusApiError {
    match err {
        CoreError::UnsupportedIntent(kind) => {
            StatusApiError::BadRequest(format!("unsupported intent kind `{kind}`"))
        }
        CoreError::AdapterRoutingDenied {
            tenant_id,
            adapter_id,
        } => StatusApiError::Forbidden(format!(
            "adapter routing denied for tenant `{tenant_id}` and adapter `{adapter_id}`"
        )),
        CoreError::IllegalTransition { from, to } => {
            StatusApiError::Conflict(format!("illegal transition from {from:?} to {to:?}"))
        }
        CoreError::JobNotFound(job_id) => {
            StatusApiError::NotFound(format!("job `{job_id}` not found"))
        }
        CoreError::IntentNotFound(intent_id) => {
            StatusApiError::NotFound(format!("intent `{intent_id}` not found"))
        }
        CoreError::TenantMismatch {
            job_id,
            expected,
            actual,
        } => StatusApiError::Forbidden(format!(
            "tenant mismatch on job `{job_id}` expected `{expected}` got `{actual}`"
        )),
        CoreError::UnauthorizedReplay { principal_id } => {
            StatusApiError::Forbidden(format!("replay forbidden for principal `{principal_id}`"))
        }
        CoreError::IdempotencyConflict { key, reason } => {
            StatusApiError::Conflict(format!("idempotency conflict for key `{key}`: {reason}"))
        }
        CoreError::ReplayDenied { reason } => StatusApiError::Conflict(reason),
        CoreError::UnauthorizedManualAction { principal_id } => StatusApiError::Forbidden(format!(
            "manual action forbidden for principal `{principal_id}`"
        )),
        CoreError::Store(err) => StatusApiError::Internal(format!("store error: {err}")),
        CoreError::Routing(err) => StatusApiError::Internal(format!("routing error: {err}")),
        CoreError::AdapterExecution(err) => {
            StatusApiError::Internal(format!("adapter execution error: {err}"))
        }
        CoreError::Callback(err) => StatusApiError::Internal(format!("callback error: {err}")),
    }
}
