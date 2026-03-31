use crate::auth::{RequestIdentity, StatusAuthConfig, StatusAuthorizer};
use crate::error::StatusApiError;
use crate::model::{
    CallbackDeliveryRecord, CallbackDestinationRecord, CallbackDestinationResponse,
    CallbackDetailResponse, CallbackHistoryQuery, CallbackHistoryResponse,
    DeleteCallbackDestinationResponse, ExceptionActionResponse, ExceptionCaseRecord,
    ExceptionDetailResponse, ExceptionEventRecord, ExceptionEvidenceRecord, ExceptionIndexQuery,
    ExceptionIndexResponse, ExceptionResolutionRecord, ExceptionStateTransitionRequest,
    ExceptionStateTransitionResponse, HistoryResponse, IntakeAuditsQuery, IntakeAuditsResponse,
    JobListResponse, JobsQuery, OperatorActionRequest, ReceiptLookupResponse, ReceiptResponse,
    ReconActionResponse, ReconciliationFactRecord, ReconciliationReceiptRecord,
    ReconciliationRolloutSummaryResponse, ReconciliationRunRecord, ReconciliationSubjectRecord,
    ReplayRequest, ReplayResponse, ReplayReviewResponse, RequestExceptionsResponse,
    RequestReconciliationResponse, RequestStatusResponse, RolloutSummaryQuery,
    UnifiedEvidenceReferenceRecord, UnifiedExceptionSummary, UnifiedRequestStatusResponse,
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
use exception_intelligence::{ExceptionSearchQuery, ExceptionState, PostgresExceptionStore};
use execution_core::{CoreError, IntentId, OperatorRole, ReceiptEntry, ReplayCommand, TenantId};
use recon_core::{PostgresReconStore, ReconOperatorActionType};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct StatusApiState {
    pub store: Arc<PostgresStatusStore>,
    pub recon_store: Arc<PostgresReconStore>,
    pub exception_store: Arc<PostgresExceptionStore>,
    pub authorizer: Arc<dyn StatusAuthorizer>,
    pub replay_gateway: Option<Arc<dyn ReplayGateway>>,
    pub auth: Arc<StatusAuthConfig>,
}

impl StatusApiState {
    pub fn new(
        store: Arc<PostgresStatusStore>,
        recon_store: Arc<PostgresReconStore>,
        exception_store: Arc<PostgresExceptionStore>,
        authorizer: Arc<dyn StatusAuthorizer>,
        replay_gateway: Option<Arc<dyn ReplayGateway>>,
        auth: Arc<StatusAuthConfig>,
    ) -> Self {
        Self {
            store,
            recon_store,
            exception_store,
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
        .route("/receipts/:receipt_id", get(get_receipt_by_id))
        .route("/requests/:id/receipt", get(get_receipt))
        .route("/requests/:id/history", get(get_history))
        .route("/requests/:id/callbacks", get(get_callbacks))
        .route("/requests/:id/unified", get(get_unified_request))
        .route("/requests/:id/reconciliation", get(get_reconciliation))
        .route(
            "/reconciliation/rollout-summary",
            get(get_reconciliation_rollout_summary),
        )
        .route(
            "/requests/:id/reconciliation/rerun",
            post(post_reconciliation_rerun),
        )
        .route(
            "/requests/:id/reconciliation/refresh-observation",
            post(post_refresh_observation),
        )
        .route("/requests/:id/exceptions", get(get_exceptions))
        .route("/exceptions", get(list_exceptions))
        .route("/exceptions/:case_id", get(get_exception_detail))
        .route("/exceptions/:case_id/state", post(post_exception_state))
        .route(
            "/exceptions/:case_id/acknowledge",
            post(post_exception_acknowledge),
        )
        .route("/exceptions/:case_id/resolve", post(post_exception_resolve))
        .route(
            "/exceptions/:case_id/false-positive",
            post(post_exception_false_positive),
        )
        .route("/callbacks/:callback_id", get(get_callback_detail))
        .route("/requests/:id/replay", post(post_replay))
        .route("/requests/:id/replay-review", post(post_replay_review))
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
        .route("/status/receipts/:receipt_id", get(get_receipt_by_id))
        .route("/status/requests/:id/receipt", get(get_receipt))
        .route("/status/requests/:id/history", get(get_history))
        .route("/status/requests/:id/callbacks", get(get_callbacks))
        .route("/status/requests/:id/unified", get(get_unified_request))
        .route(
            "/status/requests/:id/reconciliation",
            get(get_reconciliation),
        )
        .route(
            "/status/reconciliation/rollout-summary",
            get(get_reconciliation_rollout_summary),
        )
        .route(
            "/status/requests/:id/reconciliation/rerun",
            post(post_reconciliation_rerun),
        )
        .route(
            "/status/requests/:id/reconciliation/refresh-observation",
            post(post_refresh_observation),
        )
        .route("/status/requests/:id/exceptions", get(get_exceptions))
        .route("/status/exceptions", get(list_exceptions))
        .route("/status/exceptions/:case_id", get(get_exception_detail))
        .route(
            "/status/exceptions/:case_id/state",
            post(post_exception_state),
        )
        .route(
            "/status/exceptions/:case_id/acknowledge",
            post(post_exception_acknowledge),
        )
        .route(
            "/status/exceptions/:case_id/resolve",
            post(post_exception_resolve),
        )
        .route(
            "/status/exceptions/:case_id/false-positive",
            post(post_exception_false_positive),
        )
        .route("/status/callbacks/:callback_id", get(get_callback_detail))
        .route("/status/requests/:id/replay", post(post_replay))
        .route(
            "/status/requests/:id/replay-review",
            post(post_replay_review),
        )
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

async fn get_receipt_by_id(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(receipt_id): Path<String>,
) -> Result<Json<ReceiptLookupResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/receipts/:receipt_id",
        Some(receipt_id.as_str()),
    )
    .await?;

    let entry = state
        .store
        .load_receipt_by_id(&identity.tenant_id, &receipt_id)
        .await?
        .ok_or_else(|| StatusApiError::NotFound(format!("receipt `{receipt_id}` not found")))?;
    let mut entry = entry;
    redact_receipt_entry(&mut entry);
    let intent_id = entry.intent_id.to_string();

    audit_query(
        &state,
        &identity,
        "GET",
        "/receipts/:receipt_id",
        Some(receipt_id.clone()),
        true,
        json!({ "intent_id": intent_id }),
    )
    .await;

    Ok(Json(ReceiptLookupResponse {
        tenant_id: identity.tenant_id.to_string(),
        receipt_id,
        intent_id,
        entry,
    }))
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

    let mut entries = state
        .store
        .load_receipts(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?;
    redact_receipt_entries(entries.as_mut_slice());

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

async fn get_reconciliation(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<RequestReconciliationResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/reconciliation",
        Some(intent_id.as_str()),
    )
    .await?;

    let response =
        load_reconciliation_response(&state, identity.tenant_id.as_str(), &intent_id).await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/reconciliation",
        Some(intent_id),
        true,
        json!({ "runs": response.runs.len() }),
    )
    .await;

    Ok(Json(response))
}

async fn get_reconciliation_rollout_summary(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Query(query): Query<RolloutSummaryQuery>,
) -> Result<Json<ReconciliationRolloutSummaryResponse>, StatusApiError> {
    ensure_reconciliation_action_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/reconciliation/rollout-summary",
        None,
    )
    .await?;

    let mut response = state
        .store
        .load_reconciliation_rollout_summary(&identity.tenant_id, query.normalized_lookback_hours())
        .await?;
    response.queries = sample_rollout_query_metrics(
        &state,
        identity.tenant_id.as_str(),
        response.window.started_at_ms,
    )
    .await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/reconciliation/rollout-summary",
        None,
        true,
        json!({
            "lookback_hours": response.window.lookback_hours,
            "dirty_subjects": response.intake.dirty_subjects,
            "unresolved_cases": response.exceptions.unresolved_cases,
            "sampled_intent_id": response.queries.sampled_intent_id,
        }),
    )
    .await;

    Ok(Json(response))
}

