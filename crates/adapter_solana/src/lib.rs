use adapter_contract::{
    AdapterExecutionContext, AdapterExecutionEnvelope, AdapterProgressState, AdapterRegistry,
    AdapterResumeContext, AdapterStatusHandle, AdapterStatusSnapshot, DomainAdapter,
};
use async_trait::async_trait;
use execution_core::{
    AdapterExecutionError, AdapterExecutionRequest, AdapterExecutor, AdapterId, AdapterOutcome,
};
use rpc_layer::solana::{
    rpc_get_latest_blockhash_with_failover, rpc_get_signature_status_with_failover,
    rpc_send_transaction_with_failover, rpc_simulate_transaction_with_failover, RpcCallError,
};
use rpc_layer::{
    preferred_provider_urls, primary_provider_url, resolve_provider_urls,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SolanaAdapterConfig {
    pub sync_max_polls: usize,
    pub sync_poll_delay_ms: u64,
}

impl Default for SolanaAdapterConfig {
    fn default() -> Self {
        Self {
            sync_max_polls: 8,
            sync_poll_delay_ms: 1_200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolanaErrorClass {
    Retryable,
    Terminal,
    Blocked,
    ManualInterventionRequired,
}

impl SolanaErrorClass {
    fn as_str(self) -> &'static str {
        match self {
            SolanaErrorClass::Retryable => "retryable",
            SolanaErrorClass::Terminal => "terminal",
            SolanaErrorClass::Blocked => "blocked",
            SolanaErrorClass::ManualInterventionRequired => "manual_intervention_required",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedSolanaError {
    pub code: String,
    pub message: String,
    pub class: SolanaErrorClass,
    pub raw_provider_error: Value,
}

#[derive(Debug, Clone)]
struct SolanaExecutionInput {
    intent_id: String,
    intent_type: String,
    from_addr: Option<String>,
    to_addr: String,
    amount: i64,
    asset: String,
    program_id: String,
    action: String,
    signed_tx_base64: Option<String>,
    skip_preflight: bool,
    cu_limit: Option<i32>,
    cu_price_micro_lamports: Option<i64>,
    blockhash_used: Option<String>,
    simulation_outcome: Option<String>,
    provider_used: Option<String>,
    rpc_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaygroundDemoScenario {
    Real,
    SyntheticSuccess,
    RetryThenSuccess,
    TerminalFailure,
}

impl PlaygroundDemoScenario {
    fn as_str(self) -> &'static str {
        match self {
            Self::Real => "real",
            Self::SyntheticSuccess => "success",
            Self::RetryThenSuccess => "retry_then_success",
            Self::TerminalFailure => "terminal_failure",
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveAttemptRow {
    attempt_id: Uuid,
    status: String,
    signature: Option<String>,
    last_confirmation_status: Option<String>,
    last_err_json: Option<Value>,
    blockhash_used: Option<String>,
    simulation_outcome: Option<String>,
    provider_used: Option<String>,
}

impl ActiveAttemptRow {
    fn from_enhanced(
        row: (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            Option<Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    ) -> Self {
        Self {
            attempt_id: row.0,
            status: row.1,
            signature: row.2,
            last_confirmation_status: row.3,
            last_err_json: row.4,
            blockhash_used: row.5,
            simulation_outcome: row.6,
            provider_used: row.7,
        }
    }

    fn from_basic(row: (Uuid, String, Option<String>)) -> Self {
        Self {
            attempt_id: row.0,
            status: row.1,
            signature: row.2,
            last_confirmation_status: None,
            last_err_json: None,
            blockhash_used: None,
            simulation_outcome: None,
            provider_used: None,
        }
    }
}

static SOLANA_SCHEMA_READY: OnceCell<()> = OnceCell::const_new();

#[derive(Clone)]
pub struct SolanaQueueAdapter {
    pool: PgPool,
    config: SolanaAdapterConfig,
}

impl SolanaQueueAdapter {
    pub fn new(pool: PgPool, config: SolanaAdapterConfig) -> Self {
        Self { pool, config }
    }

    pub fn config(&self) -> &SolanaAdapterConfig {
        &self.config
    }

    fn sync_max_polls(&self) -> usize {
        self.config.sync_max_polls.max(1)
    }

    fn sync_poll_delay(&self) -> Duration {
        Duration::from_millis(self.config.sync_poll_delay_ms.max(200))
    }

    fn supports_intent_kind(intent_kind: &str) -> bool {
        matches!(intent_kind, "solana.transfer.v1" | "solana.broadcast.v1")
    }

    pub fn validate_intent(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<(), AdapterExecutionError> {
        if !Self::supports_intent_kind(request.intent_kind.as_str()) {
            return Err(AdapterExecutionError::UnsupportedIntent(format!(
                "unsupported intent kind `{}` for solana adapter",
                request.intent_kind
            )));
        }

        let _ = normalize_payload(request)?;
        Ok(())
    }

    pub async fn execute_solana_intent(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.validate_intent(request)?;
        let payload = normalize_payload(request)?;
        let input = parse_execution_input(&payload)?;
        self.execute_direct(request, input, None).await
    }

    pub async fn resume_solana_intent(
        &self,
        request: &AdapterExecutionRequest,
        context: &AdapterResumeContext,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.validate_intent(request)?;
        let payload = normalize_payload(request)?;
        let input = parse_execution_input(&payload)?;
        self.execute_direct(request, input, Some(context)).await
    }

    pub async fn check_submission_status(
        &self,
        handle: &AdapterStatusHandle,
    ) -> Result<AdapterStatusSnapshot, AdapterExecutionError> {
        if let Some(provider_reference) = handle.provider_reference.as_deref() {
            if let Ok(attempt_id) = Uuid::parse_str(provider_reference) {
                if let Some(row) = self.fetch_status_row_by_attempt_id(attempt_id).await? {
                    return Ok(self.status_from_row(row, Some(provider_reference.to_owned())));
                }
            }
        }

        let intent_id = handle.intent_id.trim();
        if intent_id.is_empty() {
            return Err(AdapterExecutionError::ContractViolation(
                "status handle requires non-empty `intent_id`".to_owned(),
            ));
        }

        let row = self.fetch_status_row_by_intent_id(intent_id).await?;
        Ok(self.status_from_row(row, handle.provider_reference.clone()))
    }

    pub fn normalize_solana_error(&self, raw_provider_error: &Value) -> NormalizedSolanaError {
        let raw_text = raw_provider_error.to_string();
        let text = raw_text.to_ascii_lowercase();

        if contains_any(
            &text,
            &[
                "blockhash not found",
                "expired blockhash",
                "block height exceeded",
                "transaction expired",
            ],
        ) {
            return NormalizedSolanaError {
                code: "solana.blockhash_expired".to_owned(),
                message: "solana transaction expired before finalization".to_owned(),
                class: SolanaErrorClass::Retryable,
                raw_provider_error: raw_provider_error.clone(),
            };
        }

        if contains_any(
            &text,
            &[
                "rate limit",
                "too many requests",
                "timeout",
                "timed out",
                "node is behind",
                "temporarily unavailable",
            ],
        ) {
            return NormalizedSolanaError {
                code: "solana.provider_transient".to_owned(),
                message: "solana provider transient failure".to_owned(),
                class: SolanaErrorClass::Retryable,
                raw_provider_error: raw_provider_error.clone(),
            };
        }

        if contains_any(&text, &["unauthorized", "forbidden", "permission denied"]) {
            return NormalizedSolanaError {
                code: "solana.unauthorized".to_owned(),
                message: "solana provider rejected credentials or permissions".to_owned(),
                class: SolanaErrorClass::Blocked,
                raw_provider_error: raw_provider_error.clone(),
            };
        }

        if contains_any(
            &text,
            &[
                "insufficient funds",
                "account not found",
                "invalid account",
                "signature verification failed",
                "invalid instruction",
                "invalid program",
            ],
        ) {
            return NormalizedSolanaError {
                code: "solana.invalid_request".to_owned(),
                message: "solana transaction request is invalid".to_owned(),
                class: SolanaErrorClass::Terminal,
                raw_provider_error: raw_provider_error.clone(),
            };
        }

        NormalizedSolanaError {
            code: "solana.unknown_error".to_owned(),
            message: "solana provider returned an unknown error".to_owned(),
            class: SolanaErrorClass::ManualInterventionRequired,
            raw_provider_error: raw_provider_error.clone(),
        }
    }

    async fn execute_direct(
        &self,
        request: &AdapterExecutionRequest,
        input: SolanaExecutionInput,
        resume_context: Option<&AdapterResumeContext>,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.ensure_solana_schema().await?;

        if let Some(context) = resume_context {
            if let Some(envelope) = self
                .try_resume_from_reference(&input.intent_id, context)
                .await?
            {
                return Ok(envelope);
            }
        }

        self.upsert_intent_row(request, &input).await?;

        let rpc_urls = resolve_solana_rpc_urls(input.rpc_url.as_deref());
        let rpc_url = primary_solana_rpc_url(&rpc_urls);
        let effective_platform_signing_enabled =
            platform_signing_enabled_for_request(request, &rpc_urls);
        let mut provider_used = input
            .provider_used
            .clone()
            .or_else(|| rpc_urls.first().cloned())
            .unwrap_or_else(|| rpc_url.clone());
        let cu_limit = normalize_cu_limit(input.cu_limit);
        let cu_price_micro_lamports = normalize_cu_price(input.cu_price_micro_lamports);
        let mut blockhash_used = input.blockhash_used.clone();
        let mut simulation_outcome = input.simulation_outcome.clone();

        let mut active_attempt = self
            .fetch_active_attempt_for_intent(&input.intent_id)
            .await?;
        if active_attempt.is_none() {
            let candidate_attempt_id = Uuid::new_v4();
            let created = self
                .create_attempt_row(
                    candidate_attempt_id,
                    request.tenant_id.as_str(),
                    &input.intent_id,
                    request.job_id.as_str(),
                    cu_limit,
                    cu_price_micro_lamports,
                    blockhash_used.as_deref(),
                    simulation_outcome.as_deref(),
                    Some(provider_used.as_str()),
                )
                .await?;
            if created {
                active_attempt = Some(ActiveAttemptRow {
                    attempt_id: candidate_attempt_id,
                    status: "created".to_owned(),
                    signature: None,
                    last_confirmation_status: None,
                    last_err_json: None,
                    blockhash_used: blockhash_used.clone(),
                    simulation_outcome: simulation_outcome.clone(),
                    provider_used: Some(provider_used.clone()),
                });
            } else {
                active_attempt = self
                    .fetch_active_attempt_for_intent(&input.intent_id)
                    .await?;
            }
        }

        if let Some(existing) = active_attempt.as_ref() {
            if blockhash_used.is_none() {
                blockhash_used = existing.blockhash_used.clone();
            }
            if simulation_outcome.is_none() {
                simulation_outcome = existing.simulation_outcome.clone();
            }
            if let Some(existing_provider) = existing.provider_used.as_deref() {
                provider_used = existing_provider.to_owned();
            }
        }

        let attempt_id = if let Some(existing) = active_attempt.as_ref() {
            existing.attempt_id
        } else {
            let fallback_attempt_id = Uuid::new_v4();
            let created = self
                .create_attempt_row(
                    fallback_attempt_id,
                    request.tenant_id.as_str(),
                    &input.intent_id,
                    request.job_id.as_str(),
                    cu_limit,
                    cu_price_micro_lamports,
                    blockhash_used.as_deref(),
                    simulation_outcome.as_deref(),
                    Some(provider_used.as_str()),
                )
                .await?;
            if created {
                fallback_attempt_id
            } else {
                active_attempt = self
                    .fetch_active_attempt_for_intent(&input.intent_id)
                    .await?;
                if let Some(existing) = active_attempt.as_ref() {
                    existing.attempt_id
                } else {
                    return Err(AdapterExecutionError::Transport(
                        "failed to acquire active attempt for intent".to_owned(),
                    ));
                }
            }
        };

        let mut details = BTreeMap::new();
        details.insert("tenant_id".to_owned(), request.tenant_id.to_string());
        details.insert("job_id".to_owned(), request.job_id.to_string());
        details.insert("intent_id".to_owned(), input.intent_id.clone());
        details.insert("intent_type".to_owned(), input.intent_type.clone());
        if let Some(from_addr) = input.from_addr.as_deref() {
            details.insert("from_addr".to_owned(), from_addr.to_owned());
        }
        details.insert("to_addr".to_owned(), input.to_addr.clone());
        details.insert("amount".to_owned(), input.amount.to_string());
        details.insert("asset".to_owned(), input.asset.clone());
        details.insert("program_id".to_owned(), input.program_id.clone());
        details.insert("action".to_owned(), input.action.clone());
        details.insert("attempt_id".to_owned(), attempt_id.to_string());
        details.insert("provider_used".to_owned(), provider_used.clone());
        details.insert("rpc_url".to_owned(), rpc_url.clone());
        details.insert("rpc_urls".to_owned(), rpc_urls.join(","));
        details.insert(
            "skip_preflight".to_owned(),
            input.skip_preflight.to_string(),
        );
        details.insert("cu_limit".to_owned(), cu_limit.to_string());
        details.insert(
            "cu_price_micro_lamports".to_owned(),
            cu_price_micro_lamports.to_string(),
        );
        let signed_tx_present = input.signed_tx_base64.is_some();
        let signing_mode = request
            .metadata
            .get("execution.signing_mode")
            .cloned()
            .unwrap_or_else(|| {
                if signed_tx_present {
                    "customer_signed".to_owned()
                } else {
                    "platform_sponsored".to_owned()
                }
            });
        let payer_source = request
            .metadata
            .get("execution.payer_source")
            .cloned()
            .unwrap_or_else(|| {
                if signed_tx_present {
                    "customer_wallet".to_owned()
                } else {
                    "platform_sponsored".to_owned()
                }
            });
        let fee_payer = request
            .metadata
            .get("execution.fee_payer")
            .cloned()
            .or_else(|| {
                extract_detail_string(
                    &request.payload,
                    &[
                        "fee_payer",
                        "payer",
                        "from_addr",
                        "from",
                        "payer_address",
                        "fee_payer_address",
                    ],
                )
            })
            .unwrap_or_else(|| "unknown".to_owned());
        details.insert("signing_mode".to_owned(), signing_mode);
        details.insert("payer_source".to_owned(), payer_source);
        details.insert("fee_payer".to_owned(), fee_payer);
        details.insert(
            "platform_signing_enabled".to_owned(),
            effective_platform_signing_enabled.to_string(),
        );
        details.insert(
            "platform_signing_global_enabled".to_owned(),
            platform_signing_enabled().to_string(),
        );

        if let Some(demo_envelope) = self
            .maybe_execute_playground_demo(
                request,
                &input,
                attempt_id,
                blockhash_used.as_deref(),
                simulation_outcome.as_deref(),
                Some(provider_used.as_str()),
                &details,
            )
            .await?
        {
            return Ok(demo_envelope);
        }

        if let Some(existing) = active_attempt.as_ref() {
            details.insert("attempt_reused".to_owned(), "true".to_owned());
            details.insert("attempt_status".to_owned(), existing.status.clone());
            if let Some(last_confirmation_status) = existing.last_confirmation_status.as_deref() {
                details.insert(
                    "last_confirmation_status".to_owned(),
                    last_confirmation_status.to_owned(),
                );
            }
            if let Some(last_err_json) = existing.last_err_json.as_ref() {
                details.insert(
                    "raw_provider_error".to_owned(),
                    truncate_detail(last_err_json.to_string(), 512),
                );
            }
            if existing.status == "sent" {
                if let Some(signature) = existing.signature.as_deref() {
                    details.insert("signature".to_owned(), signature.to_owned());
                    details.insert("tx_hash".to_owned(), signature.to_owned());
                    if let Some(blockhash_used) = blockhash_used.as_deref() {
                        details.insert("blockhash_used".to_owned(), blockhash_used.to_owned());
                    }
                    if let Some(simulation_outcome) = simulation_outcome.as_deref() {
                        details.insert(
                            "simulation_outcome".to_owned(),
                            simulation_outcome.to_owned(),
                        );
                    }
                    return self
                        .poll_signature_until_decision(
                            &input.intent_id,
                            existing.attempt_id,
                            signature,
                            &rpc_urls,
                            &provider_used,
                            blockhash_used.as_deref(),
                            simulation_outcome.as_deref(),
                            details,
                        )
                        .await;
                }

                details.insert("phase".to_owned(), "await_signature".to_owned());
                return Ok(build_success_envelope(
                    AdapterProgressState::Submitted,
                    "solana.awaiting_signature",
                    "transaction submission is in progress",
                    Some(existing.attempt_id.to_string()),
                    details,
                ));
            }
        } else {
            details.insert("attempt_reused".to_owned(), "false".to_owned());
        }

        let mut signed_tx_base64 = input.signed_tx_base64.clone();
        if signed_tx_base64.is_none() {
            if !effective_platform_signing_enabled {
                let raw_error = json!({
                    "code": "SOLANA_PLATFORM_SIGNING_DISABLED",
                    "message": "platform signing is disabled"
                });
                self.mark_attempt_terminal(
                    attempt_id,
                    &input.intent_id,
                    None,
                    raw_error.clone(),
                    blockhash_used.as_deref(),
                    simulation_outcome.as_deref(),
                    Some(provider_used.as_str()),
                )
                .await?;

                let mut err_details = details.clone();
                err_details.insert("phase".to_owned(), "sign_policy".to_owned());
                return Ok(AdapterExecutionEnvelope {
                    status: AdapterStatusSnapshot {
                        state: AdapterProgressState::Blocked,
                        code: "solana.platform_signing_disabled".to_owned(),
                        message:
                            "platform signing is disabled; provide `signed_tx_base64` from customer signer."
                                .to_owned(),
                        provider_reference: Some(attempt_id.to_string()),
                        details: err_details,
                    },
                    outcome: AdapterOutcome::Blocked {
                        code: "solana.platform_signing_disabled".to_owned(),
                        message:
                            "platform signing is disabled; signed transaction payload is required."
                                .to_owned(),
                    },
                });
            }
            let latest_blockhash = match rpc_get_latest_blockhash_with_failover(
                &preferred_solana_rpc_urls(Some(provider_used.as_str()), &rpc_urls),
            )
            .await
            {
                Ok(result) => {
                    provider_used = result.provider_used;
                    details.insert("provider_used".to_owned(), provider_used.clone());
                    result.value
                }
                Err(err) => {
                    let normalized = normalized_from_rpc_call_error(self, err.clone());
                    let mut err_details = details.clone();
                    err_details.insert("phase".to_owned(), "get_latest_blockhash".to_owned());
                    self.mark_attempt_expired(
                        attempt_id,
                        normalized.raw_provider_error.clone(),
                        blockhash_used.as_deref(),
                        simulation_outcome.as_deref(),
                        Some(provider_used.as_str()),
                    )
                    .await?;
                    return Ok(build_failure_envelope(normalized, None, err_details));
                }
            };

            blockhash_used = Some(latest_blockhash.clone());
            self.update_attempt_metadata(
                attempt_id,
                blockhash_used.as_deref(),
                simulation_outcome.as_deref(),
                Some(provider_used.as_str()),
            )
            .await?;

            let signed_tx = match sign_transfer_with_node(
                &input.to_addr,
                input.amount,
                &latest_blockhash,
                cu_limit,
                cu_price_micro_lamports,
            ) {
                Ok(signed_tx) => signed_tx,
                Err(signing_message) => {
                    let raw_error = json!({ "signing_error": signing_message });
                    self.mark_attempt_terminal(
                        attempt_id,
                        &input.intent_id,
                        None,
                        raw_error.clone(),
                        blockhash_used.as_deref(),
                        simulation_outcome.as_deref(),
                        Some(provider_used.as_str()),
                    )
                    .await?;

                    let mut err_details = details.clone();
                    err_details.insert("phase".to_owned(), "sign".to_owned());
                    return Ok(AdapterExecutionEnvelope {
                        status: AdapterStatusSnapshot {
                            state: AdapterProgressState::Blocked,
                            code: "solana.signing_unavailable".to_owned(),
                            message: "solana signing failed or signer is unavailable".to_owned(),
                            provider_reference: None,
                            details: err_details,
                        },
                        outcome: AdapterOutcome::Blocked {
                            code: "solana.signing_unavailable".to_owned(),
                            message: "solana signing failed or signer is unavailable".to_owned(),
                        },
                    });
                }
            };

            signed_tx_base64 = Some(signed_tx);
        }

        let signed_tx_base64 = signed_tx_base64.ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "missing signed transaction payload after signing flow".to_owned(),
            )
        })?;

        if !input.skip_preflight && simulation_outcome.is_none() {
            match rpc_simulate_transaction_with_failover(
                &preferred_solana_rpc_urls(Some(provider_used.as_str()), &rpc_urls),
                &signed_tx_base64,
            )
            .await
            {
                Ok(result) => {
                    provider_used = result.provider_used;
                    details.insert("provider_used".to_owned(), provider_used.clone());
                    let simulation_result = result.value;
                    simulation_outcome = Some(simulation_result.outcome.clone());
                    self.update_attempt_metadata(
                        attempt_id,
                        blockhash_used.as_deref(),
                        simulation_outcome.as_deref(),
                        Some(provider_used.as_str()),
                    )
                    .await?;

                    if let Some(err_json) = simulation_result.err {
                        let normalized = self.normalize_solana_error(&err_json);
                        match normalized.class {
                            SolanaErrorClass::Retryable => {
                                self.mark_attempt_expired(
                                    attempt_id,
                                    err_json.clone(),
                                    blockhash_used.as_deref(),
                                    simulation_outcome.as_deref(),
                                    Some(provider_used.as_str()),
                                )
                                .await?;
                            }
                            _ => {
                                self.mark_attempt_terminal(
                                    attempt_id,
                                    &input.intent_id,
                                    None,
                                    err_json.clone(),
                                    blockhash_used.as_deref(),
                                    simulation_outcome.as_deref(),
                                    Some(provider_used.as_str()),
                                )
                                .await?;
                            }
                        }

                        let mut err_details = details.clone();
                        err_details.insert("phase".to_owned(), "simulate".to_owned());
                        if let Some(simulation_outcome) = simulation_outcome.as_deref() {
                            err_details.insert(
                                "simulation_outcome".to_owned(),
                                simulation_outcome.to_owned(),
                            );
                        }
                        return Ok(build_failure_envelope(normalized, None, err_details));
                    }
                }
                Err(err) => {
                    let normalized = normalized_from_rpc_call_error(self, err.clone());
                    self.mark_attempt_expired(
                        attempt_id,
                        normalized.raw_provider_error.clone(),
                        blockhash_used.as_deref(),
                        simulation_outcome.as_deref(),
                        Some(provider_used.as_str()),
                    )
                    .await?;
                    let mut err_details = details.clone();
                    err_details.insert("phase".to_owned(), "simulate".to_owned());
                    return Ok(build_failure_envelope(normalized, None, err_details));
                }
            }
        } else {
            self.update_attempt_metadata(
                attempt_id,
                blockhash_used.as_deref(),
                simulation_outcome.as_deref(),
                Some(provider_used.as_str()),
            )
            .await?;
        }

        let signature = match rpc_send_transaction_with_failover(
            &preferred_solana_rpc_urls(Some(provider_used.as_str()), &rpc_urls),
            &signed_tx_base64,
            input.skip_preflight,
        )
        .await
        {
                Ok(result) => {
                    provider_used = result.provider_used;
                    details.insert("provider_used".to_owned(), provider_used.clone());
                    result.value
                }
                Err(err) => {
                    let normalized = normalized_from_rpc_call_error(self, err.clone());
                    match normalized.class {
                        SolanaErrorClass::Retryable => {
                            self.mark_attempt_expired(
                                attempt_id,
                                normalized.raw_provider_error.clone(),
                                blockhash_used.as_deref(),
                                simulation_outcome.as_deref(),
                                Some(provider_used.as_str()),
                            )
                            .await?;
                        }
                        _ => {
                            self.mark_attempt_terminal(
                                attempt_id,
                                &input.intent_id,
                                None,
                                normalized.raw_provider_error.clone(),
                                blockhash_used.as_deref(),
                                simulation_outcome.as_deref(),
                                Some(provider_used.as_str()),
                            )
                            .await?;
                        }
                    }

                    let mut err_details = details.clone();
                    err_details.insert("phase".to_owned(), "send_transaction".to_owned());
                    return Ok(build_failure_envelope(normalized, None, err_details));
                }
            };

        self.mark_attempt_sent(
            attempt_id,
            &signature,
            blockhash_used.as_deref(),
            simulation_outcome.as_deref(),
            Some(provider_used.as_str()),
        )
        .await?;

        details.insert("signature".to_owned(), signature.clone());
        details.insert("tx_hash".to_owned(), signature.clone());
        if let Some(blockhash_used) = blockhash_used.as_deref() {
            details.insert("blockhash_used".to_owned(), blockhash_used.to_owned());
        }
        if let Some(simulation_outcome) = simulation_outcome.as_deref() {
            details.insert(
                "simulation_outcome".to_owned(),
                simulation_outcome.to_owned(),
            );
        }

        self.poll_signature_until_decision(
            &input.intent_id,
            attempt_id,
            &signature,
            &rpc_urls,
            &provider_used,
            blockhash_used.as_deref(),
            simulation_outcome.as_deref(),
            details,
        )
        .await
    }

    async fn ensure_solana_schema(&self) -> Result<(), AdapterExecutionError> {
        ensure_solana_schema_with_pool(&self.pool).await
    }

    async fn try_resume_from_reference(
        &self,
        intent_id: &str,
        context: &AdapterResumeContext,
    ) -> Result<Option<AdapterExecutionEnvelope>, AdapterExecutionError> {
        let Some(reference) = context
            .previous_provider_reference
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            return Ok(None);
        };

        if let Ok(attempt_id) = Uuid::parse_str(reference) {
            if let Some(row) = self.fetch_status_row_by_attempt_id(attempt_id).await? {
                let status = self.status_from_row(row, Some(reference.to_owned()));
                return Ok(Some(snapshot_to_envelope(status)));
            }
        }

        match self.fetch_status_row_by_intent_id(intent_id).await {
            Ok(row) => {
                let status = self.status_from_row(row, Some(reference.to_owned()));
                Ok(Some(snapshot_to_envelope(status)))
            }
            Err(AdapterExecutionError::Unavailable(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn upsert_intent_row(
        &self,
        request: &AdapterExecutionRequest,
        input: &SolanaExecutionInput,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            INSERT INTO solana.tx_intents (
                id, tenant_id, job_id, intent_type, from_addr, to_addr, amount, asset, program_id, action, status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'received')
            ON CONFLICT (id) DO UPDATE
               SET tenant_id = EXCLUDED.tenant_id,
                   job_id = EXCLUDED.job_id,
                   intent_type = EXCLUDED.intent_type,
                   from_addr = EXCLUDED.from_addr,
                   to_addr = EXCLUDED.to_addr,
                   amount = EXCLUDED.amount,
                   asset = EXCLUDED.asset,
                   program_id = EXCLUDED.program_id,
                   action = EXCLUDED.action,
                   status = CASE
                     WHEN solana.tx_intents.status = 'finalized' THEN 'finalized'
                     ELSE 'received'
                   END,
                   updated_at = now()
            "#,
        )
        .bind(&input.intent_id)
        .bind(request.tenant_id.as_str())
        .bind(request.job_id.as_str())
        .bind(&input.intent_type)
        .bind(input.from_addr.as_deref())
        .bind(&input.to_addr)
        .bind(input.amount)
        .bind(&input.asset)
        .bind(&input.program_id)
        .bind(&input.action)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("upsert solana intent", err))?;

        Ok(())
    }

    async fn create_attempt_row(
        &self,
        attempt_id: Uuid,
        tenant_id: &str,
        intent_id: &str,
        job_id: &str,
        cu_limit: i32,
        cu_price_micro_lamports: i64,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<bool, AdapterExecutionError> {
        match sqlx::query(
            r#"
            INSERT INTO solana.tx_attempts (
                id,
                tenant_id,
                intent_id,
                job_id,
                status,
                cu_limit,
                cu_price_micro_lamports,
                blockhash_used,
                simulation_outcome,
                provider_used
            )
            VALUES ($1, $2, $3, $4, 'created', $5, $6, $7, $8, $9)
            "#,
        )
        .bind(attempt_id)
        .bind(tenant_id)
        .bind(intent_id)
        .bind(job_id)
        .bind(cu_limit)
        .bind(cu_price_micro_lamports)
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        {
            Ok(_) => Ok(true),
            Err(err) if is_sqlstate(&err, "23505") => Ok(false),
            Err(err) => Err(map_db_error("create solana attempt", err)),
        }
    }

    async fn count_attempt_rows_for_intent(
        &self,
        intent_id: &str,
    ) -> Result<i64, AdapterExecutionError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM solana.tx_attempts
            WHERE intent_id = $1
            "#,
        )
        .bind(intent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|err| map_db_error("count solana attempt rows", err))?;
        Ok(count.max(0))
    }

    async fn maybe_execute_playground_demo(
        &self,
        request: &AdapterExecutionRequest,
        input: &SolanaExecutionInput,
        attempt_id: Uuid,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
        details: &BTreeMap<String, String>,
    ) -> Result<Option<AdapterExecutionEnvelope>, AdapterExecutionError> {
        if !playground_demo_scenarios_enabled() {
            return Ok(None);
        }

        if !is_playground_internal_request(request) {
            return Ok(None);
        }

        let scenario = playground_demo_scenario(request);
        if matches!(scenario, PlaygroundDemoScenario::Real) {
            return Ok(None);
        }

        let mut demo_details = details.clone();
        demo_details.insert("playground_demo_scenario".to_owned(), scenario.as_str().to_owned());
        let attempt_count = self.count_attempt_rows_for_intent(&input.intent_id).await?;
        demo_details.insert(
            "playground_demo_attempt_count".to_owned(),
            attempt_count.to_string(),
        );

        match scenario {
            PlaygroundDemoScenario::SyntheticSuccess => {
                let signature = synthetic_demo_signature(attempt_id);
                self.mark_attempt_finalized_success(
                    attempt_id,
                    &input.intent_id,
                    &signature,
                    blockhash_used,
                    simulation_outcome,
                    provider_used,
                )
                .await?;
                demo_details.insert("signature".to_owned(), signature.clone());
                demo_details.insert("tx_hash".to_owned(), signature.clone());

                Ok(Some(build_success_envelope(
                    AdapterProgressState::Finalized,
                    "solana.playground_demo_success",
                    "playground synthetic success committed",
                    Some(attempt_id.to_string()),
                    demo_details,
                )))
            }
            PlaygroundDemoScenario::RetryThenSuccess => {
                if attempt_count <= 1 {
                    let raw_error = json!({
                        "code": "SOLANA_PLAYGROUND_RETRY_DEMO",
                        "message": "synthetic retry requested for playground demo"
                    });
                    self.mark_attempt_expired(
                        attempt_id,
                        raw_error.clone(),
                        blockhash_used,
                        simulation_outcome,
                        provider_used,
                    )
                    .await?;
                    demo_details.insert("playground_demo_phase".to_owned(), "retry".to_owned());
                    Ok(Some(AdapterExecutionEnvelope {
                        status: AdapterStatusSnapshot {
                            state: AdapterProgressState::FailedRetryable,
                            code: "solana.playground_retry_demo".to_owned(),
                            message: "playground demo generated a retryable failure".to_owned(),
                            provider_reference: Some(attempt_id.to_string()),
                            details: demo_details,
                        },
                        outcome: AdapterOutcome::RetryableFailure {
                            code: "solana.playground_retry_demo".to_owned(),
                            message: "playground demo generated a retryable failure".to_owned(),
                            retry_after_ms: Some(1200),
                            provider_details: Some(raw_error),
                        },
                    }))
                } else {
                    let signature = synthetic_demo_signature(attempt_id);
                    self.mark_attempt_finalized_success(
                        attempt_id,
                        &input.intent_id,
                        &signature,
                        blockhash_used,
                        simulation_outcome,
                        provider_used,
                    )
                    .await?;
                    demo_details.insert("playground_demo_phase".to_owned(), "recovered".to_owned());
                    demo_details.insert("signature".to_owned(), signature.clone());
                    demo_details.insert("tx_hash".to_owned(), signature.clone());

                    Ok(Some(build_success_envelope(
                        AdapterProgressState::Finalized,
                        "solana.playground_retry_demo_recovered",
                        "playground demo completed after one retry",
                        Some(attempt_id.to_string()),
                        demo_details,
                    )))
                }
            }
            PlaygroundDemoScenario::TerminalFailure => {
                let raw_error = json!({
                    "code": "SOLANA_PLAYGROUND_TERMINAL_DEMO",
                    "message": "synthetic terminal failure requested for playground demo"
                });
                self.mark_attempt_terminal(
                    attempt_id,
                    &input.intent_id,
                    None,
                    raw_error,
                    blockhash_used,
                    simulation_outcome,
                    provider_used,
                )
                .await?;
                demo_details.insert(
                    "playground_demo_phase".to_owned(),
                    "terminal_failure".to_owned(),
                );
                Ok(Some(AdapterExecutionEnvelope {
                    status: AdapterStatusSnapshot {
                        state: AdapterProgressState::FailedTerminal,
                        code: "solana.playground_terminal_demo".to_owned(),
                        message: "playground demo generated a terminal failure".to_owned(),
                        provider_reference: Some(attempt_id.to_string()),
                        details: demo_details,
                    },
                    outcome: AdapterOutcome::TerminalFailure {
                        code: "solana.playground_terminal_demo".to_owned(),
                        message: "playground demo generated a terminal failure".to_owned(),
                        provider_details: Some(json!({
                            "code": "SOLANA_PLAYGROUND_TERMINAL_DEMO"
                        })),
                    },
                }))
            }
            PlaygroundDemoScenario::Real => Ok(None),
        }
    }

    async fn fetch_active_attempt_for_intent(
        &self,
        intent_id: &str,
    ) -> Result<Option<ActiveAttemptRow>, AdapterExecutionError> {
        match self
            .fetch_active_attempt_for_intent_enhanced(intent_id)
            .await
        {
            Ok(row) => Ok(row),
            Err(err) if is_sqlstate(&err, "42703") => self
                .fetch_active_attempt_for_intent_basic(intent_id)
                .await
                .map_err(|err| {
                    AdapterExecutionError::Transport(format!(
                        "fetch active solana attempt fallback failed: {err}"
                    ))
                }),
            Err(err) if is_sqlstate(&err, "42P01") => Err(AdapterExecutionError::Unavailable(
                "solana attempt table is not available".to_owned(),
            )),
            Err(err) => Err(AdapterExecutionError::Transport(format!(
                "fetch active solana attempt failed: {err}"
            ))),
        }
    }

    async fn fetch_active_attempt_for_intent_enhanced(
        &self,
        intent_id: &str,
    ) -> Result<Option<ActiveAttemptRow>, sqlx::Error> {
        type Row = (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            Option<Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        );

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                a.id,
                a.status,
                a.signature,
                a.last_confirmation_status,
                a.last_err_json,
                a.blockhash_used,
                a.simulation_outcome,
                COALESCE(
                    a.provider_used,
                    to_jsonb(a)->>'provider',
                    to_jsonb(a)->>'rpc_url'
                ) AS provider_used
            FROM solana.tx_attempts a
            WHERE a.intent_id = $1
              AND a.status IN ('created', 'sent')
            ORDER BY a.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(ActiveAttemptRow::from_enhanced))
    }

    async fn fetch_active_attempt_for_intent_basic(
        &self,
        intent_id: &str,
    ) -> Result<Option<ActiveAttemptRow>, sqlx::Error> {
        type Row = (Uuid, String, Option<String>);

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                a.id,
                a.status,
                a.signature
            FROM solana.tx_attempts a
            WHERE a.intent_id = $1
              AND a.status IN ('created', 'sent')
            ORDER BY a.created_at DESC
            LIMIT 1
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(ActiveAttemptRow::from_basic))
    }

    async fn update_attempt_metadata(
        &self,
        attempt_id: Uuid,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET blockhash_used = COALESCE($2, blockhash_used),
                simulation_outcome = COALESCE($3, simulation_outcome),
                provider_used = COALESCE($4, provider_used),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("update solana attempt metadata", err))?;

        Ok(())
    }

    async fn mark_attempt_sent(
        &self,
        attempt_id: Uuid,
        signature: &str,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET status = 'sent',
                signature = $2,
                blockhash_used = COALESCE($3, blockhash_used),
                simulation_outcome = COALESCE($4, simulation_outcome),
                provider_used = COALESCE($5, provider_used),
                last_checked_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(signature)
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana attempt sent", err))?;

        Ok(())
    }

    async fn record_attempt_check(
        &self,
        attempt_id: Uuid,
        confirmation_status: Option<&str>,
        err_json: Option<Value>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET poll_no = poll_no + 1,
                last_confirmation_status = COALESCE($2, last_confirmation_status),
                last_err_json = $3,
                provider_used = COALESCE($4, provider_used),
                last_checked_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(confirmation_status)
        .bind(err_json)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("record solana attempt check", err))?;

        Ok(())
    }

    async fn mark_attempt_expired(
        &self,
        attempt_id: Uuid,
        err_json: Value,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET status = 'expired',
                last_err_json = $2,
                blockhash_used = COALESCE($3, blockhash_used),
                simulation_outcome = COALESCE($4, simulation_outcome),
                provider_used = COALESCE($5, provider_used),
                last_checked_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(err_json)
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana attempt expired", err))?;

        Ok(())
    }

    async fn mark_attempt_terminal(
        &self,
        attempt_id: Uuid,
        intent_id: &str,
        signature: Option<&str>,
        err_json: Value,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET status = 'finalized',
                signature = COALESCE($2, signature),
                last_err_json = $3,
                blockhash_used = COALESCE($4, blockhash_used),
                simulation_outcome = COALESCE($5, simulation_outcome),
                provider_used = COALESCE($6, provider_used),
                last_checked_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(signature)
        .bind(err_json.clone())
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana attempt terminal", err))?;

        sqlx::query(
            r#"
            UPDATE solana.tx_intents
            SET status = 'finalized',
                final_signature = COALESCE($2, final_signature),
                final_err_json = $3,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(intent_id)
        .bind(signature)
        .bind(err_json)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana intent terminal", err))?;

        Ok(())
    }

    async fn mark_attempt_finalized_success(
        &self,
        attempt_id: Uuid,
        intent_id: &str,
        signature: &str,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        provider_used: Option<&str>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            UPDATE solana.tx_attempts
            SET status = 'finalized',
                signature = $2,
                last_confirmation_status = 'finalized',
                last_err_json = NULL,
                blockhash_used = COALESCE($3, blockhash_used),
                simulation_outcome = COALESCE($4, simulation_outcome),
                provider_used = COALESCE($5, provider_used),
                last_checked_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(signature)
        .bind(blockhash_used)
        .bind(simulation_outcome)
        .bind(provider_used)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana attempt finalized success", err))?;

        sqlx::query(
            r#"
            UPDATE solana.tx_intents
            SET status = 'finalized',
                final_signature = $2,
                final_err_json = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(intent_id)
        .bind(signature)
        .execute(&self.pool)
        .await
        .map_err(|err| map_db_error("mark solana intent finalized success", err))?;

        Ok(())
    }

    async fn poll_signature_until_decision(
        &self,
        intent_id: &str,
        attempt_id: Uuid,
        signature: &str,
        rpc_urls: &[String],
        provider_used: &str,
        blockhash_used: Option<&str>,
        simulation_outcome: Option<&str>,
        mut details: BTreeMap<String, String>,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        let max_polls = self.sync_max_polls();
        let poll_delay = self.sync_poll_delay();
        let mut landed_seen = false;
        let mut current_provider_used = provider_used.to_owned();

        for poll in 0..max_polls {
            let response = rpc_get_signature_status_with_failover(
                &preferred_solana_rpc_urls(Some(current_provider_used.as_str()), rpc_urls),
                signature,
            )
            .await;
            match response {
                Ok(result) if result.value.is_none() => {
                    current_provider_used = result.provider_used;
                    details.insert("provider_used".to_owned(), current_provider_used.clone());
                    self.record_attempt_check(
                        attempt_id,
                        None,
                        None,
                        Some(current_provider_used.as_str()),
                    )
                        .await?;
                }
                Ok(result) => {
                    current_provider_used = result.provider_used;
                    details.insert("provider_used".to_owned(), current_provider_used.clone());
                    let status = result.value.expect("status present");
                    self.record_attempt_check(
                        attempt_id,
                        status.confirmation_status.as_deref(),
                        status.err.clone(),
                        Some(current_provider_used.as_str()),
                    )
                    .await?;

                    if let Some(err_json) = status.err.clone() {
                        let normalized = self.normalize_solana_error(&err_json);
                        match normalized.class {
                            SolanaErrorClass::Retryable => {
                                self.mark_attempt_expired(
                                    attempt_id,
                                    err_json,
                                    blockhash_used,
                                    simulation_outcome,
                                    Some(current_provider_used.as_str()),
                                )
                                .await?;
                            }
                            _ => {
                                self.mark_attempt_terminal(
                                    attempt_id,
                                    intent_id,
                                    Some(signature),
                                    err_json,
                                    blockhash_used,
                                    simulation_outcome,
                                    Some(current_provider_used.as_str()),
                                )
                                .await?;
                            }
                        }

                        details.insert("phase".to_owned(), "track_status".to_owned());
                        return Ok(build_failure_envelope(
                            normalized,
                            Some(signature.to_owned()),
                            details,
                        ));
                    }

                    if status
                        .confirmation_status
                        .as_deref()
                        .is_some_and(is_landed_confirmation_status)
                    {
                        landed_seen = true;
                    }

                    if status.confirmation_status.as_deref() == Some("finalized") {
                        self.mark_attempt_finalized_success(
                            attempt_id,
                            intent_id,
                            signature,
                            blockhash_used,
                            simulation_outcome,
                            Some(current_provider_used.as_str()),
                        )
                        .await?;
                        details.insert("confirmation_status".to_owned(), "finalized".to_owned());
                        return Ok(build_success_envelope(
                            AdapterProgressState::Finalized,
                            "solana.finalized",
                            "solana transaction finalized",
                            Some(signature.to_owned()),
                            details,
                        ));
                    }
                }
                Err(err) => {
                    let normalized = normalized_from_rpc_call_error(self, err.clone());
                    self.record_attempt_check(
                        attempt_id,
                        None,
                        Some(normalized.raw_provider_error.clone()),
                        Some(current_provider_used.as_str()),
                    )
                    .await?;
                    details.insert("phase".to_owned(), "track_status".to_owned());
                    return Ok(build_failure_envelope(
                        normalized,
                        Some(signature.to_owned()),
                        details,
                    ));
                }
            }

            if poll + 1 < max_polls {
                sleep(poll_delay).await;
            }
        }

        let (state, code, message) = if landed_seen {
            (
                AdapterProgressState::Landed,
                "solana.landed",
                "transaction landed; waiting for finalization",
            )
        } else {
            (
                AdapterProgressState::Confirming,
                "solana.confirming",
                "transaction submitted; waiting for finality",
            )
        };

        Ok(build_success_envelope(
            state,
            code,
            message,
            Some(signature.to_owned()),
            details,
        ))
    }

    async fn fetch_status_row_by_attempt_id(
        &self,
        attempt_id: Uuid,
    ) -> Result<Option<SolanaStatusRow>, AdapterExecutionError> {
        match self
            .fetch_status_row_by_attempt_id_enhanced(attempt_id)
            .await
        {
            Ok(row) => Ok(row),
            Err(err) if is_sqlstate(&err, "42703") => self
                .fetch_status_row_by_attempt_id_basic(attempt_id)
                .await
                .map_err(|e| {
                    AdapterExecutionError::Transport(format!(
                        "fetch solana status by attempt_id fallback failed: {e}"
                    ))
                }),
            Err(err) if is_sqlstate(&err, "42P01") => Err(AdapterExecutionError::Unavailable(
                "solana status tables are not available".to_owned(),
            )),
            Err(err) => Err(AdapterExecutionError::Transport(format!(
                "fetch solana status by attempt_id failed: {err}"
            ))),
        }
    }

    async fn fetch_status_row_by_intent_id(
        &self,
        intent_id: &str,
    ) -> Result<SolanaStatusRow, AdapterExecutionError> {
        let maybe_row = match self.fetch_status_row_by_intent_id_enhanced(intent_id).await {
            Ok(row) => row,
            Err(err) if is_sqlstate(&err, "42703") => self
                .fetch_status_row_by_intent_id_basic(intent_id)
                .await
                .map_err(|e| {
                    AdapterExecutionError::Transport(format!(
                        "fetch solana status by intent_id fallback failed: {e}"
                    ))
                })?,
            Err(err) if is_sqlstate(&err, "42P01") => {
                return Err(AdapterExecutionError::Unavailable(
                    "solana status tables are not available".to_owned(),
                ));
            }
            Err(err) => {
                return Err(AdapterExecutionError::Transport(format!(
                    "fetch solana status by intent_id failed: {err}"
                )));
            }
        };

        maybe_row.ok_or_else(|| {
            AdapterExecutionError::Unavailable(format!("solana intent `{intent_id}` not found"))
        })
    }

    async fn fetch_status_row_by_attempt_id_enhanced(
        &self,
        attempt_id: Uuid,
    ) -> Result<Option<SolanaStatusRow>, sqlx::Error> {
        type Row = (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<Value>,
            Option<Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        );

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                i.id,
                i.status,
                i.final_signature,
                a.id,
                a.status,
                a.signature,
                a.last_confirmation_status,
                a.last_err_json,
                i.final_err_json,
                to_jsonb(a)->>'blockhash_used',
                to_jsonb(a)->>'simulation_outcome',
                COALESCE(to_jsonb(a)->>'provider_used', to_jsonb(a)->>'provider', to_jsonb(a)->>'rpc_url')
            FROM solana.tx_attempts a
            JOIN solana.tx_intents i ON i.id = a.intent_id
            WHERE a.id = $1
            "#,
        )
        .bind(attempt_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(SolanaStatusRow::from_enhanced))
    }

    async fn fetch_status_row_by_attempt_id_basic(
        &self,
        attempt_id: Uuid,
    ) -> Result<Option<SolanaStatusRow>, sqlx::Error> {
        type Row = (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
        );

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                i.id,
                i.status,
                i.final_signature,
                a.id,
                a.status,
                a.signature
            FROM solana.tx_attempts a
            JOIN solana.tx_intents i ON i.id = a.intent_id
            WHERE a.id = $1
            "#,
        )
        .bind(attempt_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(SolanaStatusRow::from_basic))
    }

    async fn fetch_status_row_by_intent_id_enhanced(
        &self,
        intent_id: &str,
    ) -> Result<Option<SolanaStatusRow>, sqlx::Error> {
        type Row = (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<Value>,
            Option<Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        );

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                i.id,
                i.status,
                i.final_signature,
                a.id,
                a.status,
                a.signature,
                a.last_confirmation_status,
                a.last_err_json,
                i.final_err_json,
                a.blockhash_used,
                a.simulation_outcome,
                a.provider_used
            FROM solana.tx_intents i
            LEFT JOIN LATERAL (
                SELECT
                    id,
                    status,
                    signature,
                    last_confirmation_status,
                    last_err_json,
                    to_jsonb(sol_attempt)->>'blockhash_used' AS blockhash_used,
                    to_jsonb(sol_attempt)->>'simulation_outcome' AS simulation_outcome,
                    COALESCE(
                        to_jsonb(sol_attempt)->>'provider_used',
                        to_jsonb(sol_attempt)->>'provider',
                        to_jsonb(sol_attempt)->>'rpc_url'
                    ) AS provider_used
                FROM solana.tx_attempts sol_attempt
                WHERE sol_attempt.intent_id = i.id
                ORDER BY sol_attempt.created_at DESC
                LIMIT 1
            ) a ON TRUE
            WHERE i.id = $1
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(SolanaStatusRow::from_enhanced))
    }

    async fn fetch_status_row_by_intent_id_basic(
        &self,
        intent_id: &str,
    ) -> Result<Option<SolanaStatusRow>, sqlx::Error> {
        type Row = (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
        );

        sqlx::query_as::<_, Row>(
            r#"
            SELECT
                i.id,
                i.status,
                i.final_signature,
                a.id,
                a.status,
                a.signature
            FROM solana.tx_intents i
            LEFT JOIN LATERAL (
                SELECT id, status, signature
                FROM solana.tx_attempts
                WHERE intent_id = i.id
                ORDER BY created_at DESC
                LIMIT 1
            ) a ON TRUE
            WHERE i.id = $1
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(SolanaStatusRow::from_basic))
    }

    fn status_from_row(
        &self,
        row: SolanaStatusRow,
        fallback_provider_reference: Option<String>,
    ) -> AdapterStatusSnapshot {
        let mut details = BTreeMap::new();
        details.insert("intent_id".to_owned(), row.intent_id.clone());
        details.insert("intent_status".to_owned(), row.intent_status.clone());
        if let Some(attempt_id) = row.attempt_id {
            details.insert("attempt_id".to_owned(), attempt_id.to_string());
        }
        if let Some(attempt_status) = &row.attempt_status {
            details.insert("attempt_status".to_owned(), attempt_status.clone());
        }
        if let Some(last_confirmation_status) = &row.last_confirmation_status {
            details.insert(
                "last_confirmation_status".to_owned(),
                last_confirmation_status.clone(),
            );
        }
        if let Some(blockhash_used) = &row.blockhash_used {
            details.insert("blockhash_used".to_owned(), blockhash_used.clone());
        }
        if let Some(simulation_outcome) = &row.simulation_outcome {
            details.insert("simulation_outcome".to_owned(), simulation_outcome.clone());
        }
        if let Some(provider_used) = &row.provider_used {
            details.insert("provider_used".to_owned(), provider_used.clone());
        }

        let provider_reference = row
            .final_signature
            .clone()
            .or_else(|| row.attempt_signature.clone())
            .or(fallback_provider_reference);
        if let Some(signature) = provider_reference.as_deref() {
            details.insert("signature".to_owned(), signature.to_owned());
            details.insert("tx_hash".to_owned(), signature.to_owned());
        }

        if let Some(raw_error) = row.final_err_json.clone().or(row.last_err_json.clone()) {
            let normalized = self.normalize_solana_error(&raw_error);
            details.insert(
                "normalized_error_class".to_owned(),
                normalized.class.as_str().to_owned(),
            );
            details.insert(
                "raw_provider_error".to_owned(),
                truncate_detail(raw_error.to_string(), 512),
            );

            return match normalized.class {
                SolanaErrorClass::Retryable => AdapterStatusSnapshot {
                    state: AdapterProgressState::FailedRetryable,
                    code: normalized.code,
                    message: normalized.message,
                    provider_reference,
                    details,
                },
                SolanaErrorClass::Terminal => AdapterStatusSnapshot {
                    state: AdapterProgressState::FailedTerminal,
                    code: normalized.code,
                    message: normalized.message,
                    provider_reference,
                    details,
                },
                SolanaErrorClass::Blocked => AdapterStatusSnapshot {
                    state: AdapterProgressState::Blocked,
                    code: normalized.code,
                    message: normalized.message,
                    provider_reference,
                    details,
                },
                SolanaErrorClass::ManualInterventionRequired => AdapterStatusSnapshot {
                    state: AdapterProgressState::ManualInterventionRequired,
                    code: normalized.code,
                    message: normalized.message,
                    provider_reference,
                    details,
                },
            };
        }

        if row.intent_status == "finalized" {
            return AdapterStatusSnapshot {
                state: AdapterProgressState::Finalized,
                code: "solana.finalized".to_owned(),
                message: "solana transaction finalized".to_owned(),
                provider_reference,
                details,
            };
        }

        match row.attempt_status.as_deref() {
            None => AdapterStatusSnapshot {
                state: AdapterProgressState::Submitted,
                code: "solana.intent_received".to_owned(),
                message: "intent received; waiting for attempt creation".to_owned(),
                provider_reference,
                details,
            },
            Some("created") => AdapterStatusSnapshot {
                state: AdapterProgressState::Submitted,
                code: "solana.attempt_created".to_owned(),
                message: "attempt created; not yet submitted to RPC".to_owned(),
                provider_reference,
                details,
            },
            Some("sent") => {
                if row
                    .last_confirmation_status
                    .as_deref()
                    .is_some_and(is_landed_confirmation_status)
                {
                    AdapterStatusSnapshot {
                        state: AdapterProgressState::Landed,
                        code: "solana.landed".to_owned(),
                        message: "transaction landed; waiting for finalization".to_owned(),
                        provider_reference,
                        details,
                    }
                } else {
                    AdapterStatusSnapshot {
                        state: AdapterProgressState::Confirming,
                        code: "solana.confirming".to_owned(),
                        message: "transaction submitted; waiting for finality".to_owned(),
                        provider_reference,
                        details,
                    }
                }
            }
            Some("finalized") => {
                if provider_reference.is_some() {
                    AdapterStatusSnapshot {
                        state: AdapterProgressState::Finalized,
                        code: "solana.finalized".to_owned(),
                        message: "transaction finalized".to_owned(),
                        provider_reference,
                        details,
                    }
                } else {
                    AdapterStatusSnapshot {
                        state: AdapterProgressState::ManualInterventionRequired,
                        code: "solana.finalized_missing_signature".to_owned(),
                        message: "finalized state without signature reference".to_owned(),
                        provider_reference,
                        details,
                    }
                }
            }
            Some("expired") => AdapterStatusSnapshot {
                state: AdapterProgressState::FailedRetryable,
                code: "solana.blockhash_expired".to_owned(),
                message: "attempt expired; safe to retry".to_owned(),
                provider_reference,
                details,
            },
            Some(other) => {
                details.insert("unknown_attempt_status".to_owned(), other.to_owned());
                AdapterStatusSnapshot {
                    state: AdapterProgressState::ManualInterventionRequired,
                    code: "solana.unknown_attempt_status".to_owned(),
                    message: "unknown attempt status from solana engine".to_owned(),
                    provider_reference,
                    details,
                }
            }
        }
    }
}

#[async_trait]
impl AdapterExecutor for SolanaQueueAdapter {
    async fn execute(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<AdapterOutcome, AdapterExecutionError> {
        self.validate_intent(request)?;
        let envelope = self.execute_solana_intent(request).await?;
        Ok(envelope.outcome)
    }
}

pub fn register_default_solana_adapter(
    registry: &mut AdapterRegistry,
    adapter: Arc<SolanaQueueAdapter>,
) {
    let adapter_id = AdapterId::from("adapter_solana");
    registry.register_domain_adapter_for_intent(
        "solana.transfer.v1",
        adapter_id.clone(),
        "kind=solana.transfer.v1",
        adapter.clone(),
    );
    registry.register_domain_adapter_for_intent(
        "solana.broadcast.v1",
        adapter_id,
        "kind=solana.broadcast.v1",
        adapter,
    );
}

fn normalize_payload(request: &AdapterExecutionRequest) -> Result<Value, AdapterExecutionError> {
    let payload = &request.payload;

    let intent_id = payload
        .get("intent_id")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| request.intent_id.to_string());

    let intent_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "transfer".to_owned());

    let to_addr = payload
        .get("to_addr")
        .or_else(|| payload.get("to"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "solana payload must include `to_addr` or `to`".to_owned(),
            )
        })?
        .to_owned();

    let amount = payload
        .get("amount")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "solana payload must include integer `amount`".to_owned(),
            )
        })?;

    if amount <= 0 {
        return Err(AdapterExecutionError::ContractViolation(
            "solana payload `amount` must be a positive integer".to_owned(),
        ));
    }

    let mut out = serde_json::json!({
        "intent_id": intent_id,
        "type": intent_type,
        "to_addr": to_addr,
        "amount": amount
    });

    copy_optional(payload, &mut out, "signed_tx_base64");
    copy_optional(payload, &mut out, "skip_preflight");
    copy_optional(payload, &mut out, "cu_limit");
    copy_optional(payload, &mut out, "cu_price_micro_lamports");
    copy_optional(payload, &mut out, "blockhash_used");
    copy_optional(payload, &mut out, "simulation_outcome");
    copy_optional(payload, &mut out, "provider_used");
    copy_optional(payload, &mut out, "provider");
    copy_optional(payload, &mut out, "rpc_url");
    copy_optional(payload, &mut out, "from_addr");
    copy_optional(payload, &mut out, "from");
    copy_optional(payload, &mut out, "asset");
    copy_optional(payload, &mut out, "program");
    copy_optional(payload, &mut out, "program_id");
    copy_optional(payload, &mut out, "action");

    Ok(out)
}