async fn get_exceptions(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<RequestExceptionsResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/exceptions",
        Some(intent_id.as_str()),
    )
    .await?;

    let response =
        load_request_exceptions_response(&state, identity.tenant_id.as_str(), &intent_id).await?;

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/exceptions",
        Some(intent_id),
        true,
        json!({ "cases": response.cases.len() }),
    )
    .await;

    Ok(Json(response))
}

async fn post_reconciliation_rerun(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
    Json(request): Json<OperatorActionRequest>,
) -> Result<Json<ReconActionResponse>, StatusApiError> {
    post_reconciliation_action(
        state,
        headers,
        identity,
        intent_id,
        request,
        ReconOperatorActionType::Rerun,
        "/requests/:id/reconciliation/rerun",
    )
    .await
}

async fn post_refresh_observation(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
    Json(request): Json<OperatorActionRequest>,
) -> Result<Json<ReconActionResponse>, StatusApiError> {
    post_reconciliation_action(
        state,
        headers,
        identity,
        intent_id,
        request,
        ReconOperatorActionType::RefreshObservation,
        "/requests/:id/reconciliation/refresh-observation",
    )
    .await
}

async fn get_unified_request(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
) -> Result<Json<UnifiedRequestStatusResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/requests/:id/unified",
        Some(intent_id.as_str()),
    )
    .await?;

    let mut request = state
        .store
        .load_request_status(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?
        .ok_or_else(|| StatusApiError::NotFound(format!("request `{intent_id}` not found")))?;
    redact_request_status(&state, &identity, &mut request);

    let mut receipt_entries = state
        .store
        .load_receipts(&identity.tenant_id, &IntentId::from(intent_id.clone()))
        .await?;
    redact_receipt_entries(receipt_entries.as_mut_slice());

    let receipt = ReceiptResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id: intent_id.clone(),
        entries: receipt_entries,
    };

    let history = HistoryResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id: intent_id.clone(),
        transitions: state
            .store
            .load_history(&identity.tenant_id, &IntentId::from(intent_id.clone()))
            .await?,
    };

    let mut callbacks = state
        .store
        .load_callback_history(
            &identity.tenant_id,
            &IntentId::from(intent_id.clone()),
            true,
            25,
        )
        .await?;
    redact_callback_history(&state, &identity, callbacks.as_mut_slice());
    let callbacks = CallbackHistoryResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id: intent_id.clone(),
        callbacks,
    };

    let reconciliation =
        load_reconciliation_response(&state, identity.tenant_id.as_str(), &intent_id).await?;
    let exceptions =
        load_request_exceptions_response(&state, identity.tenant_id.as_str(), &intent_id).await?;

    let latest_receipt = receipt.entries.last();
    let reconciliation_eligible = receipt
        .entries
        .iter()
        .any(|entry| entry.reconciliation_eligible);
    let recon_status = reconciliation
        .latest_receipt
        .as_ref()
        .and_then(|receipt| receipt.normalized_result.clone())
        .or_else(|| {
            reconciliation.subject.as_ref().map(|subject| {
                if subject.dirty {
                    "pending_observation".to_owned()
                } else {
                    subject
                        .last_run_state
                        .clone()
                        .unwrap_or_else(|| "queued".to_owned())
                }
            })
        });
    let exception_summary = summarize_exceptions(&exceptions);
    let dashboard_status = derive_dashboard_status(
        reconciliation_eligible,
        recon_status.as_deref(),
        &exception_summary,
    );
    let evidence_references =
        collect_evidence_references(&reconciliation, &exceptions, latest_receipt);
    let latest_execution_receipt_id = latest_receipt.map(|entry| entry.receipt_id.to_string());
    let latest_recon_receipt_id = response_latest_recon_receipt_id(&reconciliation);
    let latest_evidence_snapshot_id = response_latest_evidence_snapshot_id(&exceptions);

    let response = UnifiedRequestStatusResponse {
        tenant_id: identity.tenant_id.to_string(),
        intent_id: intent_id.clone(),
        request,
        receipt,
        history,
        callbacks,
        reconciliation,
        latest_execution_receipt_id,
        latest_recon_receipt_id,
        latest_evidence_snapshot_id,
        exceptions,
        dashboard_status: dashboard_status.to_owned(),
        recon_status,
        reconciliation_eligible,
        exception_summary,
        evidence_references,
    };

    audit_query(
        &state,
        &identity,
        "GET",
        "/requests/:id/unified",
        Some(intent_id),
        true,
        json!({
            "dashboard_status": response.dashboard_status,
            "recon_status": response.recon_status,
            "unresolved_exceptions": response.exception_summary.unresolved_cases,
        }),
    )
    .await;

    Ok(Json(response))
}

async fn list_exceptions(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Query(query): Query<ExceptionIndexQuery>,
) -> Result<Json<ExceptionIndexResponse>, StatusApiError> {
    ensure_view_allowed(&state, &headers, &identity, "GET", "/exceptions", None).await?;

    let cases = state
        .exception_store
        .list_cases(
            identity.tenant_id.as_str(),
            &ExceptionSearchQuery {
                state: query.normalized_state(),
                severity: query.normalized_severity(),
                category: query.normalized_category(),
                adapter_id: query.normalized_adapter_id(),
                subject_id: query.normalized_subject_id(),
                intent_id: query.normalized_intent_id(),
                cluster_key: query.normalized_cluster_key(),
                search: query.normalized_search(),
                include_terminal: query.include_terminal(),
                limit: query.normalized_limit(),
                offset: query.normalized_offset(),
            },
        )
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?;

    let response = ExceptionIndexResponse {
        tenant_id: identity.tenant_id.to_string(),
        cases: cases
            .into_iter()
            .map(|case| map_exception_case_record(case, Vec::new()))
            .collect(),
    };

    audit_query(
        &state,
        &identity,
        "GET",
        "/exceptions",
        None,
        true,
        json!({ "cases": response.cases.len() }),
    )
    .await;

    Ok(Json(response))
}

async fn get_exception_detail(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(case_id): Path<String>,
) -> Result<Json<ExceptionDetailResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/exceptions/:case_id",
        Some(case_id.as_str()),
    )
    .await?;

    let detail = state
        .exception_store
        .load_case_detail(identity.tenant_id.as_str(), &case_id)
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?
        .ok_or_else(|| StatusApiError::NotFound(format!("exception case `{case_id}` not found")))?;

    let response = ExceptionDetailResponse {
        tenant_id: identity.tenant_id.to_string(),
        case: map_exception_case_record(detail.case, detail.evidence),
        events: detail
            .events
            .into_iter()
            .map(|event| ExceptionEventRecord {
                event_id: event.event_id,
                case_id: event.case_id,
                event_type: event.event_type,
                from_state: event.from_state.map(|value| value.as_str().to_owned()),
                to_state: event.to_state.map(|value| value.as_str().to_owned()),
                actor: event.actor,
                reason: event.reason,
                payload: event.payload,
                created_at_ms: event.created_at_ms,
            })
            .collect(),
        resolution_history: detail
            .resolution_history
            .into_iter()
            .map(|entry| ExceptionResolutionRecord {
                resolution_id: entry.resolution_id,
                case_id: entry.case_id,
                resolution_state: entry.resolution_state.as_str().to_owned(),
                actor: entry.actor,
                reason: entry.reason,
                payload: entry.payload,
                created_at_ms: entry.created_at_ms,
            })
            .collect(),
    };

    audit_query(
        &state,
        &identity,
        "GET",
        "/exceptions/:case_id",
        Some(case_id),
        true,
        json!({
            "event_count": response.events.len(),
            "resolution_history_count": response.resolution_history.len(),
        }),
    )
    .await;

    Ok(Json(response))
}

async fn post_exception_state(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(case_id): Path<String>,
    Json(request): Json<ExceptionStateTransitionRequest>,
) -> Result<Json<ExceptionStateTransitionResponse>, StatusApiError> {
    let next_state = parse_exception_state(&request.state)?;
    ensure_exception_action_allowed(
        &state,
        &headers,
        &identity,
        next_state,
        "POST",
        "/exceptions/:case_id/state",
        Some(case_id.as_str()),
    )
    .await?;
    let case =
        state
            .exception_store
            .transition_case_state(
                identity.tenant_id.as_str(),
                &case_id,
                next_state,
                &identity.principal.principal_id,
                &normalize_operator_reason(&request.reason)?,
                request.payload.unwrap_or_else(|| json!({})),
                current_unix_ms(),
            )
            .await
            .map_err(|err| match err {
                exception_intelligence::ExceptionIntelligenceError::NotFound(message) => {
                    StatusApiError::NotFound(message)
                }
                exception_intelligence::ExceptionIntelligenceError::BadRequest(message)
                | exception_intelligence::ExceptionIntelligenceError::InvalidStateTransition(
                    message,
                ) => StatusApiError::BadRequest(message),
                exception_intelligence::ExceptionIntelligenceError::Backend(message) => {
                    StatusApiError::Internal(message)
                }
            })?;

    let response = ExceptionStateTransitionResponse {
        ok: true,
        case: map_exception_case_record(case, Vec::new()),
    };
    let audited_case_id = response.case.case_id.clone();
    let audited_intent_id = response.case.intent_id.clone();
    let audited_state = response.case.state.clone();

    audit_operator_action(
        &state,
        &identity,
        "exception_state_transition",
        &audited_intent_id,
        true,
        "exception case state updated",
        Some(json!({
            "case_id": audited_case_id,
            "state": audited_state,
        })),
    )
    .await;

    Ok(Json(response))
}