fn parse_execution_input(payload: &Value) -> Result<SolanaExecutionInput, AdapterExecutionError> {
    let intent_id = payload
        .get("intent_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "solana payload must include non-empty `intent_id`".to_owned(),
            )
        })?
        .to_owned();

    let intent_type = payload
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("transfer")
        .to_owned();

    let to_addr = payload
        .get("to_addr")
        .or_else(|| payload.get("to"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "solana payload must include non-empty `to_addr` or `to`".to_owned(),
            )
        })?
        .to_owned();

    let from_addr = extract_detail_string(
        payload,
        &[
            "from_addr",
            "from",
            "fee_payer",
            "payer",
            "payer_address",
            "fee_payer_address",
        ],
    );

    let amount = payload
        .get("amount")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AdapterExecutionError::ContractViolation(
                "solana payload must include integer `amount`".to_owned(),
            )
        })?;

    if amount <= 0 {
        return Err(AdapterExecutionError::ContractViolation(
            "solana payload `amount` must be a positive integer".to_owned(),
        ));
    }

    let signed_tx_base64 = payload
        .get("signed_tx_base64")
        .or_else(|| payload.get("signed_tx_b64"))
        .or_else(|| payload.get("signed_tx"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let skip_preflight = payload
        .get("skip_preflight")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let cu_limit = payload
        .get("cu_limit")
        .and_then(Value::as_i64)
        .map(|raw| {
            i32::try_from(raw).map_err(|_| {
                AdapterExecutionError::ContractViolation(
                    "solana payload `cu_limit` must fit into 32-bit integer".to_owned(),
                )
            })
        })
        .transpose()?;

    let cu_price_micro_lamports = payload
        .get("cu_price_micro_lamports")
        .or_else(|| payload.get("cu_price"))
        .and_then(Value::as_i64);

    let blockhash_used = extract_detail_string(payload, &["blockhash_used", "blockhash"]);
    let simulation_outcome = extract_detail_string(payload, &["simulation_outcome"]);
    let provider_used = extract_detail_string(payload, &["provider_used", "provider"]);
    let rpc_url = extract_detail_string(payload, &["rpc_url"]);
    let asset = extract_detail_string(payload, &["asset", "mint"]).unwrap_or_else(|| "SOL".to_owned());
    let program_id = extract_detail_string(payload, &["program_id", "program"])
        .unwrap_or_else(|| "system_program".to_owned());
    let action = extract_detail_string(payload, &["action", "type"])
        .unwrap_or_else(|| intent_type.clone());

    Ok(SolanaExecutionInput {
        intent_id,
        intent_type,
        from_addr,
        to_addr,
        amount,
        asset,
        program_id,
        action,
        signed_tx_base64,
        skip_preflight,
        cu_limit,
        cu_price_micro_lamports,
        blockhash_used,
        simulation_outcome,
        provider_used,
        rpc_url,
    })
}

fn copy_optional(src: &Value, dst: &mut Value, key: &str) {
    if let Some(v) = src.get(key) {
        dst[key] = v.clone();
    }
}

fn extract_detail_string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = payload.get(*key)?;
        match value {
            Value::Null => None,
            Value::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            }
            Value::Bool(v) => Some(v.to_string()),
            Value::Number(v) => Some(v.to_string()),
            _ => Some(truncate_detail(value.to_string(), 180)),
        }
    })
}