async fn post_exception_acknowledge(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(case_id): Path<String>,
    Json(request): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, StatusApiError> {
    post_exception_action(
        state,
        headers,
        identity,
        case_id,
        request,
        ExceptionState::Acknowledged,
        "exception_acknowledge",
        "/exceptions/:case_id/acknowledge",
    )
    .await
}

async fn post_exception_resolve(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(case_id): Path<String>,
    Json(request): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, StatusApiError> {
    post_exception_action(
        state,
        headers,
        identity,
        case_id,
        request,
        ExceptionState::Resolved,
        "exception_resolve",
        "/exceptions/:case_id/resolve",
    )
    .await
}

async fn post_exception_false_positive(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(case_id): Path<String>,
    Json(request): Json<OperatorActionRequest>,
) -> Result<Json<ExceptionActionResponse>, StatusApiError> {
    post_exception_action(
        state,
        headers,
        identity,
        case_id,
        request,
        ExceptionState::FalsePositive,
        "exception_false_positive",
        "/exceptions/:case_id/false-positive",
    )
    .await
}

async fn get_callback_detail(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(callback_id): Path<String>,
) -> Result<Json<CallbackDetailResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "GET",
        "/callbacks/:callback_id",
        Some(callback_id.as_str()),
    )
    .await?;

    let (intent_id, mut callback) = state
        .store
        .load_callback_delivery(&identity.tenant_id, &callback_id, true, 25)
        .await?
        .ok_or_else(|| StatusApiError::NotFound(format!("callback `{callback_id}` not found")))?;

    let mut request = state
        .store
        .load_request_status(&identity.tenant_id, &intent_id)
        .await?
        .ok_or_else(|| {
            StatusApiError::NotFound(format!(
                "request for callback `{callback_id}` was not found"
            ))
        })?;
    let mut receipt_entries = state
        .store
        .load_receipts(&identity.tenant_id, &intent_id)
        .await?;
    redact_receipt_entries(receipt_entries.as_mut_slice());
    let history = state
        .store
        .load_history(&identity.tenant_id, &intent_id)
        .await?;

    redact_request_status(&state, &identity, &mut request);
    redact_callback_history(&state, &identity, std::slice::from_mut(&mut callback));

    audit_query(
        &state,
        &identity,
        "GET",
        "/callbacks/:callback_id",
        Some(callback_id.clone()),
        true,
        json!({ "intent_id": intent_id.to_string() }),
    )
    .await;

    Ok(Json(CallbackDetailResponse {
        ok: true,
        callback_id,
        intent_id: intent_id.to_string(),
        callback,
        request,
        receipt: ReceiptResponse {
            tenant_id: identity.tenant_id.to_string(),
            intent_id: intent_id.to_string(),
            entries: receipt_entries,
        },
        history: HistoryResponse {
            tenant_id: identity.tenant_id.to_string(),
            intent_id: intent_id.to_string(),
            transitions: history,
        },
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

async fn post_replay_review(
    State(state): State<StatusApiState>,
    headers: HeaderMap,
    identity: RequestIdentity,
    Path(intent_id): Path<String>,
    Json(body): Json<OperatorActionRequest>,
) -> Result<Json<ReplayReviewResponse>, StatusApiError> {
    ensure_view_allowed(
        &state,
        &headers,
        &identity,
        "POST",
        "/requests/:id/replay-review",
        Some(intent_id.as_str()),
    )
    .await?;

    if !state
        .authorizer
        .can_replay(&identity.principal, &identity.tenant_id)
    {
        let reason = normalize_operator_reason(&body.reason)?;
        audit_operator_action(
            &state,
            &identity,
            "request_execution_replay_review",
            &intent_id,
            false,
            &reason,
            None,
        )
        .await;
        return Err(StatusApiError::Forbidden(
            "principal is not authorized to request execution replay review".to_owned(),
        ));
    }

    let replay_gateway = state.replay_gateway.as_ref().ok_or_else(|| {
        StatusApiError::Unavailable("replay gateway is not configured".to_owned())
    })?;

    let reason = normalize_operator_reason(&body.reason)?;
    let command = ReplayCommand {
        tenant_id: identity.tenant_id.clone(),
        intent_id: IntentId::from(intent_id.clone()),
        requested_by: identity.principal.clone(),
        reason: reason.clone(),
    };

    match replay_gateway.request_replay(command).await {
        Ok(replay) => {
            let replay_response = ReplayResponse {
                source_job_id: replay.source_job_id.to_string(),
                replay_job_id: replay.replay_job.job_id.to_string(),
                replay_count: replay.replay_job.replay_count,
                state: replay.replay_job.state,
                route_adapter_id: replay.replay_job.adapter_id.to_string(),
                details: {
                    let mut details = BTreeMap::new();
                    details.insert(
                        "reason".to_owned(),
                        "execution replay review accepted by execution core".to_owned(),
                    );
                    details
                },
            };
            audit_operator_action(
                &state,
                &identity,
                "request_execution_replay_review",
                &intent_id,
                true,
                &reason,
                Some(json!({
                    "handoff": "execution_core",
                    "source_job_id": replay_response.source_job_id,
                    "replay_job_id": replay_response.replay_job_id,
                    "state": format!("{:?}", replay_response.state),
                    "payload": body.payload,
                })),
            )
            .await;

            Ok(Json(ReplayReviewResponse {
                ok: true,
                handoff: "execution_core".to_owned(),
                replay: replay_response,
            }))
        }
        Err(err) => {
            audit_operator_action(
                &state,
                &identity,
                "request_execution_replay_review",
                &intent_id,
                false,
                &reason,
                Some(json!({
                    "handoff": "execution_core",
                    "error": err.to_string(),
                    "payload": body.payload,
                })),
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
        "admin_action_denied",
        "__admin_action__",
        false,
        "principal role must be admin",
        Some(json!({
            "required_role": "admin",
            "actual_role": role_label(identity.principal.role),
        })),
    )
    .await;
    Err(StatusApiError::Forbidden(
        "principal is not authorized to perform this admin action".to_owned(),
    ))
}

async fn ensure_reconciliation_action_allowed(
    state: &StatusApiState,
    headers: &HeaderMap,
    identity: &RequestIdentity,
    method: &str,
    endpoint: &str,
    resource_id: Option<&str>,
) -> Result<(), StatusApiError> {
    ensure_view_allowed(state, headers, identity, method, endpoint, resource_id).await?;

    if state
        .authorizer
        .can_manage_reconciliation(&identity.principal, &identity.tenant_id)
    {
        return Ok(());
    }

    audit_operator_action(
        state,
        identity,
        "reconciliation_action_denied",
        resource_id.unwrap_or("__reconciliation_action__"),
        false,
        "principal is not authorized to manage reconciliation",
        Some(json!({
            "required_role": "operator_or_admin",
            "actual_role": role_label(identity.principal.role),
        })),
    )
    .await;
    Err(StatusApiError::Forbidden(
        "principal is not authorized to manage reconciliation".to_owned(),
    ))
}

async fn ensure_exception_action_allowed(
    state: &StatusApiState,
    headers: &HeaderMap,
    identity: &RequestIdentity,
    target_state: ExceptionState,
    method: &str,
    endpoint: &str,
    resource_id: Option<&str>,
) -> Result<(), StatusApiError> {
    ensure_view_allowed(state, headers, identity, method, endpoint, resource_id).await?;

    let allowed = if target_state.is_terminal() {
        state
            .authorizer
            .can_resolve_exception_case(&identity.principal, &identity.tenant_id)
    } else {
        state
            .authorizer
            .can_manage_exception_case(&identity.principal, &identity.tenant_id)
    };
    if allowed {
        return Ok(());
    }

    audit_operator_action(
        state,
        identity,
        "exception_action_denied",
        resource_id.unwrap_or("__exception_action__"),
        false,
        "principal is not authorized to perform this exception action",
        Some(json!({
            "target_state": target_state.as_str(),
            "required_role": if target_state.is_terminal() { "admin" } else { "operator_or_admin" },
            "actual_role": role_label(identity.principal.role),
        })),
    )
    .await;
    Err(StatusApiError::Forbidden(
        "principal is not authorized to perform this exception action".to_owned(),
    ))
}

fn normalize_operator_reason(raw: &str) -> Result<String, StatusApiError> {
    let reason = raw.trim();
    if reason.len() < 8 {
        return Err(StatusApiError::BadRequest(
            "operator reason must be at least 8 characters".to_owned(),
        ));
    }
    Ok(reason.to_owned())
}

async fn post_reconciliation_action(
    state: StatusApiState,
    headers: HeaderMap,
    identity: RequestIdentity,
    intent_id: String,
    request: OperatorActionRequest,
    action_type: ReconOperatorActionType,
    endpoint: &str,
) -> Result<Json<ReconActionResponse>, StatusApiError> {
    ensure_reconciliation_action_allowed(
        &state,
        &headers,
        &identity,
        "POST",
        endpoint,
        Some(intent_id.as_str()),
    )
    .await?;

    let reason = normalize_operator_reason(&request.reason)?;
    let subject = state
        .recon_store
        .load_subject_for_intent(identity.tenant_id.as_str(), intent_id.as_str())
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?
        .ok_or_else(|| {
            StatusApiError::NotFound(format!(
                "reconciliation subject for request `{intent_id}` was not found"
            ))
        })?;

    let (action, subject) = state
        .recon_store
        .queue_operator_action(
            &subject,
            action_type,
            identity.principal.principal_id.as_str(),
            &reason,
            request.payload.clone().unwrap_or_else(|| json!({})),
            current_unix_ms(),
        )
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?;

    audit_operator_action(
        &state,
        &identity,
        action_type.as_str(),
        &intent_id,
        true,
        &reason,
        Some(json!({
            "action_id": action.action_id,
            "subject_id": action.subject_id,
            "scheduled_at_ms": subject.scheduled_at_ms,
            "next_reconcile_after_ms": subject.next_reconcile_after_ms,
            "payload": action.payload,
        })),
    )
    .await;

    Ok(Json(ReconActionResponse {
        ok: true,
        action: action.action_type.as_str().to_owned(),
        action_id: action.action_id,
        subject: map_reconciliation_subject(subject),
    }))
}

async fn post_exception_action(
    state: StatusApiState,
    headers: HeaderMap,
    identity: RequestIdentity,
    case_id: String,
    request: OperatorActionRequest,
    target_state: ExceptionState,
    action_type: &str,
    endpoint: &str,
) -> Result<Json<ExceptionActionResponse>, StatusApiError> {
    ensure_exception_action_allowed(
        &state,
        &headers,
        &identity,
        target_state,
        "POST",
        endpoint,
        Some(case_id.as_str()),
    )
    .await?;

    let reason = normalize_operator_reason(&request.reason)?;
    let case =
        state
            .exception_store
            .transition_case_state(
                identity.tenant_id.as_str(),
                &case_id,
                target_state,
                &identity.principal.principal_id,
                &reason,
                request.payload.clone().unwrap_or_else(|| json!({})),
                current_unix_ms(),
            )
            .await
            .map_err(|err| match err {
                exception_intelligence::ExceptionIntelligenceError::NotFound(message) => {
                    StatusApiError::NotFound(message)
                }
                exception_intelligence::ExceptionIntelligenceError::BadRequest(message)
                | exception_intelligence::ExceptionIntelligenceError::InvalidStateTransition(
                    message,
                ) => StatusApiError::BadRequest(message),
                exception_intelligence::ExceptionIntelligenceError::Backend(message) => {
                    StatusApiError::Internal(message)
                }
            })?;

    audit_operator_action(
        &state,
        &identity,
        action_type,
        &case.intent_id,
        true,
        &reason,
        Some(json!({
            "case_id": case.case_id,
            "state": case.state.as_str(),
            "payload": request.payload,
        })),
    )
    .await;

    Ok(Json(ExceptionActionResponse {
        ok: true,
        action: action_type.to_owned(),
        case: map_exception_case_record(case, Vec::new()),
    }))
}

async fn load_reconciliation_response(
    state: &StatusApiState,
    tenant_id: &str,
    intent_id: &str,
) -> Result<RequestReconciliationResponse, StatusApiError> {
    let payload = state
        .recon_store
        .load_request_reconciliation(tenant_id, intent_id)
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?;

    Ok(match payload {
        Some((subject, runs, latest_receipt, expected_facts, observed_facts)) => {
            RequestReconciliationResponse {
                tenant_id: tenant_id.to_owned(),
                intent_id: intent_id.to_owned(),
                subject: Some(map_reconciliation_subject(subject)),
                runs: runs.into_iter().map(map_reconciliation_run).collect(),
                latest_receipt: latest_receipt.map(map_reconciliation_receipt),
                expected_facts: expected_facts.into_iter().map(map_expected_fact).collect(),
                observed_facts: observed_facts.into_iter().map(map_observed_fact).collect(),
            }
        }
        None => RequestReconciliationResponse {
            tenant_id: tenant_id.to_owned(),
            intent_id: intent_id.to_owned(),
            subject: None,
            runs: Vec::new(),
            latest_receipt: None,
            expected_facts: Vec::new(),
            observed_facts: Vec::new(),
        },
    })
}

async fn sample_rollout_query_metrics(
    state: &StatusApiState,
    tenant_id: &str,
    started_at_ms: u64,
) -> Result<crate::model::ReconciliationRolloutQueryMetrics, StatusApiError> {
    let started_at_ms_i64 = started_at_ms.min(i64::MAX as u64) as i64;
    let sampled_intent_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT intent_id
        FROM execution_core_receipts
        WHERE tenant_id = $1
          AND occurred_at_ms >= $2
        ORDER BY occurred_at_ms DESC, receipt_id DESC
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(started_at_ms_i64)
    .fetch_optional(state.store.pool())
    .await
    .map_err(|err| StatusApiError::Internal(format!("failed to sample rollout intent: {err}")))?;

    let exception_query_started = Instant::now();
    state
        .exception_store
        .list_cases(
            tenant_id,
            &ExceptionSearchQuery {
                state: None,
                severity: None,
                category: None,
                adapter_id: None,
                subject_id: None,
                intent_id: None,
                cluster_key: None,
                search: None,
                include_terminal: true,
                limit: 50,
                offset: 0,
            },
        )
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?;
    let exception_index_query_ms = Some(
        exception_query_started
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
    );

    let unified_request_query_ms = if let Some(intent_id) = sampled_intent_id.as_deref() {
        let unified_started = Instant::now();
        state
            .store
            .load_request_status(
                &TenantId::from(tenant_id),
                &IntentId::from(intent_id.to_owned()),
            )
            .await?;
        state
            .store
            .load_receipts(
                &TenantId::from(tenant_id),
                &IntentId::from(intent_id.to_owned()),
            )
            .await?;
        state
            .store
            .load_history(
                &TenantId::from(tenant_id),
                &IntentId::from(intent_id.to_owned()),
            )
            .await?;
        state
            .store
            .load_callback_history(
                &TenantId::from(tenant_id),
                &IntentId::from(intent_id.to_owned()),
                true,
                25,
            )
            .await?;
        let _ = load_reconciliation_response(state, tenant_id, intent_id).await?;
        let _ = load_request_exceptions_response(state, tenant_id, intent_id).await?;
        Some(
            unified_started
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        )
    } else {
        None
    };

    Ok(crate::model::ReconciliationRolloutQueryMetrics {
        sampled_intent_id,
        exception_index_query_ms,
        unified_request_query_ms,
    })
}

async fn load_request_exceptions_response(
    state: &StatusApiState,
    tenant_id: &str,
    intent_id: &str,
) -> Result<RequestExceptionsResponse, StatusApiError> {
    let cases = state
        .exception_store
        .list_cases_for_intent(tenant_id, intent_id)
        .await
        .map_err(|err| StatusApiError::Internal(err.to_string()))?;

    Ok(RequestExceptionsResponse {
        tenant_id: tenant_id.to_owned(),
        intent_id: intent_id.to_owned(),
        cases: cases
            .into_iter()
            .map(|(case, evidence)| map_exception_case_record(case, evidence))
            .collect(),
    })
}

fn map_exception_case_record(
    case: exception_intelligence::ExceptionCase,
    evidence: Vec<exception_intelligence::ExceptionEvidence>,
) -> ExceptionCaseRecord {
    ExceptionCaseRecord {
        case_id: case.case_id,
        tenant_id: case.tenant_id,
        subject_id: case.subject_id,
        intent_id: case.intent_id,
        job_id: case.job_id,
        adapter_id: case.adapter_id,
        category: case.category.as_str().to_owned(),
        severity: case.severity.as_str().to_owned(),
        state: case.state.as_str().to_owned(),
        summary: case.summary,
        machine_reason: case.machine_reason,
        dedupe_key: case.dedupe_key,
        cluster_key: case.cluster_key,
        first_seen_at_ms: case.first_seen_at_ms,
        last_seen_at_ms: case.last_seen_at_ms,
        occurrence_count: case.occurrence_count,
        created_at_ms: case.created_at_ms,
        updated_at_ms: case.updated_at_ms,
        resolved_at_ms: case.resolved_at_ms,
        latest_run_id: case.latest_run_id,
        latest_outcome_id: case.latest_outcome_id,
        latest_recon_receipt_id: case.latest_recon_receipt_id,
        latest_execution_receipt_id: case.latest_execution_receipt_id,
        latest_evidence_snapshot_id: case.latest_evidence_snapshot_id,
        last_actor: case.last_actor,
        evidence: evidence
            .into_iter()
            .map(|entry| ExceptionEvidenceRecord {
                evidence_id: entry.evidence_id,
                case_id: entry.case_id,
                evidence_type: entry.evidence_type,
                source_table: entry.source_table,
                source_id: entry.source_id,
                observed_at_ms: entry.observed_at_ms,
                payload: entry.payload,
                created_at_ms: entry.created_at_ms,
            })
            .collect(),
    }
}

fn parse_exception_state(value: &str) -> Result<ExceptionState, StatusApiError> {
    ExceptionState::parse(value)
        .ok_or_else(|| StatusApiError::BadRequest(format!("unsupported exception state `{value}`")))
}

fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn summarize_exceptions(response: &RequestExceptionsResponse) -> UnifiedExceptionSummary {
    let total_cases = response.cases.len() as u32;
    let unresolved: Vec<&ExceptionCaseRecord> = response
        .cases
        .iter()
        .filter(|case| {
            !matches!(
                case.state.as_str(),
                "resolved" | "dismissed" | "false_positive"
            )
        })
        .collect();
    let highest_severity = unresolved
        .iter()
        .max_by_key(|case| severity_rank(case.severity.as_str()))
        .map(|case| case.severity.clone());
    let mut categories = BTreeSet::new();
    let mut open_case_ids = Vec::new();
    for case in &unresolved {
        categories.insert(case.category.clone());
        open_case_ids.push(case.case_id.clone());
    }

    UnifiedExceptionSummary {
        total_cases,
        unresolved_cases: unresolved.len() as u32,
        highest_severity,
        categories: categories.into_iter().collect(),
        open_case_ids,
    }
}

fn derive_dashboard_status(
    reconciliation_eligible: bool,
    recon_status: Option<&str>,
    exception_summary: &UnifiedExceptionSummary,
) -> &'static str {
    if exception_summary.unresolved_cases > 0 {
        if exception_summary
            .categories
            .iter()
            .any(|category| category == "manual_review_required")
        {
            return "manual_review_required";
        }
        return "mismatch_detected";
    }

    match recon_status.unwrap_or_default() {
        "matched" => "matched",
        "manual_review_required" => "manual_review_required",
        "partially_matched" | "unmatched" | "stale" => "mismatch_detected",
        "pending_observation"
        | "queued"
        | "collecting_observations"
        | "matching"
        | "writing_receipt" => "pending_verification",
        _ if reconciliation_eligible => "pending_verification",
        _ => "pending_verification",
    }
}

fn collect_evidence_references(
    reconciliation: &RequestReconciliationResponse,
    exceptions: &RequestExceptionsResponse,
    latest_receipt: Option<&execution_core::ReceiptEntry>,
) -> Vec<UnifiedEvidenceReferenceRecord> {
    let mut refs = Vec::new();

    if let Some(entry) = latest_receipt {
        refs.push(UnifiedEvidenceReferenceRecord {
            kind: "execution_receipt".to_owned(),
            label: format!("receipt {}", entry.receipt_id),
            source_table: Some("execution_core_receipts".to_owned()),
            source_id: Some(entry.receipt_id.to_string()),
            observed_at_ms: Some(entry.occurred_at_ms),
        });
    }

    if let Some(receipt) = reconciliation.latest_receipt.as_ref() {
        refs.push(UnifiedEvidenceReferenceRecord {
            kind: "recon_receipt".to_owned(),
            label: format!("recon {}", receipt.recon_receipt_id),
            source_table: Some("recon_core_receipts".to_owned()),
            source_id: Some(receipt.recon_receipt_id.clone()),
            observed_at_ms: Some(receipt.created_at_ms),
        });
    }

    for case in &exceptions.cases {
        if let Some(outcome_id) = case.latest_outcome_id.as_ref() {
            refs.push(UnifiedEvidenceReferenceRecord {
                kind: "recon_outcome".to_owned(),
                label: format!("outcome {}", outcome_id),
                source_table: Some("recon_core_outcomes".to_owned()),
                source_id: Some(outcome_id.clone()),
                observed_at_ms: None,
            });
        }
        if let Some(snapshot_id) = case.latest_evidence_snapshot_id.as_ref() {
            refs.push(UnifiedEvidenceReferenceRecord {
                kind: "evidence_snapshot".to_owned(),
                label: format!("snapshot {}", snapshot_id),
                source_table: Some("recon_core_evidence_snapshots".to_owned()),
                source_id: Some(snapshot_id.clone()),
                observed_at_ms: None,
            });
        }
        for evidence in case.evidence.iter().take(6) {
            refs.push(UnifiedEvidenceReferenceRecord {
                kind: evidence.evidence_type.clone(),
                label: evidence
                    .source_id
                    .clone()
                    .unwrap_or_else(|| evidence.evidence_type.clone()),
                source_table: evidence.source_table.clone(),
                source_id: evidence.source_id.clone(),
                observed_at_ms: evidence.observed_at_ms.or(Some(evidence.created_at_ms)),
            });
        }
    }

    let mut seen = BTreeSet::new();
    refs.into_iter()
        .filter(|entry| {
            let key = format!(
                "{}|{}|{}|{}",
                entry.kind,
                entry.source_table.as_deref().unwrap_or_default(),
                entry.source_id.as_deref().unwrap_or_default(),
                entry.observed_at_ms.unwrap_or_default()
            );
            seen.insert(key)
        })
        .collect()
}

fn response_latest_recon_receipt_id(
    reconciliation: &RequestReconciliationResponse,
) -> Option<String> {
    reconciliation
        .latest_receipt
        .as_ref()
        .map(|receipt| receipt.recon_receipt_id.clone())
}

fn response_latest_evidence_snapshot_id(exceptions: &RequestExceptionsResponse) -> Option<String> {
    exceptions
        .cases
        .iter()
        .find_map(|case| case.latest_evidence_snapshot_id.clone())
}

fn severity_rank(value: &str) -> u8 {
    match value {
        "critical" => 4,
        "high" => 3,
        "warning" => 2,
        "info" => 1,
        _ => 0,
    }
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

fn redact_receipt_entries(entries: &mut [ReceiptEntry]) {
    for entry in entries {
        redact_receipt_entry(entry);
    }
}

fn redact_receipt_entry(entry: &mut ReceiptEntry) {
    let sensitive_markers = ["secret", "token", "password", "bearer", "signing"];
    entry.details.retain(|key, _| {
        let lower = key.to_ascii_lowercase();
        !sensitive_markers
            .iter()
            .any(|marker| lower.contains(marker))
    });
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

fn map_reconciliation_subject(subject: recon_core::ReconSubject) -> ReconciliationSubjectRecord {
    ReconciliationSubjectRecord {
        subject_id: subject.subject_id,
        tenant_id: subject.tenant_id,
        intent_id: subject.intent_id,
        job_id: subject.job_id,
        adapter_id: subject.adapter_id,
        canonical_state: subject.canonical_state,
        platform_classification: subject.platform_classification,
        latest_receipt_id: subject.latest_receipt_id,
        latest_transition_id: subject.latest_transition_id,
        latest_callback_id: subject.latest_callback_id,
        latest_signal_id: subject.latest_signal_id,
        latest_signal_kind: subject.latest_signal_kind,
        execution_correlation_id: subject.execution_correlation_id,
        adapter_execution_reference: subject.adapter_execution_reference,
        external_observation_key: subject.external_observation_key,
        expected_fact_snapshot: subject.expected_fact_snapshot,
        dirty: subject.dirty,
        recon_attempt_count: subject.recon_attempt_count,
        recon_retry_count: subject.recon_retry_count,
        created_at_ms: subject.created_at_ms,
        updated_at_ms: subject.updated_at_ms,
        scheduled_at_ms: subject.scheduled_at_ms,
        next_reconcile_after_ms: subject.next_reconcile_after_ms,
        last_reconciled_at_ms: subject.last_reconciled_at_ms,
        last_recon_error: subject.last_recon_error,
        last_run_state: subject
            .last_run_state
            .map(|value| value.as_str().to_owned()),
    }
}

fn map_reconciliation_run(run: recon_core::ReconRun) -> ReconciliationRunRecord {
    ReconciliationRunRecord {
        run_id: run.run_id,
        subject_id: run.subject_id,
        tenant_id: run.tenant_id,
        intent_id: run.intent_id,
        job_id: run.job_id,
        adapter_id: run.adapter_id,
        rule_pack: run.rule_pack,
        lifecycle_state: run.lifecycle_state.as_str().to_owned(),
        normalized_result: run.normalized_result.map(|value| value.as_str().to_owned()),
        outcome: run.outcome.as_str().to_owned(),
        summary: run.summary,
        machine_reason: run.machine_reason,
        expected_fact_count: run.expected_fact_count,
        observed_fact_count: run.observed_fact_count,
        matched_fact_count: run.matched_fact_count,
        unmatched_fact_count: run.unmatched_fact_count,
        created_at_ms: run.created_at_ms,
        updated_at_ms: run.updated_at_ms,
        completed_at_ms: run.completed_at_ms,
        attempt_number: run.attempt_number,
        retry_scheduled_at_ms: run.retry_scheduled_at_ms,
        last_error: run.last_error,
        exception_case_ids: run.exception_case_ids,
    }
}

fn map_reconciliation_receipt(receipt: recon_core::ReconReceipt) -> ReconciliationReceiptRecord {
    ReconciliationReceiptRecord {
        recon_receipt_id: receipt.recon_receipt_id,
        run_id: receipt.run_id,
        subject_id: receipt.subject_id,
        normalized_result: receipt
            .normalized_result
            .map(|value| value.as_str().to_owned()),
        outcome: receipt.outcome.as_str().to_owned(),
        summary: receipt.summary,
        details: receipt.details,
        created_at_ms: receipt.created_at_ms,
    }
}

fn map_expected_fact(fact: recon_core::ExpectedFact) -> ReconciliationFactRecord {
    ReconciliationFactRecord {
        fact_id: fact.expected_fact_id,
        run_id: fact.run_id,
        subject_id: fact.subject_id,
        fact_type: fact.fact_type,
        fact_key: fact.fact_key,
        fact_value: fact.fact_value,
        source_kind: None,
        source_table: None,
        source_id: None,
        metadata: fact.derived_from,
        observed_at_ms: None,
        created_at_ms: fact.created_at_ms,
    }
}

fn map_observed_fact(fact: recon_core::ObservedFact) -> ReconciliationFactRecord {
    ReconciliationFactRecord {
        fact_id: fact.observed_fact_id,
        run_id: fact.run_id,
        subject_id: fact.subject_id,
        fact_type: fact.fact_type,
        fact_key: fact.fact_key,
        fact_value: fact.fact_value,
        source_kind: Some(fact.source_kind),
        source_table: fact.source_table,
        source_id: fact.source_id,
        metadata: fact.metadata,
        observed_at_ms: fact.observed_at_ms,
        created_at_ms: fact.created_at_ms,
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