async fn ensure_solana_schema_with_pool(pool: &PgPool) -> Result<(), AdapterExecutionError> {
    SOLANA_SCHEMA_READY
        .get_or_try_init(|| async {
            let ddl = r#"
            CREATE SCHEMA IF NOT EXISTS solana;

            CREATE TABLE IF NOT EXISTS solana.tx_intents (
              id              TEXT PRIMARY KEY,
              tenant_id       TEXT NULL,
              job_id          TEXT NULL,
              intent_type     TEXT NOT NULL,
              from_addr       TEXT NULL,
              to_addr         TEXT NOT NULL,
              amount          BIGINT NOT NULL,
              asset           TEXT NULL,
              program_id      TEXT NULL,
              action          TEXT NULL,
              status          TEXT NOT NULL CHECK (status IN ('received','finalized')),
              final_signature TEXT NULL,
              final_err_json  JSONB NULL,
              created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            CREATE TABLE IF NOT EXISTS solana.tx_attempts (
              id                        UUID PRIMARY KEY,
              tenant_id                 TEXT NULL,
              intent_id                 TEXT NOT NULL REFERENCES solana.tx_intents(id) ON DELETE CASCADE,
              job_id                    TEXT NULL,
              status                    TEXT NOT NULL CHECK (status IN ('created','sent','finalized','expired')),
              signature                 TEXT NULL,
              cu_limit                  INT NULL,
              cu_price_micro_lamports   BIGINT NULL,
              poll_no                   INT NOT NULL DEFAULT 0,
              last_confirmation_status  TEXT NULL,
              last_err_json             JSONB NULL,
              last_checked_at           TIMESTAMPTZ NULL,
              blockhash_used            TEXT NULL,
              simulation_outcome        TEXT NULL,
              provider_used             TEXT NULL,
              created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
              updated_at                TIMESTAMPTZ NOT NULL DEFAULT now()
            );

            CREATE INDEX IF NOT EXISTS solana_tx_attempts_intent_id_idx
              ON solana.tx_attempts(intent_id);
            "#;

            for stmt in ddl.split(';') {
                let stmt = stmt.trim();
                if stmt.is_empty() {
                    continue;
                }

                sqlx::query(stmt).execute(pool).await.map_err(|err| {
                    AdapterExecutionError::Transport(format!(
                        "solana schema bootstrap failed: {err}"
                    ))
                })?;
            }

            let alters = [
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS final_err_json JSONB",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS tenant_id TEXT",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS job_id TEXT",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS from_addr TEXT",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS asset TEXT",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS program_id TEXT",
                "ALTER TABLE solana.tx_intents ADD COLUMN IF NOT EXISTS action TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS tenant_id TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS job_id TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS cu_limit INT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS cu_price_micro_lamports BIGINT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS poll_no INT NOT NULL DEFAULT 0",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS last_confirmation_status TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS last_err_json JSONB",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS last_checked_at TIMESTAMPTZ",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS blockhash_used TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS simulation_outcome TEXT",
                "ALTER TABLE solana.tx_attempts ADD COLUMN IF NOT EXISTS provider_used TEXT",
            ];

            for stmt in alters {
                sqlx::query(stmt).execute(pool).await.map_err(|err| {
                    AdapterExecutionError::Transport(format!("solana schema alter failed: {err}"))
                })?;
            }

            let indexes = [
                "CREATE INDEX IF NOT EXISTS solana_tx_intents_tenant_job_idx ON solana.tx_intents(tenant_id, job_id, updated_at DESC)",
                "CREATE INDEX IF NOT EXISTS solana_tx_attempts_tenant_intent_job_idx ON solana.tx_attempts(tenant_id, intent_id, job_id, updated_at DESC)",
            ];
            for stmt in indexes {
                sqlx::query(stmt).execute(pool).await.map_err(|err| {
                    AdapterExecutionError::Transport(format!("solana schema index failed: {err}"))
                })?;
            }

            enforce_single_active_attempt_uniqueness(pool).await?;

            let constraint_names = sqlx::query_scalar::<_, String>(
                r#"
                SELECT c.conname
                FROM pg_constraint c
                JOIN pg_class t ON t.oid = c.conrelid
                JOIN pg_namespace n ON n.oid = t.relnamespace
                WHERE n.nspname = 'solana'
                  AND t.relname = 'tx_attempts'
                  AND c.contype = 'c'
                  AND pg_get_constraintdef(c.oid) ILIKE '%status%'
                "#,
            )
            .fetch_all(pool)
            .await
            .map_err(|err| {
                AdapterExecutionError::Transport(format!(
                    "failed to inspect solana.tx_attempts constraints: {err}"
                ))
            })?;

            for conname in constraint_names {
                let safe_name = conname.replace('"', "\"\"");
                let stmt = format!(
                    r#"ALTER TABLE solana.tx_attempts DROP CONSTRAINT IF EXISTS "{}""#,
                    safe_name
                );
                sqlx::query(&stmt).execute(pool).await.map_err(|err| {
                    AdapterExecutionError::Transport(format!(
                        "failed to drop solana tx_attempts status check: {err}"
                    ))
                })?;
            }

            sqlx::query(
                "ALTER TABLE solana.tx_attempts ADD CONSTRAINT tx_attempts_status_check CHECK (status IN ('created','sent','finalized','expired'))",
            )
            .execute(pool)
            .await
            .map_err(|err| {
                AdapterExecutionError::Transport(format!(
                    "failed to enforce solana tx_attempts status check: {err}"
                ))
            })?;

            Ok::<(), AdapterExecutionError>(())
        })
        .await
        .map(|_| ())
}

async fn enforce_single_active_attempt_uniqueness(
    pool: &PgPool,
) -> Result<(), AdapterExecutionError> {
    const INDEX_SQL: &str = r#"
        CREATE UNIQUE INDEX IF NOT EXISTS solana_tx_attempts_single_active_idx
        ON solana.tx_attempts(intent_id)
        WHERE status IN ('created', 'sent')
    "#;
    const MAX_PASSES: usize = 3;
    let mut deduped_total: u64 = 0;

    for pass in 1..=MAX_PASSES {
        let deduped = dedupe_active_attempt_rows(pool).await?;
        deduped_total = deduped_total.saturating_add(deduped);
        if deduped > 0 {
            eprintln!(
                "adapter_solana bootstrap: expired {} duplicate active attempt row(s) on pass {}",
                deduped, pass
            );
        }

        match sqlx::query(INDEX_SQL).execute(pool).await {
            Ok(_) => {
                if deduped_total > 0 {
                    eprintln!(
                        "adapter_solana bootstrap: enforced active-attempt uniqueness after deduping {} row(s)",
                        deduped_total
                    );
                }
                return Ok(());
            }
            Err(err) if is_sqlstate(&err, "23505") && pass < MAX_PASSES => {
                eprintln!(
                    "adapter_solana bootstrap: active-attempt uniqueness index conflict on pass {}, retrying",
                    pass
                );
                continue;
            }
            Err(err) if is_sqlstate(&err, "23505") => {
                return Err(AdapterExecutionError::Transport(format!(
                    "failed to enforce active-attempt uniqueness after {MAX_PASSES} passes: {err}"
                )));
            }
            Err(err) => {
                return Err(AdapterExecutionError::Transport(format!(
                    "failed to create active-attempt uniqueness index: {err}"
                )));
            }
        }
    }

    Err(AdapterExecutionError::Transport(
        "unexpected failure enforcing active-attempt uniqueness".to_owned(),
    ))
}

async fn dedupe_active_attempt_rows(pool: &PgPool) -> Result<u64, AdapterExecutionError> {
    let result = sqlx::query(
        r#"
        WITH ranked AS (
            SELECT
                id,
                ROW_NUMBER() OVER (
                    PARTITION BY intent_id
                    ORDER BY created_at DESC, id DESC
                ) AS rn
            FROM solana.tx_attempts
            WHERE status IN ('created', 'sent')
        ),
        duplicates AS (
            SELECT id
            FROM ranked
            WHERE rn > 1
        )
        UPDATE solana.tx_attempts attempt
        SET status = 'expired',
            last_err_json = COALESCE(attempt.last_err_json, '{}'::jsonb)
                || jsonb_build_object(
                    'dedup_reason', 'bootstrap_single_active_attempt_enforcement',
                    'dedup_at', now()
                ),
            last_checked_at = COALESCE(attempt.last_checked_at, now()),
            updated_at = now()
        FROM duplicates
        WHERE attempt.id = duplicates.id
        "#,
    )
    .execute(pool)
    .await
    .map_err(|err| {
        AdapterExecutionError::Transport(format!(
            "failed to dedupe duplicate active attempts before uniqueness index: {err}"
        ))
    })?;

    Ok(result.rows_affected())
}

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn platform_signing_enabled() -> bool {
    env_bool("SOLANA_PLATFORM_SIGNING_ENABLED", true)
}

fn playground_signing_enabled() -> bool {
    env_bool("SOLANA_PLAYGROUND_PLATFORM_SIGNING_ENABLED", true)
}

fn playground_demo_scenarios_enabled() -> bool {
    env_bool("SOLANA_PLAYGROUND_DEMO_SCENARIOS_ENABLED", true)
}

fn is_playground_internal_request(request: &AdapterExecutionRequest) -> bool {
    let metering_scope_is_playground = request
        .metadata
        .get("metering.scope")
        .map(|value| value.trim().eq_ignore_ascii_case("playground"))
        .unwrap_or(false);
    let ui_surface_is_playground = request
        .metadata
        .get("ui.surface")
        .map(|value| value.trim().eq_ignore_ascii_case("playground"))
        .unwrap_or(false);
    let internal_submitter = request
        .metadata
        .get("submitter.kind")
        .map(|value| value.trim().eq_ignore_ascii_case("internal_service"))
        .unwrap_or(false);
    metering_scope_is_playground && ui_surface_is_playground && internal_submitter
}

fn playground_demo_scenario(request: &AdapterExecutionRequest) -> PlaygroundDemoScenario {
    let Some(value) = request.metadata.get("playground.demo_scenario") else {
        return PlaygroundDemoScenario::Real;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "success" | "synthetic_success" | "synthetic-success" => {
            PlaygroundDemoScenario::SyntheticSuccess
        }
        "retry_then_success" | "retry-then-success" | "retry" => {
            PlaygroundDemoScenario::RetryThenSuccess
        }
        "terminal_failure" | "terminal-failure" | "terminal" => {
            PlaygroundDemoScenario::TerminalFailure
        }
        _ => PlaygroundDemoScenario::Real,
    }
}

fn rpc_url_is_mainnet(rpc_url: &str) -> bool {
    let normalized = rpc_url.trim().to_ascii_lowercase();
    normalized.contains("mainnet")
}

fn platform_signing_enabled_for_request(
    request: &AdapterExecutionRequest,
    rpc_urls: &[String],
) -> bool {
    if platform_signing_enabled() {
        return true;
    }
    if !playground_signing_enabled() {
        return false;
    }
    if !is_playground_internal_request(request) {
        return false;
    }
    !rpc_urls.iter().any(|rpc_url| rpc_url_is_mainnet(rpc_url))
}

fn synthetic_demo_signature(attempt_id: Uuid) -> String {
    format!(
        "demo_sig_{}",
        attempt_id.to_string().replace('-', "").to_ascii_lowercase()
    )
}

fn resolve_solana_rpc_urls(explicit: Option<&str>) -> Vec<String> {
    default_solana_rpc_endpoints(explicit).urls
}

fn preferred_solana_rpc_urls(preferred: Option<&str>, candidates: &[String]) -> Vec<String> {
    preferred_provider_urls(preferred, candidates)
}

fn primary_solana_rpc_url(candidates: &[String]) -> String {
    primary_provider_url(candidates, "https://api.devnet.solana.com")
}

fn default_solana_rpc_endpoints(
    explicit: Option<&str>,
) -> rpc_layer::OrderedProviderEndpoints {
    resolve_provider_urls(
        explicit,
        "SOLANA_RPC_PRIMARY_URL",
        "SOLANA_RPC_URLS",
        "SOLANA_RPC_FALLBACK_URLS",
        "SOLANA_RPC_URL",
        "https://api.devnet.solana.com",
    )
}

fn default_cu_limit() -> i32 {
    std::env::var("SOLANA_CU_LIMIT")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(300_000)
}

fn default_cu_price_micro_lamports() -> i64 {
    std::env::var("SOLANA_CU_PRICE_MICROLAMPORTS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(1_000)
}

fn max_cu_price_micro_lamports() -> i64 {
    std::env::var("SOLANA_CU_PRICE_MAX_MICROLAMPORTS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v >= 0)
        .unwrap_or(100_000)
}

fn normalize_cu_limit(value: Option<i32>) -> i32 {
    value.filter(|v| *v > 0).unwrap_or_else(default_cu_limit)
}

fn normalize_cu_price(value: Option<i64>) -> i64 {
    value
        .filter(|v| *v >= 0)
        .unwrap_or_else(default_cu_price_micro_lamports)
        .clamp(0, max_cu_price_micro_lamports())
}

fn payer_secret_material() -> Result<String, String> {
    if let Some(secret) =
        env_non_empty("SOLANA_PAYER_SECRET_BASE58").or_else(|| env_non_empty("PAYER_SECRET_BASE58"))
    {
        return Ok(secret);
    }

    let path = env_non_empty("SOLANA_PAYER_SECRET_FILE")
        .or_else(|| env_non_empty("PAYER_SECRET_FILE"))
        .ok_or_else(|| {
            "missing payer secret (set SOLANA_PAYER_SECRET_BASE58 or SOLANA_PAYER_SECRET_FILE)"
                .to_owned()
        })?;

    fs::read_to_string(&path)
        .map(|v| v.trim().to_owned())
        .map_err(|err| format!("failed to read payer secret file '{path}': {err}"))
}

#[derive(Debug, Deserialize)]
struct SignedTxOutput {
    signed_tx_base64: String,
}

fn sign_transfer_with_node(
    to_addr: &str,
    amount: i64,
    recent_blockhash: &str,
    cu_limit: i32,
    cu_price_micro_lamports: i64,
) -> Result<String, String> {
    if amount <= 0 {
        return Err("amount must be a positive integer".to_owned());
    }

    let payer_secret = payer_secret_material()?;
    let default_script = format!("{}/gen_tx.mjs", env!("CARGO_MANIFEST_DIR"));
    let script_path = env_non_empty("SOLANA_SIGN_SCRIPT").unwrap_or(default_script);

    let output = Command::new("node")
        .arg(&script_path)
        .env("PAYER_SECRET_BASE58", payer_secret)
        .env("LATEST_BLOCKHASH", recent_blockhash)
        .env("TO_PUBKEY", to_addr)
        .env("LAMPORTS", amount.to_string())
        .env("CU_LIMIT", cu_limit.to_string())
        .env(
            "CU_PRICE_MICROLAMPORTS",
            cu_price_micro_lamports.to_string(),
        )
        .output()
        .map_err(|err| format!("failed to run node signer: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return Err(format!(
            "node signer failed (script={}): {}{}",
            script_path,
            if stderr.is_empty() {
                "no stderr".to_owned()
            } else {
                stderr
            },
            if stdout.is_empty() {
                "".to_owned()
            } else {
                format!("; stdout={stdout}")
            }
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("invalid signer stdout utf8: {err}"))?;
    let parsed: SignedTxOutput = serde_json::from_str(stdout.trim())
        .map_err(|err| format!("invalid signer json output from {}: {err}", script_path))?;

    let signed_tx = parsed.signed_tx_base64.trim().to_owned();
    if signed_tx.is_empty() {
        return Err("signer returned empty signed_tx_base64".to_owned());
    }

    Ok(signed_tx)
}

fn map_db_error(operation: &str, err: sqlx::Error) -> AdapterExecutionError {
    if is_sqlstate(&err, "42P01") || is_sqlstate(&err, "3F000") {
        return AdapterExecutionError::Unavailable(format!(
            "solana storage unavailable during `{operation}`"
        ));
    }

    AdapterExecutionError::Transport(format!("{operation} failed: {err}"))
}

fn build_success_envelope(
    state: AdapterProgressState,
    code: &str,
    message: &str,
    provider_reference: Option<String>,
    details: BTreeMap<String, String>,
) -> AdapterExecutionEnvelope {
    let outcome = if state == AdapterProgressState::Finalized {
        AdapterOutcome::Succeeded {
            provider_reference: provider_reference.clone(),
            details: details.clone(),
        }
    } else {
        AdapterOutcome::InProgress {
            provider_reference: provider_reference.clone(),
            details: details.clone(),
            poll_after_ms: None,
        }
    };

    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot {
            state,
            code: code.to_owned(),
            message: message.to_owned(),
            provider_reference: provider_reference.clone(),
            details: details.clone(),
        },
        outcome,
    }
}

fn build_failure_envelope(
    normalized: NormalizedSolanaError,
    provider_reference: Option<String>,
    mut details: BTreeMap<String, String>,
) -> AdapterExecutionEnvelope {
    details.insert(
        "normalized_error_class".to_owned(),
        normalized.class.as_str().to_owned(),
    );
    details.insert(
        "raw_provider_error".to_owned(),
        truncate_detail(normalized.raw_provider_error.to_string(), 512),
    );

    match normalized.class {
        SolanaErrorClass::Retryable => AdapterExecutionEnvelope {
            status: AdapterStatusSnapshot {
                state: AdapterProgressState::FailedRetryable,
                code: normalized.code.clone(),
                message: normalized.message.clone(),
                provider_reference: provider_reference.clone(),
                details: details.clone(),
            },
            outcome: AdapterOutcome::RetryableFailure {
                code: normalized.code,
                message: normalized.message,
                retry_after_ms: None,
                provider_details: Some(normalized.raw_provider_error),
            },
        },
        SolanaErrorClass::Terminal => AdapterExecutionEnvelope {
            status: AdapterStatusSnapshot {
                state: AdapterProgressState::FailedTerminal,
                code: normalized.code.clone(),
                message: normalized.message.clone(),
                provider_reference: provider_reference.clone(),
                details,
            },
            outcome: AdapterOutcome::TerminalFailure {
                code: normalized.code,
                message: normalized.message,
                provider_details: Some(normalized.raw_provider_error),
            },
        },
        SolanaErrorClass::Blocked => AdapterExecutionEnvelope {
            status: AdapterStatusSnapshot {
                state: AdapterProgressState::Blocked,
                code: normalized.code.clone(),
                message: normalized.message.clone(),
                provider_reference,
                details,
            },
            outcome: AdapterOutcome::Blocked {
                code: normalized.code,
                message: normalized.message,
            },
        },
        SolanaErrorClass::ManualInterventionRequired => AdapterExecutionEnvelope {
            status: AdapterStatusSnapshot {
                state: AdapterProgressState::ManualInterventionRequired,
                code: normalized.code.clone(),
                message: normalized.message.clone(),
                provider_reference,
                details,
            },
            outcome: AdapterOutcome::ManualReview {
                code: normalized.code,
                message: normalized.message,
            },
        },
    }
}

fn normalized_from_rpc_call_error(
    adapter: &SolanaQueueAdapter,
    err: RpcCallError,
) -> NormalizedSolanaError {
    match err {
        RpcCallError::Provider(err) => {
            let mut normalized = adapter.normalize_solana_error(&err.raw);
            if normalized.code == "solana.unknown_error" {
                normalized.message = format!("rpc error {}: {}", err.code, err.message);
            }
            normalized
        }
        RpcCallError::Transport(message) => NormalizedSolanaError {
            code: "solana.provider_transient".to_owned(),
            message: format!("solana rpc transport failure: {message}"),
            class: SolanaErrorClass::Retryable,
            raw_provider_error: json!({ "transport_error": message }),
        },
    }
}

fn snapshot_to_envelope(snapshot: AdapterStatusSnapshot) -> AdapterExecutionEnvelope {
    let outcome = match snapshot.state {
        AdapterProgressState::Submitted
        | AdapterProgressState::Confirming
        | AdapterProgressState::Landed => AdapterOutcome::InProgress {
            provider_reference: snapshot.provider_reference.clone(),
            details: snapshot.details.clone(),
            poll_after_ms: None,
        },
        AdapterProgressState::Finalized => AdapterOutcome::Succeeded {
            provider_reference: snapshot.provider_reference.clone(),
            details: snapshot.details.clone(),
        },
        AdapterProgressState::FailedRetryable => AdapterOutcome::RetryableFailure {
            code: snapshot.code.clone(),
            message: snapshot.message.clone(),
            retry_after_ms: None,
            provider_details: None,
        },
        AdapterProgressState::FailedTerminal => AdapterOutcome::TerminalFailure {
            code: snapshot.code.clone(),
            message: snapshot.message.clone(),
            provider_details: None,
        },
        AdapterProgressState::Blocked => AdapterOutcome::Blocked {
            code: snapshot.code.clone(),
            message: snapshot.message.clone(),
        },
        AdapterProgressState::ManualInterventionRequired => AdapterOutcome::ManualReview {
            code: snapshot.code.clone(),
            message: snapshot.message.clone(),
        },
    };

    AdapterExecutionEnvelope {
        status: snapshot,
        outcome,
    }
}

#[derive(Debug, Clone)]
struct SolanaStatusRow {
    intent_id: String,
    intent_status: String,
    final_signature: Option<String>,
    attempt_id: Option<Uuid>,
    attempt_status: Option<String>,
    attempt_signature: Option<String>,
    last_confirmation_status: Option<String>,
    last_err_json: Option<Value>,
    final_err_json: Option<Value>,
    blockhash_used: Option<String>,
    simulation_outcome: Option<String>,
    provider_used: Option<String>,
}

impl SolanaStatusRow {
    fn from_enhanced(
        row: (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<Value>,
            Option<Value>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    ) -> Self {
        Self {
            intent_id: row.0,
            intent_status: row.1,
            final_signature: row.2,
            attempt_id: row.3,
            attempt_status: row.4,
            attempt_signature: row.5,
            last_confirmation_status: row.6,
            last_err_json: row.7,
            final_err_json: row.8,
            blockhash_used: row.9,
            simulation_outcome: row.10,
            provider_used: row.11,
        }
    }

    fn from_basic(
        row: (
            String,
            String,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
        ),
    ) -> Self {
        Self {
            intent_id: row.0,
            intent_status: row.1,
            final_signature: row.2,
            attempt_id: row.3,
            attempt_status: row.4,
            attempt_signature: row.5,
            last_confirmation_status: None,
            last_err_json: None,
            final_err_json: None,
            blockhash_used: None,
            simulation_outcome: None,
            provider_used: None,
        }
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn is_sqlstate(err: &sqlx::Error, code: &str) -> bool {
    matches!(err, sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some(code))
}

fn truncate_detail(mut value: String, max_len: usize) -> String {
    if value.len() <= max_len {
        return value;
    }

    value.truncate(max_len);
    value.push_str("...");
    value
}

fn is_landed_confirmation_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "processed" | "confirmed" | "finalized"
    )
}

#[async_trait]
impl DomainAdapter for SolanaQueueAdapter {
    async fn validate(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<(), AdapterExecutionError> {
        self.validate_intent(request)
    }

    async fn execute(
        &self,
        request: &AdapterExecutionRequest,
        _context: &AdapterExecutionContext,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.execute_solana_intent(request).await
    }

    async fn resume(
        &self,
        request: &AdapterExecutionRequest,
        context: &AdapterResumeContext,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.resume_solana_intent(request, context).await
    }

    async fn fetch_status(
        &self,
        handle: &AdapterStatusHandle,
    ) -> Result<AdapterStatusSnapshot, AdapterExecutionError> {
        self.check_submission_status(handle).await
    }
}
