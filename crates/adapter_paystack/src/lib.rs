use adapter_contract::{
    AdapterExecutionContext, AdapterExecutionEnvelope, AdapterProgressState, AdapterRegistry,
    AdapterStatusHandle, AdapterStatusSnapshot, DomainAdapter,
};
use anyhow::Context;
use async_trait::async_trait;
use execution_core::{AdapterExecutionError, AdapterExecutionRequest, AdapterId, AdapterOutcome};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::PgPool;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

static PAYSTACK_SCHEMA_READY: OnceCell<()> = OnceCell::const_new();

const PAYSTACK_INTENT_TRANSACTION_VERIFY: &str = "paystack.transaction.verify.v1";
const PAYSTACK_INTENT_REFUND_CREATE: &str = "paystack.refund.create.v1";
const PAYSTACK_INTENT_REFUND_VERIFY: &str = "paystack.refund.verify.v1";
const PAYSTACK_INTENT_TRANSFER_CREATE: &str = "paystack.transfer.create.v1";
const PAYSTACK_INTENT_TRANSFER_VERIFY: &str = "paystack.transfer.verify.v1";

#[derive(Debug, Clone)]
pub struct PaystackAdapterConfig {
    pub api_base_url: String,
    pub secret_key: Option<String>,
    pub timeout_ms: u64,
    pub poll_after_ms: u64,
    pub connector_broker_base_url: Option<String>,
    pub connector_broker_bearer_token: Option<String>,
    pub connector_broker_principal_id: Option<String>,
}

impl Default for PaystackAdapterConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.paystack.co".to_owned(),
            secret_key: None,
            timeout_ms: 10_000,
            poll_after_ms: 5_000,
            connector_broker_base_url: None,
            connector_broker_bearer_token: None,
            connector_broker_principal_id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaystackOperation {
    TransactionVerify,
    RefundCreate,
    RefundVerify,
    TransferCreate,
    TransferVerify,
}

impl PaystackOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::TransactionVerify => "transaction_verify",
            Self::RefundCreate => "refund_create",
            Self::RefundVerify => "refund_verify",
            Self::TransferCreate => "transfer_create",
            Self::TransferVerify => "transfer_verify",
        }
    }

    fn is_create(self) -> bool {
        matches!(self, Self::RefundCreate | Self::TransferCreate)
    }
}

#[derive(Debug, Clone)]
struct PaystackExecutionInput {
    intent_kind: String,
    operation: PaystackOperation,
    reference: String,
    amount_minor: Option<i64>,
    currency: Option<String>,
    source_reference: Option<String>,
    destination_reference: Option<String>,
    connector_binding_id: Option<String>,
    connector_reference: Option<String>,
    provider_body: Value,
}

#[derive(Debug, Clone)]
struct StoredExecutionRow {
    status: String,
    provider_reference: Option<String>,
    remote_id: Option<String>,
    last_response_json: Option<Value>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    amount_minor: Option<i64>,
    currency: Option<String>,
    source_reference: Option<String>,
    destination_reference: Option<String>,
    connector_reference: Option<String>,
}

#[derive(Debug, Clone)]
struct PaystackApiResponse {
    status_code: StatusCode,
    body: Value,
    data: Option<Value>,
    message: String,
    envelope_ok: bool,
}

#[derive(Debug, Serialize)]
struct ConnectorBrokerUsePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor_kind: Option<String>,
    purpose: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval_request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    intent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    correlation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectorBrokerBindingView {
    connector_type: String,
}

#[derive(Debug, Deserialize)]
struct ConnectorBrokerUseResponse {
    binding: ConnectorBrokerBindingView,
    secrets: BTreeMap<String, String>,
}

enum ProviderCallError {
    Timeout(String),
    Transport(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaystackTransactionVerifyPayloadSchema {
    reference: String,
    #[serde(default)]
    amount: Option<i64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    customer_reference: Option<String>,
    #[serde(default)]
    expected_state: Option<String>,
    #[serde(default)]
    connector_binding_id: Option<String>,
    #[serde(default)]
    connector_reference: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaystackRefundCreatePayloadSchema {
    #[serde(default, alias = "transaction_reference", alias = "reference")]
    payment_reference: Option<String>,
    amount: i64,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    destination_reference: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    connector_binding_id: Option<String>,
    #[serde(default)]
    connector_reference: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaystackRefundVerifyPayloadSchema {
    #[serde(default, alias = "id", alias = "refund_reference")]
    refund_id: Option<String>,
    #[serde(default)]
    connector_binding_id: Option<String>,
    #[serde(default)]
    connector_reference: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaystackTransferCreatePayloadSchema {
    #[serde(default, alias = "recipient")]
    recipient_code: Option<String>,
    amount: i64,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    connector_binding_id: Option<String>,
    #[serde(default)]
    connector_reference: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaystackTransferVerifyPayloadSchema {
    reference: String,
    #[serde(default)]
    connector_binding_id: Option<String>,
    #[serde(default)]
    connector_reference: Option<String>,
}

#[derive(Clone)]
pub struct PaystackAdapter {
    pool: PgPool,
    client: Client,
    config: PaystackAdapterConfig,
}

impl PaystackAdapter {
    pub fn new(pool: PgPool, config: PaystackAdapterConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(
                config.timeout_ms.max(1_000),
            ))
            .build()
            .context("failed to build Paystack HTTP client")?;
        Ok(Self {
            pool,
            client,
            config,
        })
    }

    pub fn config(&self) -> &PaystackAdapterConfig {
        &self.config
    }

    fn supports_intent_kind(intent_kind: &str) -> bool {
        matches!(
            intent_kind,
            PAYSTACK_INTENT_TRANSACTION_VERIFY
                | PAYSTACK_INTENT_REFUND_CREATE
                | PAYSTACK_INTENT_REFUND_VERIFY
                | PAYSTACK_INTENT_TRANSFER_CREATE
                | PAYSTACK_INTENT_TRANSFER_VERIFY
        )
    }

    fn poll_after_ms(&self) -> u64 {
        self.config.poll_after_ms.max(1_000)
    }

    fn global_secret_key(&self) -> Option<&str> {
        self.config
            .secret_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn require_secret_key(&self) -> Result<&str, AdapterExecutionError> {
        self.global_secret_key().ok_or_else(|| {
            AdapterExecutionError::Unavailable(
                "paystack adapter is not configured with PAYSTACK_SECRET_KEY".to_owned(),
            )
        })
    }

    fn connector_broker_base_url(&self) -> Option<&str> {
        self.config
            .connector_broker_base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn connector_broker_bearer_token(&self) -> Option<&str> {
        self.config
            .connector_broker_bearer_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn connector_broker_principal_id(&self) -> Option<&str> {
        self.config
            .connector_broker_principal_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn ensure_secret_source_available(
        &self,
        input: &PaystackExecutionInput,
    ) -> Result<(), AdapterExecutionError> {
        if input.connector_binding_id.is_some() {
            if self.connector_broker_base_url().is_none()
                || self.connector_broker_bearer_token().is_none()
                || self.connector_broker_principal_id().is_none()
            {
                return Err(AdapterExecutionError::Unavailable(
                    "paystack connector binding requires connector broker configuration".to_owned(),
                ));
            }
            return Ok(());
        }
        self.require_secret_key().map(|_| ())
    }

    fn resolve_connector_environment_id(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Option<String> {
        request
            .auth_context
            .as_ref()
            .and_then(|auth| auth.environment_id.clone())
            .or_else(|| request.metadata.get("agent.environment_id").cloned())
            .or_else(|| request.metadata.get("connector.environment_id").cloned())
            .or_else(|| request.metadata.get("environment_id").cloned())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }

    async fn resolve_secret_key(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
    ) -> Result<String, AdapterExecutionError> {
        let Some(binding_id) = input.connector_binding_id.as_deref() else {
            return Ok(self.require_secret_key()?.to_owned());
        };
        let tenant_id = request.tenant_id.as_str();
        let environment_id = self
            .resolve_connector_environment_id(request)
            .ok_or_else(|| {
                AdapterExecutionError::ContractViolation(
                    "paystack connector binding requires an environment_id".to_owned(),
                )
            })?;
        let base_url = self
            .connector_broker_base_url()
            .ok_or_else(|| {
                AdapterExecutionError::Unavailable(
                    "paystack connector binding requires CONNECTOR_BROKER_BASE_URL".to_owned(),
                )
            })?
            .trim_end_matches('/');
        let bearer_token = self.connector_broker_bearer_token().ok_or_else(|| {
            AdapterExecutionError::Unavailable(
                "paystack connector binding requires CONNECTOR_BROKER_BEARER_TOKEN".to_owned(),
            )
        })?;
        let principal_id = self.connector_broker_principal_id().ok_or_else(|| {
            AdapterExecutionError::Unavailable(
                "paystack connector binding requires CONNECTOR_BROKER_PRINCIPAL_ID".to_owned(),
            )
        })?;
        let purpose = format!("paystack.{}.execute", input.operation.as_str());
        let payload = ConnectorBrokerUsePayload {
            actor_id: request
                .auth_context
                .as_ref()
                .and_then(|auth| auth.agent_id.clone())
                .or_else(|| request.metadata.get("agent.id").cloned()),
            actor_kind: request.metadata.get("submitter.kind").cloned(),
            purpose,
            request_id: request.request_id.as_ref().map(ToString::to_string),
            action_request_id: request.metadata.get("agent.action_request_id").cloned(),
            approval_request_id: request.metadata.get("approval.request_id").cloned(),
            intent_id: Some(request.intent_id.to_string()),
            job_id: Some(request.job_id.to_string()),
            correlation_id: request
                .correlation_id
                .clone()
                .or_else(|| request.metadata.get("correlation_id").cloned()),
        };
        let url = format!(
            "{}/api/internal/tenants/{}/environments/{}/connector-bindings/{}/broker-use",
            base_url, tenant_id, environment_id, binding_id
        );
        let response = self
            .client
            .post(url)
            .bearer_auth(bearer_token)
            .header("x-principal-id", principal_id)
            .header("x-submitter-kind", "internal_service")
            .json(&payload)
            .send()
            .await
            .map_err(map_reqwest_error)
            .map_err(|err| {
                AdapterExecutionError::Unavailable(format!(
                    "paystack connector broker request failed: {:?}",
                    provider_call_details(err)
                ))
            })?;
        let status_code = response.status();
        let text = response.text().await.map_err(|err| {
            AdapterExecutionError::Unavailable(format!(
                "failed to read paystack connector broker response: {err}"
            ))
        })?;
        if !status_code.is_success() {
            return Err(AdapterExecutionError::Unavailable(format!(
                "paystack connector broker returned {}: {}",
                status_code.as_u16(),
                truncate_for_json(&text, 512)
            )));
        }
        let broker_response: ConnectorBrokerUseResponse =
            serde_json::from_str(&text).map_err(|err| {
                AdapterExecutionError::Unavailable(format!(
                    "failed to decode paystack connector broker response: {err}"
                ))
            })?;
        if !broker_response
            .binding
            .connector_type
            .trim()
            .eq_ignore_ascii_case("paystack")
        {
            return Err(AdapterExecutionError::ContractViolation(format!(
                "connector binding `{binding_id}` is not a paystack binding"
            )));
        }
        extract_paystack_secret_key(&broker_response.secrets).ok_or_else(|| {
            AdapterExecutionError::ContractViolation(format!(
                "connector binding `{binding_id}` did not provide a usable paystack secret key"
            ))
        })
    }

    async fn ensure_schema(&self) -> Result<(), AdapterExecutionError> {
        PAYSTACK_SCHEMA_READY
            .get_or_try_init(|| async {
                ensure_paystack_schema(&self.pool)
                    .await
                    .map_err(|err| AdapterExecutionError::Unavailable(err.to_string()))
            })
            .await?;
        Ok(())
    }

    pub fn validate_intent(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<(), AdapterExecutionError> {
        if !Self::supports_intent_kind(request.intent_kind.as_str()) {
            return Err(AdapterExecutionError::UnsupportedIntent(format!(
                "unsupported intent kind `{}` for paystack adapter",
                request.intent_kind
            )));
        }
        let payload = normalize_payload(request)?;
        let input = parse_execution_input(request, &payload)?;
        self.ensure_secret_source_available(&input)?;
        Ok(())
    }

    pub async fn execute_paystack_intent(
        &self,
        request: &AdapterExecutionRequest,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        self.ensure_schema().await?;
        self.validate_intent(request)?;
        let payload = normalize_payload(request)?;
        let input = parse_execution_input(request, &payload)?;
        let secret_key = self.resolve_secret_key(request, &input).await?;
        let existing = self.load_execution_row(request.intent_id.as_str()).await?;

        match input.operation {
            PaystackOperation::TransactionVerify => {
                self.execute_transaction_verify(request, &input, &secret_key).await
            }
            PaystackOperation::RefundCreate => {
                self.execute_refund_create(request, &input, existing, &secret_key)
                    .await
            }
            PaystackOperation::RefundVerify => {
                self.execute_refund_verify(request, &input, &secret_key).await
            }
            PaystackOperation::TransferCreate => {
                self.execute_transfer_create(request, &input, existing, &secret_key)
                    .await
            }
            PaystackOperation::TransferVerify => {
                self.execute_transfer_verify(request, &input, &secret_key).await
            }
        }
    }

    pub async fn check_status(
        &self,
        handle: &AdapterStatusHandle,
    ) -> Result<AdapterStatusSnapshot, AdapterExecutionError> {
        self.ensure_schema().await?;
        if let Some(provider_reference) = handle
            .provider_reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if let Some(row) = self
                .load_execution_row_by_provider_reference(provider_reference)
                .await?
            {
                return Ok(status_snapshot_from_row(&row));
            }
        }

        let row = self
            .load_execution_row(handle.intent_id.trim())
            .await?
            .ok_or_else(|| {
                AdapterExecutionError::Unavailable(format!(
                    "no paystack execution row found for intent `{}`",
                    handle.intent_id
                ))
            })?;
        Ok(status_snapshot_from_row(&row))
    }

    async fn execute_transaction_verify(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        let response = self
            .call_paystack(
                Method::GET,
                &format!("/transaction/verify/{}", input.reference),
                None,
                secret_key,
            )
            .await;
        let envelope = match response {
            Ok(response) => classify_transaction_verify_response(input, response),
            Err(err) => retryable_provider_failure(
                "paystack.transaction_verify_unavailable",
                "paystack transaction verification is temporarily unavailable",
                provider_call_details(err),
                Some(self.poll_after_ms()),
            ),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn execute_refund_verify(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        let response = self
            .call_paystack(
                Method::GET,
                &format!("/refund/{}", input.reference),
                None,
                secret_key,
            )
            .await;
        let envelope = match response {
            Ok(response) => classify_refund_response(input, response, self.poll_after_ms()),
            Err(err) => retryable_provider_failure(
                "paystack.refund_verify_unavailable",
                "paystack refund verification is temporarily unavailable",
                provider_call_details(err),
                Some(self.poll_after_ms()),
            ),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn execute_transfer_verify(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        let response = self
            .call_paystack(
                Method::GET,
                &format!("/transfer/verify/{}", input.reference),
                None,
                secret_key,
            )
            .await;
        let envelope = match response {
            Ok(response) => classify_transfer_response(input, response, self.poll_after_ms()),
            Err(err) => retryable_provider_failure(
                "paystack.transfer_verify_unavailable",
                "paystack transfer verification is temporarily unavailable",
                provider_call_details(err),
                Some(self.poll_after_ms()),
            ),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn execute_refund_create(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        existing: Option<StoredExecutionRow>,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        if let Some(row) = existing {
            return self
                .resume_create_from_row(input, row, request, secret_key)
                .await;
        }

        self.record_dispatching_row(request, input).await?;
        let response = self
            .call_paystack(
                Method::POST,
                "/refund",
                Some(input.provider_body.clone()),
                secret_key,
            )
            .await;
        let envelope = match response {
            Ok(response) => classify_refund_response(input, response, self.poll_after_ms()),
            Err(err) => ambiguous_create_failure(
                "paystack.refund_create_outcome_unknown",
                "paystack refund creation returned an ambiguous transport failure; manual review is required",
                provider_call_details(err),
            ),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn execute_transfer_create(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        existing: Option<StoredExecutionRow>,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        if let Some(row) = existing {
            return self
                .resume_create_from_row(input, row, request, secret_key)
                .await;
        }

        self.record_dispatching_row(request, input).await?;
        let response = self
            .call_paystack(
                Method::POST,
                "/transfer",
                Some(input.provider_body.clone()),
                secret_key,
            )
            .await;
        let envelope = match response {
            Ok(response) => classify_transfer_response(input, response, self.poll_after_ms()),
            Err(err) => ambiguous_create_failure(
                "paystack.transfer_create_outcome_unknown",
                "paystack transfer creation returned an ambiguous transport failure; manual review is required",
                provider_call_details(err),
            ),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn resume_create_from_row(
        &self,
        input: &PaystackExecutionInput,
        row: StoredExecutionRow,
        request: &AdapterExecutionRequest,
        secret_key: &str,
    ) -> Result<AdapterExecutionEnvelope, AdapterExecutionError> {
        if row.status == "dispatching"
            && row.provider_reference.is_none()
            && row.remote_id.is_none()
        {
            let envelope = ambiguous_create_failure(
                "paystack.previous_attempt_outcome_unknown",
                "a previous paystack create attempt did not record a provider reference; manual review is required",
                row.last_response_json.clone(),
            );
            self.persist_outcome(request, input, &envelope).await?;
            return Ok(envelope);
        }

        if is_final_row_status(&row.status) {
            return Ok(envelope_from_row(&row));
        }

        let envelope = match input.operation {
            PaystackOperation::RefundCreate => {
                let reference = row
                    .remote_id
                    .clone()
                    .or_else(|| row.provider_reference.clone())
                    .unwrap_or_else(|| input.reference.clone());
                match self
                    .call_paystack(
                        Method::GET,
                        &format!("/refund/{reference}"),
                        None,
                        secret_key,
                    )
                    .await
                {
                    Ok(response) => classify_refund_response(input, response, self.poll_after_ms()),
                    Err(err) => retryable_provider_failure(
                        "paystack.refund_poll_unavailable",
                        "paystack refund polling is temporarily unavailable",
                        provider_call_details(err),
                        Some(self.poll_after_ms()),
                    ),
                }
            }
            PaystackOperation::TransferCreate => {
                let reference = row
                    .provider_reference
                    .clone()
                    .or_else(|| row.remote_id.clone())
                    .unwrap_or_else(|| input.reference.clone());
                match self
                    .call_paystack(
                        Method::GET,
                        &format!("/transfer/verify/{reference}"),
                        None,
                        secret_key,
                    )
                    .await
                {
                    Ok(response) => {
                        classify_transfer_response(input, response, self.poll_after_ms())
                    }
                    Err(err) => retryable_provider_failure(
                        "paystack.transfer_poll_unavailable",
                        "paystack transfer polling is temporarily unavailable",
                        provider_call_details(err),
                        Some(self.poll_after_ms()),
                    ),
                }
            }
            _ => envelope_from_row(&row),
        };
        self.persist_outcome(request, input, &envelope).await?;
        Ok(envelope)
    }

    async fn call_paystack(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        secret_key: &str,
    ) -> Result<PaystackApiResponse, ProviderCallError> {
        let url = format!(
            "{}/{}",
            self.config.api_base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let mut request = self.client.request(method, url).bearer_auth(secret_key);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(map_reqwest_error)?;
        let status_code = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| ProviderCallError::Transport(err.to_string()))?;
        let body = serde_json::from_str::<Value>(&text)
            .unwrap_or_else(|_| json!({ "raw_body": truncate_for_json(&text, 2048) }));
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                status_code
                    .canonical_reason()
                    .unwrap_or("paystack request completed")
                    .to_owned()
            });
        let data = body.get("data").cloned();
        let envelope_ok = body
            .get("status")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| status_code.is_success());
        Ok(PaystackApiResponse {
            status_code,
            body,
            data,
            message,
            envelope_ok,
        })
    }

    async fn record_dispatching_row(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
    ) -> Result<(), AdapterExecutionError> {
        let connector_reference = input
            .connector_reference
            .clone()
            .or_else(|| request.metadata.get("connector.reference").cloned());
        self.upsert_execution_row(
            request,
            input,
            "dispatching",
            None,
            None,
            None,
            None,
            None,
            connector_reference,
        )
        .await?;
        self.append_attempt_row(request, "dispatching", None, None, None)
            .await
    }

    async fn persist_outcome(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        envelope: &AdapterExecutionEnvelope,
    ) -> Result<(), AdapterExecutionError> {
        let (status, provider_reference, remote_id, response_json, error_code, error_message) =
            storage_projection(envelope);
        let connector_reference = input
            .connector_reference
            .clone()
            .or_else(|| request.metadata.get("connector.reference").cloned());
        self.upsert_execution_row(
            request,
            input,
            status,
            provider_reference,
            remote_id,
            response_json,
            error_code,
            error_message,
            connector_reference,
        )
        .await?;
        self.append_attempt_row(
            request,
            status,
            provider_reference,
            error_code,
            response_json,
        )
        .await
    }

    async fn upsert_execution_row(
        &self,
        request: &AdapterExecutionRequest,
        input: &PaystackExecutionInput,
        status: &str,
        provider_reference: Option<&str>,
        remote_id: Option<&str>,
        response_json: Option<&Value>,
        error_code: Option<&str>,
        error_message: Option<&str>,
        connector_reference: Option<String>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            INSERT INTO paystack.executions (
              intent_id, tenant_id, job_id, intent_kind, operation, status,
              provider_reference, remote_id, request_payload_json, last_response_json,
              last_error_code, last_error_message, amount_minor, currency,
              source_reference, destination_reference, connector_reference, updated_at
            )
            VALUES (
              $1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, $10::jsonb,
              $11, $12, $13, $14, $15, $16, $17, CURRENT_TIMESTAMP
            )
            ON CONFLICT (intent_id) DO UPDATE
            SET tenant_id = EXCLUDED.tenant_id,
                job_id = EXCLUDED.job_id,
                intent_kind = EXCLUDED.intent_kind,
                operation = EXCLUDED.operation,
                status = EXCLUDED.status,
                provider_reference = COALESCE(EXCLUDED.provider_reference, paystack.executions.provider_reference),
                remote_id = COALESCE(EXCLUDED.remote_id, paystack.executions.remote_id),
                request_payload_json = EXCLUDED.request_payload_json,
                last_response_json = COALESCE(EXCLUDED.last_response_json, paystack.executions.last_response_json),
                last_error_code = EXCLUDED.last_error_code,
                last_error_message = EXCLUDED.last_error_message,
                amount_minor = COALESCE(EXCLUDED.amount_minor, paystack.executions.amount_minor),
                currency = COALESCE(EXCLUDED.currency, paystack.executions.currency),
                source_reference = COALESCE(EXCLUDED.source_reference, paystack.executions.source_reference),
                destination_reference = COALESCE(EXCLUDED.destination_reference, paystack.executions.destination_reference),
                connector_reference = COALESCE(EXCLUDED.connector_reference, paystack.executions.connector_reference),
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(request.intent_id.as_str())
        .bind(request.tenant_id.as_str())
        .bind(request.job_id.as_str())
        .bind(input.intent_kind.as_str())
        .bind(input.operation.as_str())
        .bind(status)
        .bind(provider_reference)
        .bind(remote_id)
        .bind(&input.provider_body)
        .bind(response_json)
        .bind(error_code)
        .bind(error_message)
        .bind(input.amount_minor)
        .bind(input.currency.as_deref())
        .bind(input.source_reference.as_deref())
        .bind(input.destination_reference.as_deref())
        .bind(connector_reference.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|err| {
            AdapterExecutionError::Unavailable(format!(
                "failed to upsert paystack execution row: {err}"
            ))
        })?;
        Ok(())
    }

    async fn append_attempt_row(
        &self,
        request: &AdapterExecutionRequest,
        phase: &str,
        provider_reference: Option<&str>,
        error_code: Option<&str>,
        response_json: Option<&Value>,
    ) -> Result<(), AdapterExecutionError> {
        sqlx::query(
            r#"
            INSERT INTO paystack.attempts (
              id, intent_id, tenant_id, job_id, attempt_no,
              phase, provider_reference, error_code, response_json,
              created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(request.intent_id.as_str())
        .bind(request.tenant_id.as_str())
        .bind(request.job_id.as_str())
        .bind(i32::try_from(request.attempt).unwrap_or(i32::MAX))
        .bind(phase)
        .bind(provider_reference)
        .bind(error_code)
        .bind(response_json)
        .execute(&self.pool)
        .await
        .map_err(|err| {
            AdapterExecutionError::Unavailable(format!(
                "failed to append paystack attempt row: {err}"
            ))
        })?;
        Ok(())
    }

    async fn load_execution_row(
        &self,
        intent_id: &str,
    ) -> Result<Option<StoredExecutionRow>, AdapterExecutionError> {
        sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<Value>,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT status, provider_reference, remote_id, last_response_json,
                   last_error_code, last_error_message, amount_minor, currency,
                   source_reference, destination_reference, connector_reference
            FROM paystack.executions
            WHERE intent_id = $1
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&self.pool)
        .await
        .map(|maybe| {
            maybe.map(|row| StoredExecutionRow {
                status: row.0,
                provider_reference: row.1,
                remote_id: row.2,
                last_response_json: row.3,
                last_error_code: row.4,
                last_error_message: row.5,
                amount_minor: row.6,
                currency: row.7,
                source_reference: row.8,
                destination_reference: row.9,
                connector_reference: row.10,
            })
        })
        .map_err(|err| {
            AdapterExecutionError::Unavailable(format!(
                "failed to load paystack execution row: {err}"
            ))
        })
    }

    async fn load_execution_row_by_provider_reference(
        &self,
        provider_reference: &str,
    ) -> Result<Option<StoredExecutionRow>, AdapterExecutionError> {
        sqlx::query_as::<
            _,
            (
                String,
                Option<String>,
                Option<String>,
                Option<Value>,
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT status, provider_reference, remote_id, last_response_json,
                   last_error_code, last_error_message, amount_minor, currency,
                   source_reference, destination_reference, connector_reference
            FROM paystack.executions
            WHERE provider_reference = $1 OR remote_id = $1
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(provider_reference)
        .fetch_optional(&self.pool)
        .await
        .map(|maybe| {
            maybe.map(|row| StoredExecutionRow {
                status: row.0,
                provider_reference: row.1,
                remote_id: row.2,
                last_response_json: row.3,
                last_error_code: row.4,
                last_error_message: row.5,
                amount_minor: row.6,
                currency: row.7,
                source_reference: row.8,
                destination_reference: row.9,
                connector_reference: row.10,
            })
        })
        .map_err(|err| {
            AdapterExecutionError::Unavailable(format!(
                "failed to load paystack execution row by provider reference: {err}"
            ))
        })
    }
}

#[async_trait]
impl DomainAdapter for PaystackAdapter {
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
        self.execute_paystack_intent(request).await
    }

    async fn fetch_status(
        &self,
        handle: &AdapterStatusHandle,
    ) -> Result<AdapterStatusSnapshot, AdapterExecutionError> {
        self.check_status(handle).await
    }
}

pub fn register_default_paystack_adapter(
    registry: &mut AdapterRegistry,
    adapter: Arc<PaystackAdapter>,
) {
    let adapter_id = AdapterId::from("adapter_paystack");
    for intent_kind in [
        PAYSTACK_INTENT_TRANSACTION_VERIFY,
        PAYSTACK_INTENT_REFUND_CREATE,
        PAYSTACK_INTENT_REFUND_VERIFY,
        PAYSTACK_INTENT_TRANSFER_CREATE,
        PAYSTACK_INTENT_TRANSFER_VERIFY,
    ] {
        registry.register_domain_adapter_for_intent(
            intent_kind,
            adapter_id.clone(),
            format!("kind={intent_kind}"),
            adapter.clone(),
        );
    }
}

fn normalize_payload(request: &AdapterExecutionRequest) -> Result<Value, AdapterExecutionError> {
    match request.intent_kind.as_str() {
        PAYSTACK_INTENT_TRANSACTION_VERIFY => {
            let parsed: PaystackTransactionVerifyPayloadSchema =
                serde_json::from_value(request.payload.clone()).map_err(|err| {
                    AdapterExecutionError::ContractViolation(format!(
                        "paystack.transaction.verify.v1 payload schema invalid: {err}"
                    ))
                })?;
            let reference = normalize_non_empty(parsed.reference, "reference")?;
            let mut out = Map::new();
            out.insert("reference".to_owned(), Value::String(reference));
            copy_optional_string(parsed.currency.as_deref(), "currency", &mut out)?;
            copy_optional_string(
                parsed.customer_reference.as_deref(),
                "customer_reference",
                &mut out,
            )?;
            copy_optional_string(parsed.expected_state.as_deref(), "expected_state", &mut out)?;
            copy_optional_string(
                parsed.connector_binding_id.as_deref(),
                "connector_binding_id",
                &mut out,
            )?;
            copy_optional_string(
                parsed.connector_reference.as_deref(),
                "connector_reference",
                &mut out,
            )?;
            if let Some(amount) = parsed.amount {
                if amount <= 0 {
                    return Err(AdapterExecutionError::ContractViolation(
                        "payload field `amount` must be a positive integer when provided"
                            .to_owned(),
                    ));
                }
                out.insert("amount".to_owned(), Value::Number(amount.into()));
            }
            Ok(Value::Object(out))
        }
        PAYSTACK_INTENT_REFUND_CREATE => {
            let parsed: PaystackRefundCreatePayloadSchema =
                serde_json::from_value(request.payload.clone()).map_err(|err| {
                    AdapterExecutionError::ContractViolation(format!(
                        "paystack.refund.create.v1 payload schema invalid: {err}"
                    ))
                })?;
            let payment_reference = normalize_non_empty_optional(
                parsed.payment_reference.as_deref(),
                "payment_reference",
            )?
            .ok_or_else(|| {
                AdapterExecutionError::ContractViolation(
                    "payload field `payment_reference` (or alias `transaction_reference`) is required"
                        .to_owned(),
                )
            })?;
            if parsed.amount <= 0 {
                return Err(AdapterExecutionError::ContractViolation(
                    "payload field `amount` must be a positive integer".to_owned(),
                ));
            }
            let mut out = Map::new();
            out.insert(
                "payment_reference".to_owned(),
                Value::String(payment_reference),
            );
            out.insert("amount".to_owned(), Value::Number(parsed.amount.into()));
            copy_optional_string(parsed.currency.as_deref(), "currency", &mut out)?;
            copy_optional_string(
                parsed.destination_reference.as_deref(),
                "destination_reference",
                &mut out,
            )?;
            copy_optional_string(parsed.reason_code.as_deref(), "reason_code", &mut out)?;
            copy_optional_string(
                parsed.connector_binding_id.as_deref(),
                "connector_binding_id",
                &mut out,
            )?;
            copy_optional_string(
                parsed.connector_reference.as_deref(),
                "connector_reference",
                &mut out,
            )?;
            Ok(Value::Object(out))
        }
        PAYSTACK_INTENT_REFUND_VERIFY => {
            let parsed: PaystackRefundVerifyPayloadSchema =
                serde_json::from_value(request.payload.clone()).map_err(|err| {
                    AdapterExecutionError::ContractViolation(format!(
                        "paystack.refund.verify.v1 payload schema invalid: {err}"
                    ))
                })?;
            let refund_id = normalize_non_empty_optional(parsed.refund_id.as_deref(), "refund_id")?
                .ok_or_else(|| {
                    AdapterExecutionError::ContractViolation(
                        "payload field `refund_id` (or alias `refund_reference`) is required"
                            .to_owned(),
                    )
                })?;
            let mut out = Map::new();
            out.insert("refund_id".to_owned(), Value::String(refund_id));
            copy_optional_string(
                parsed.connector_binding_id.as_deref(),
                "connector_binding_id",
                &mut out,
            )?;
            copy_optional_string(
                parsed.connector_reference.as_deref(),
                "connector_reference",
                &mut out,
            )?;
            Ok(Value::Object(out))
        }
        PAYSTACK_INTENT_TRANSFER_CREATE => {
            let parsed: PaystackTransferCreatePayloadSchema =
                serde_json::from_value(request.payload.clone()).map_err(|err| {
                    AdapterExecutionError::ContractViolation(format!(
                        "paystack.transfer.create.v1 payload schema invalid: {err}"
                    ))
                })?;
            let recipient_code =
                normalize_non_empty_optional(parsed.recipient_code.as_deref(), "recipient_code")?
                    .ok_or_else(|| {
                    AdapterExecutionError::ContractViolation(
                        "payload field `recipient_code` (or alias `recipient`) is required"
                            .to_owned(),
                    )
                })?;
            if parsed.amount <= 0 {
                return Err(AdapterExecutionError::ContractViolation(
                    "payload field `amount` must be a positive integer".to_owned(),
                ));
            }
            let mut out = Map::new();
            out.insert("recipient_code".to_owned(), Value::String(recipient_code));
            out.insert("amount".to_owned(), Value::Number(parsed.amount.into()));
            let reference = normalize_non_empty_optional(parsed.reference.as_deref(), "reference")?
                .unwrap_or_else(|| request.intent_id.to_string());
            out.insert("reference".to_owned(), Value::String(reference));
            copy_optional_string(parsed.currency.as_deref(), "currency", &mut out)?;
            copy_optional_string(parsed.reason.as_deref(), "reason", &mut out)?;
            copy_optional_string(parsed.source.as_deref(), "source", &mut out)?;
            copy_optional_string(
                parsed.connector_binding_id.as_deref(),
                "connector_binding_id",
                &mut out,
            )?;
            copy_optional_string(
                parsed.connector_reference.as_deref(),
                "connector_reference",
                &mut out,
            )?;
            Ok(Value::Object(out))
        }
        PAYSTACK_INTENT_TRANSFER_VERIFY => {
            let parsed: PaystackTransferVerifyPayloadSchema =
                serde_json::from_value(request.payload.clone()).map_err(|err| {
                    AdapterExecutionError::ContractViolation(format!(
                        "paystack.transfer.verify.v1 payload schema invalid: {err}"
                    ))
                })?;
            let reference = normalize_non_empty(parsed.reference, "reference")?;
            let mut out = Map::new();
            out.insert("reference".to_owned(), Value::String(reference));
            copy_optional_string(
                parsed.connector_binding_id.as_deref(),
                "connector_binding_id",
                &mut out,
            )?;
            copy_optional_string(
                parsed.connector_reference.as_deref(),
                "connector_reference",
                &mut out,
            )?;
            Ok(Value::Object(out))
        }
        other => Err(AdapterExecutionError::UnsupportedIntent(format!(
            "unsupported paystack intent kind `{other}`"
        ))),
    }
}

fn parse_execution_input(
    request: &AdapterExecutionRequest,
    payload: &Value,
) -> Result<PaystackExecutionInput, AdapterExecutionError> {
    let operation = match request.intent_kind.as_str() {
        PAYSTACK_INTENT_TRANSACTION_VERIFY => PaystackOperation::TransactionVerify,
        PAYSTACK_INTENT_REFUND_CREATE => PaystackOperation::RefundCreate,
        PAYSTACK_INTENT_REFUND_VERIFY => PaystackOperation::RefundVerify,
        PAYSTACK_INTENT_TRANSFER_CREATE => PaystackOperation::TransferCreate,
        PAYSTACK_INTENT_TRANSFER_VERIFY => PaystackOperation::TransferVerify,
        other => {
            return Err(AdapterExecutionError::UnsupportedIntent(format!(
                "unsupported paystack intent kind `{other}`"
            )))
        }
    };

    let connector_reference = request
        .metadata
        .get("connector.reference")
        .cloned()
        .or_else(|| extract_string(payload, &["connector_reference"]));
    let connector_binding_id = request
        .metadata
        .get("connector.binding_id")
        .cloned()
        .or_else(|| extract_string(payload, &["connector_binding_id"]));

    match operation {
        PaystackOperation::TransactionVerify => Ok(PaystackExecutionInput {
            intent_kind: request.intent_kind.to_string(),
            operation,
            reference: required_string(payload, &["reference"])?,
            amount_minor: payload.get("amount").and_then(Value::as_i64),
            currency: extract_string(payload, &["currency"]),
            source_reference: extract_string(payload, &["customer_reference"]),
            destination_reference: None,
            connector_binding_id,
            connector_reference,
            provider_body: payload.clone(),
        }),
        PaystackOperation::RefundCreate => Ok(PaystackExecutionInput {
            intent_kind: request.intent_kind.to_string(),
            operation,
            reference: required_string(payload, &["payment_reference"])?,
            amount_minor: Some(required_i64(payload, "amount")?),
            currency: extract_string(payload, &["currency"]),
            source_reference: extract_string(payload, &["payment_reference"]),
            destination_reference: extract_string(payload, &["destination_reference"]),
            connector_binding_id,
            connector_reference,
            provider_body: build_refund_provider_body(payload)?,
        }),
        PaystackOperation::RefundVerify => Ok(PaystackExecutionInput {
            intent_kind: request.intent_kind.to_string(),
            operation,
            reference: required_string(payload, &["refund_id"])?,
            amount_minor: None,
            currency: None,
            source_reference: None,
            destination_reference: None,
            connector_binding_id,
            connector_reference,
            provider_body: payload.clone(),
        }),
        PaystackOperation::TransferCreate => Ok(PaystackExecutionInput {
            intent_kind: request.intent_kind.to_string(),
            operation,
            reference: required_string(payload, &["reference"])?,
            amount_minor: Some(required_i64(payload, "amount")?),
            currency: extract_string(payload, &["currency"]),
            source_reference: extract_string(payload, &["source"]),
            destination_reference: extract_string(payload, &["recipient_code"]),
            connector_binding_id,
            connector_reference,
            provider_body: build_transfer_provider_body(payload)?,
        }),
        PaystackOperation::TransferVerify => Ok(PaystackExecutionInput {
            intent_kind: request.intent_kind.to_string(),
            operation,
            reference: required_string(payload, &["reference"])?,
            amount_minor: None,
            currency: None,
            source_reference: None,
            destination_reference: None,
            connector_binding_id,
            connector_reference,
            provider_body: payload.clone(),
        }),
    }
}

fn extract_paystack_secret_key(secrets: &BTreeMap<String, String>) -> Option<String> {
    ["secret_key", "paystack_secret_key", "api_key"]
        .iter()
        .find_map(|key| {
            secrets
                .get(*key)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
}

fn build_refund_provider_body(payload: &Value) -> Result<Value, AdapterExecutionError> {
    let mut out = Map::new();
    out.insert(
        "transaction".to_owned(),
        Value::String(required_string(payload, &["payment_reference"])?),
    );
    out.insert(
        "amount".to_owned(),
        Value::Number(required_i64(payload, "amount")?.into()),
    );
    if let Some(currency) = extract_string(payload, &["currency"]) {
        out.insert("currency".to_owned(), Value::String(currency));
    }
    if let Some(reason_code) = extract_string(payload, &["reason_code"]) {
        out.insert("merchant_note".to_owned(), Value::String(reason_code));
    }
    Ok(Value::Object(out))
}

fn build_transfer_provider_body(payload: &Value) -> Result<Value, AdapterExecutionError> {
    let mut out = Map::new();
    out.insert(
        "source".to_owned(),
        Value::String(extract_string(payload, &["source"]).unwrap_or_else(|| "balance".to_owned())),
    );
    out.insert(
        "amount".to_owned(),
        Value::Number(required_i64(payload, "amount")?.into()),
    );
    out.insert(
        "recipient".to_owned(),
        Value::String(required_string(payload, &["recipient_code"])?),
    );
    out.insert(
        "reference".to_owned(),
        Value::String(required_string(payload, &["reference"])?),
    );
    if let Some(reason) = extract_string(payload, &["reason"]) {
        out.insert("reason".to_owned(), Value::String(reason));
    }
    if let Some(currency) = extract_string(payload, &["currency"]) {
        out.insert("currency".to_owned(), Value::String(currency));
    }
    Ok(Value::Object(out))
}

fn classify_transaction_verify_response(
    input: &PaystackExecutionInput,
    response: PaystackApiResponse,
) -> AdapterExecutionEnvelope {
    if !response.status_code.is_success() || !response.envelope_ok {
        return classify_provider_error_response(
            "paystack.transaction_verify_failed",
            "paystack transaction verification failed",
            response,
            false,
            None,
        );
    }
    let data = response.data.clone().unwrap_or(Value::Null);
    let provider_status = extract_provider_status(&data);
    let provider_reference =
        extract_provider_reference(&data).or_else(|| Some(input.reference.clone()));
    match provider_status.as_deref() {
        Some(status) if is_transaction_success_status(status) => {
            succeeded_envelope(provider_reference, details_from_provider(&data, status))
        }
        Some(status) if is_transaction_pending_status(status) => in_progress_envelope(
            provider_reference,
            details_from_provider(&data, status),
            Some(5_000),
        ),
        Some(status) if is_failure_status(status) => terminal_envelope(
            "paystack.transaction_failed",
            format!("paystack transaction is in terminal state `{status}`"),
            Some(response.body),
        ),
        Some(status) => manual_review_envelope(
            "paystack.transaction_state_unknown",
            format!("paystack transaction returned unsupported state `{status}`"),
        ),
        None => manual_review_envelope(
            "paystack.transaction_missing_state",
            "paystack transaction verification response did not include a usable status".to_owned(),
        ),
    }
}

fn classify_refund_response(
    input: &PaystackExecutionInput,
    response: PaystackApiResponse,
    poll_after_ms: u64,
) -> AdapterExecutionEnvelope {
    if !response.status_code.is_success() || !response.envelope_ok {
        return classify_provider_error_response(
            "paystack.refund_failed",
            "paystack refund request failed",
            response,
            input.operation.is_create(),
            Some(poll_after_ms),
        );
    }
    let data = response.data.clone().unwrap_or(Value::Null);
    let provider_status = extract_provider_status(&data);
    let provider_reference = extract_remote_id(&data)
        .or_else(|| extract_provider_reference(&data))
        .or_else(|| Some(input.reference.clone()));
    match provider_status.as_deref() {
        Some(status) if is_refund_success_status(status) => {
            succeeded_envelope(provider_reference, details_from_provider(&data, status))
        }
        Some(status) if is_pending_status(status) => in_progress_envelope(
            provider_reference,
            details_from_provider(&data, status),
            Some(poll_after_ms),
        ),
        Some(status) if is_failure_status(status) => terminal_envelope(
            "paystack.refund_terminal_failure",
            format!("paystack refund is in terminal state `{status}`"),
            Some(response.body),
        ),
        Some(status) => manual_review_envelope(
            "paystack.refund_state_unknown",
            format!("paystack refund returned unsupported state `{status}`"),
        ),
        None => manual_review_envelope(
            "paystack.refund_missing_state",
            "paystack refund response did not include a usable status".to_owned(),
        ),
    }
}

fn classify_transfer_response(
    input: &PaystackExecutionInput,
    response: PaystackApiResponse,
    poll_after_ms: u64,
) -> AdapterExecutionEnvelope {
    if !response.status_code.is_success() || !response.envelope_ok {
        return classify_provider_error_response(
            "paystack.transfer_failed",
            "paystack transfer request failed",
            response,
            input.operation.is_create(),
            Some(poll_after_ms),
        );
    }
    let data = response.data.clone().unwrap_or(Value::Null);
    let provider_status = extract_provider_status(&data);
    let provider_reference = extract_provider_reference(&data)
        .or_else(|| extract_remote_id(&data))
        .or_else(|| Some(input.reference.clone()));
    match provider_status.as_deref() {
        Some(status) if is_transfer_success_status(status) => {
            succeeded_envelope(provider_reference, details_from_provider(&data, status))
        }
        Some(status) if is_pending_status(status) => in_progress_envelope(
            provider_reference,
            details_from_provider(&data, status),
            Some(poll_after_ms),
        ),
        Some(status) if is_failure_status(status) => terminal_envelope(
            "paystack.transfer_terminal_failure",
            format!("paystack transfer is in terminal state `{status}`"),
            Some(response.body),
        ),
        Some(status) => manual_review_envelope(
            "paystack.transfer_state_unknown",
            format!("paystack transfer returned unsupported state `{status}`"),
        ),
        None => manual_review_envelope(
            "paystack.transfer_missing_state",
            "paystack transfer response did not include a usable status".to_owned(),
        ),
    }
}

fn classify_provider_error_response(
    code: &str,
    message: &str,
    response: PaystackApiResponse,
    ambiguous_create: bool,
    retry_after_ms: Option<u64>,
) -> AdapterExecutionEnvelope {
    let status_code = response.status_code;
    if status_code == StatusCode::UNAUTHORIZED || status_code == StatusCode::FORBIDDEN {
        return blocked_envelope(
            "paystack.unauthorized",
            "paystack credentials were rejected by the provider".to_owned(),
        );
    }
    if status_code == StatusCode::NOT_FOUND {
        return terminal_envelope("paystack.not_found", response.message, Some(response.body));
    }
    if status_code == StatusCode::TOO_MANY_REQUESTS || status_code.is_server_error() {
        if ambiguous_create {
            return ambiguous_create_failure(
                "paystack.provider_outcome_unknown",
                "paystack returned a transient provider failure for a create operation; manual review is required",
                Some(response.body),
            );
        }
        return retryable_provider_failure(code, message, Some(response.body), retry_after_ms);
    }
    terminal_envelope(code, response.message, Some(response.body))
}

fn succeeded_envelope(
    provider_reference: Option<String>,
    details: BTreeMap<String, String>,
) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::Succeeded {
        provider_reference: provider_reference.clone(),
        details: details.clone(),
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn in_progress_envelope(
    provider_reference: Option<String>,
    details: BTreeMap<String, String>,
    poll_after_ms: Option<u64>,
) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::InProgress {
        provider_reference: provider_reference.clone(),
        details: details.clone(),
        poll_after_ms,
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn retryable_provider_failure(
    code: &str,
    message: &str,
    provider_details: Option<Value>,
    retry_after_ms: Option<u64>,
) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::RetryableFailure {
        code: code.to_owned(),
        message: message.to_owned(),
        retry_after_ms,
        provider_details,
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn terminal_envelope(
    code: &str,
    message: String,
    provider_details: Option<Value>,
) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::TerminalFailure {
        code: code.to_owned(),
        message,
        provider_details,
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn blocked_envelope(code: &str, message: String) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::Blocked {
        code: code.to_owned(),
        message,
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn manual_review_envelope(code: &str, message: String) -> AdapterExecutionEnvelope {
    let outcome = AdapterOutcome::ManualReview {
        code: code.to_owned(),
        message,
    };
    AdapterExecutionEnvelope {
        status: AdapterStatusSnapshot::from_outcome(&outcome),
        outcome,
    }
}

fn ambiguous_create_failure(
    code: &str,
    message: &str,
    provider_details: Option<Value>,
) -> AdapterExecutionEnvelope {
    let mut envelope = manual_review_envelope(code, message.to_owned());
    if let Some(provider_details) = provider_details {
        if let AdapterOutcome::ManualReview { code, message } = &envelope.outcome {
            envelope.status = AdapterStatusSnapshot {
                state: AdapterProgressState::ManualInterventionRequired,
                code: code.clone(),
                message: message.clone(),
                provider_reference: None,
                details: BTreeMap::from([(
                    "provider_details".to_owned(),
                    provider_details.to_string(),
                )]),
            };
        }
    }
    envelope
}

fn storage_projection(
    envelope: &AdapterExecutionEnvelope,
) -> (
    &str,
    Option<&str>,
    Option<&str>,
    Option<&Value>,
    Option<&str>,
    Option<&str>,
) {
    match &envelope.outcome {
        AdapterOutcome::Succeeded {
            provider_reference,
            details,
        } => (
            "succeeded",
            provider_reference.as_deref(),
            details.get("remote_id").map(String::as_str),
            None,
            None,
            None,
        ),
        AdapterOutcome::InProgress {
            provider_reference,
            details,
            ..
        } => (
            "pending",
            provider_reference.as_deref(),
            details.get("remote_id").map(String::as_str),
            None,
            None,
            None,
        ),
        AdapterOutcome::RetryableFailure {
            code,
            message,
            provider_details,
            ..
        } => (
            "retryable_failure",
            None,
            None,
            provider_details.as_ref(),
            Some(code.as_str()),
            Some(message.as_str()),
        ),
        AdapterOutcome::TerminalFailure {
            code,
            message,
            provider_details,
        } => (
            "failed_terminal",
            None,
            None,
            provider_details.as_ref(),
            Some(code.as_str()),
            Some(message.as_str()),
        ),
        AdapterOutcome::Blocked { code, message } => (
            "blocked",
            None,
            None,
            None,
            Some(code.as_str()),
            Some(message.as_str()),
        ),
        AdapterOutcome::ManualReview { code, message } => (
            "manual_review",
            None,
            None,
            None,
            Some(code.as_str()),
            Some(message.as_str()),
        ),
    }
}

fn envelope_from_row(row: &StoredExecutionRow) -> AdapterExecutionEnvelope {
    match row.status.as_str() {
        "succeeded" => succeeded_envelope(
            row.provider_reference
                .clone()
                .or_else(|| row.remote_id.clone()),
            details_from_row(row),
        ),
        "pending" | "dispatching" => in_progress_envelope(
            row.provider_reference
                .clone()
                .or_else(|| row.remote_id.clone()),
            details_from_row(row),
            Some(5_000),
        ),
        "retryable_failure" => retryable_provider_failure(
            row.last_error_code
                .as_deref()
                .unwrap_or("paystack.retryable_failure"),
            row.last_error_message
                .as_deref()
                .unwrap_or("paystack execution is temporarily unavailable"),
            row.last_response_json.clone(),
            Some(5_000),
        ),
        "blocked" => blocked_envelope(
            row.last_error_code.as_deref().unwrap_or("paystack.blocked"),
            row.last_error_message
                .clone()
                .unwrap_or_else(|| "paystack execution is blocked".to_owned()),
        ),
        "manual_review" => manual_review_envelope(
            row.last_error_code
                .as_deref()
                .unwrap_or("paystack.manual_review"),
            row.last_error_message
                .clone()
                .unwrap_or_else(|| "paystack execution requires manual review".to_owned()),
        ),
        _ => terminal_envelope(
            row.last_error_code
                .as_deref()
                .unwrap_or("paystack.failed_terminal"),
            row.last_error_message
                .clone()
                .unwrap_or_else(|| "paystack execution failed".to_owned()),
            row.last_response_json.clone(),
        ),
    }
}

fn status_snapshot_from_row(row: &StoredExecutionRow) -> AdapterStatusSnapshot {
    AdapterStatusSnapshot::from_outcome(&envelope_from_row(row).outcome)
}

fn details_from_row(row: &StoredExecutionRow) -> BTreeMap<String, String> {
    let mut details = BTreeMap::new();
    if let Some(remote_id) = row.remote_id.as_ref() {
        details.insert("remote_id".to_owned(), remote_id.clone());
    }
    if let Some(amount_minor) = row.amount_minor {
        details.insert("amount_minor".to_owned(), amount_minor.to_string());
    }
    if let Some(currency) = row.currency.as_ref() {
        details.insert("currency".to_owned(), currency.clone());
    }
    if let Some(source_reference) = row.source_reference.as_ref() {
        details.insert("source_reference".to_owned(), source_reference.clone());
    }
    if let Some(destination_reference) = row.destination_reference.as_ref() {
        details.insert(
            "destination_reference".to_owned(),
            destination_reference.clone(),
        );
    }
    if let Some(connector_reference) = row.connector_reference.as_ref() {
        details.insert(
            "connector_reference".to_owned(),
            connector_reference.clone(),
        );
    }
    details
}

fn details_from_provider(data: &Value, provider_status: &str) -> BTreeMap<String, String> {
    let mut details = BTreeMap::new();
    details.insert("provider_status".to_owned(), provider_status.to_owned());
    if let Some(reference) = extract_provider_reference(data) {
        details.insert("reference".to_owned(), reference);
    }
    if let Some(remote_id) = extract_remote_id(data) {
        details.insert("remote_id".to_owned(), remote_id);
    }
    if let Some(amount) = data.get("amount").and_then(Value::as_i64) {
        details.insert("amount_minor".to_owned(), amount.to_string());
    }
    if let Some(currency) = extract_string(data, &["currency"]) {
        details.insert("currency".to_owned(), currency);
    }
    details
}

fn extract_provider_status(data: &Value) -> Option<String> {
    extract_string(data, &["status", "transfer_status", "refund_status"])
        .map(|value| value.to_ascii_lowercase())
}

fn extract_provider_reference(data: &Value) -> Option<String> {
    extract_string(
        data,
        &["reference", "transaction_reference", "transfer_code"],
    )
}

fn extract_remote_id(data: &Value) -> Option<String> {
    if let Some(value) = data.get("id") {
        match value {
            Value::String(value) => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        }
    } else {
        None
    }
}

fn is_transaction_success_status(status: &str) -> bool {
    matches!(status, "success" | "successful" | "completed")
}

fn is_refund_success_status(status: &str) -> bool {
    matches!(status, "success" | "successful" | "processed" | "completed")
}

fn is_transfer_success_status(status: &str) -> bool {
    matches!(status, "success" | "successful" | "processed" | "approved")
}

fn is_pending_status(status: &str) -> bool {
    matches!(
        status,
        "pending" | "processing" | "queued" | "received" | "ongoing" | "otp"
    )
}

fn is_transaction_pending_status(status: &str) -> bool {
    is_pending_status(status) || matches!(status, "ongoing")
}

fn is_failure_status(status: &str) -> bool {
    matches!(
        status,
        "failed" | "failure" | "rejected" | "reversed" | "abandoned" | "cancelled" | "canceled"
    )
}

fn is_final_row_status(status: &str) -> bool {
    matches!(
        status,
        "succeeded" | "failed_terminal" | "blocked" | "manual_review"
    )
}

fn normalize_non_empty(value: String, field: &str) -> Result<String, AdapterExecutionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AdapterExecutionError::ContractViolation(format!(
            "payload field `{field}` must not be empty"
        )));
    }
    Ok(trimmed.to_owned())
}

fn normalize_non_empty_optional(
    value: Option<&str>,
    field: &str,
) -> Result<Option<String>, AdapterExecutionError> {
    match value.map(str::trim) {
        Some("") => Err(AdapterExecutionError::ContractViolation(format!(
            "payload field `{field}` must not be empty"
        ))),
        Some(value) => Ok(Some(value.to_owned())),
        None => Ok(None),
    }
}

fn copy_optional_string(
    value: Option<&str>,
    field: &str,
    out: &mut Map<String, Value>,
) -> Result<(), AdapterExecutionError> {
    if let Some(value) = normalize_non_empty_optional(value, field)? {
        out.insert(field.to_owned(), Value::String(value));
    }
    Ok(())
}

fn required_string(payload: &Value, keys: &[&str]) -> Result<String, AdapterExecutionError> {
    extract_string(payload, keys).ok_or_else(|| {
        AdapterExecutionError::ContractViolation(format!(
            "payload field `{}` is required",
            keys.first().copied().unwrap_or("value")
        ))
    })
}

fn required_i64(payload: &Value, field: &str) -> Result<i64, AdapterExecutionError> {
    payload.get(field).and_then(Value::as_i64).ok_or_else(|| {
        AdapterExecutionError::ContractViolation(format!(
            "payload field `{field}` must be an integer"
        ))
    })
}

fn extract_string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn map_reqwest_error(err: reqwest::Error) -> ProviderCallError {
    if err.is_timeout() {
        ProviderCallError::Timeout(err.to_string())
    } else {
        ProviderCallError::Transport(err.to_string())
    }
}

fn provider_call_details(err: ProviderCallError) -> Option<Value> {
    match err {
        ProviderCallError::Timeout(message) => Some(json!({
            "kind": "timeout",
            "message": message,
        })),
        ProviderCallError::Transport(message) => Some(json!({
            "kind": "transport",
            "message": message,
        })),
    }
}

fn truncate_for_json(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_owned();
    }
    let mut out = value.chars().take(max_len).collect::<String>();
    out.push_str("...");
    out
}

async fn ensure_paystack_schema(pool: &PgPool) -> anyhow::Result<()> {
    for stmt in [
        "CREATE SCHEMA IF NOT EXISTS paystack",
        r#"
        CREATE TABLE IF NOT EXISTS paystack.executions (
          intent_id             TEXT PRIMARY KEY,
          tenant_id             TEXT NOT NULL,
          job_id                TEXT,
          intent_kind           TEXT NOT NULL,
          operation             TEXT NOT NULL,
          status                TEXT NOT NULL,
          provider_reference    TEXT,
          remote_id             TEXT,
          request_payload_json  JSONB NOT NULL,
          last_response_json    JSONB,
          last_error_code       TEXT,
          last_error_message    TEXT,
          amount_minor          BIGINT,
          currency              TEXT,
          source_reference      TEXT,
          destination_reference TEXT,
          connector_reference   TEXT,
          created_at            TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
          updated_at            TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS paystack.attempts (
          id                 UUID PRIMARY KEY,
          intent_id          TEXT NOT NULL REFERENCES paystack.executions(intent_id) ON DELETE CASCADE,
          tenant_id          TEXT NOT NULL,
          job_id             TEXT,
          attempt_no         INT NOT NULL,
          phase              TEXT NOT NULL,
          provider_reference TEXT,
          error_code         TEXT,
          response_json      JSONB,
          created_at         TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
          updated_at         TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
        "CREATE INDEX IF NOT EXISTS paystack_executions_tenant_job_idx ON paystack.executions(tenant_id, job_id, updated_at DESC)",
        "CREATE INDEX IF NOT EXISTS paystack_executions_provider_reference_idx ON paystack.executions(provider_reference)",
        "CREATE INDEX IF NOT EXISTS paystack_executions_remote_id_idx ON paystack.executions(remote_id)",
        "CREATE INDEX IF NOT EXISTS paystack_attempts_intent_id_idx ON paystack.attempts(intent_id, created_at DESC)",
    ] {
        sqlx::query(stmt).execute(pool).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use execution_core::{
        AdapterExecutionRequest, AdapterId, IntentId, IntentKind, JobId, TenantId,
    };
    use sqlx::postgres::PgPoolOptions;

    fn request(intent_kind: &str, payload: Value) -> AdapterExecutionRequest {
        AdapterExecutionRequest {
            request_id: None,
            tenant_id: TenantId::from("tenant_demo"),
            intent_id: IntentId::from("intent_demo"),
            job_id: JobId::from("job_demo"),
            adapter_id: AdapterId::from("adapter_paystack"),
            attempt: 1,
            intent_kind: IntentKind::new(intent_kind),
            payload,
            correlation_id: None,
            idempotency_key: None,
            auth_context: None,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn normalize_refund_create_accepts_agent_shape() {
        let normalized = normalize_payload(&request(
            PAYSTACK_INTENT_REFUND_CREATE,
            json!({
                "payment_reference": "txn_123",
                "amount": 2500,
                "currency": "NGN",
                "reason_code": "customer_request"
            }),
        ))
        .expect("expected refund payload to normalize");
        assert_eq!(
            normalized.get("payment_reference").and_then(Value::as_str),
            Some("txn_123")
        );
        assert_eq!(
            normalized.get("currency").and_then(Value::as_str),
            Some("NGN")
        );
    }

    #[test]
    fn normalize_transfer_create_defaults_reference_to_intent_id() {
        let normalized = normalize_payload(&request(
            PAYSTACK_INTENT_TRANSFER_CREATE,
            json!({
                "recipient_code": "RCP_123",
                "amount": 9000,
                "currency": "NGN"
            }),
        ))
        .expect("expected transfer payload to normalize");
        assert_eq!(
            normalized.get("reference").and_then(Value::as_str),
            Some("intent_demo")
        );
    }

    #[test]
    fn classify_transaction_verify_success_maps_to_succeeded() {
        let input = PaystackExecutionInput {
            intent_kind: PAYSTACK_INTENT_TRANSACTION_VERIFY.to_owned(),
            operation: PaystackOperation::TransactionVerify,
            reference: "ref_123".to_owned(),
            amount_minor: Some(5000),
            currency: Some("NGN".to_owned()),
            source_reference: None,
            destination_reference: None,
            connector_binding_id: None,
            connector_reference: None,
            provider_body: json!({ "reference": "ref_123" }),
        };
        let envelope = classify_transaction_verify_response(
            &input,
            PaystackApiResponse {
                status_code: StatusCode::OK,
                body: json!({
                    "status": true,
                    "message": "Verification successful",
                    "data": {"status": "success", "reference": "ref_123", "id": 99}
                }),
                data: Some(json!({"status": "success", "reference": "ref_123", "id": 99})),
                message: "Verification successful".to_owned(),
                envelope_ok: true,
            },
        );
        assert!(matches!(envelope.outcome, AdapterOutcome::Succeeded { .. }));
    }

    #[test]
    fn validate_intent_accepts_connector_bound_execution_without_global_secret() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("expected tokio runtime");
        runtime.block_on(async {
            let pool = PgPoolOptions::new()
                .connect_lazy("postgresql://postgres:postgres@localhost/azums_test")
                .expect("expected lazy postgres pool");
            let adapter = PaystackAdapter::new(
                pool,
                PaystackAdapterConfig {
                    secret_key: None,
                    connector_broker_base_url: Some("http://127.0.0.1:8082".to_owned()),
                    connector_broker_bearer_token: Some("dev-ingress-token".to_owned()),
                    connector_broker_principal_id: Some("execution-worker".to_owned()),
                    ..PaystackAdapterConfig::default()
                },
            )
            .expect("expected adapter construction to succeed");
            let mut request = request(
                PAYSTACK_INTENT_REFUND_CREATE,
                json!({
                    "payment_reference": "txn_123",
                    "amount": 2500,
                    "currency": "NGN",
                    "connector_binding_id": "paystack_live"
                }),
            );
            request.auth_context = Some(execution_core::AuthContext {
                principal_id: None,
                submitter_kind: None,
                auth_scheme: None,
                channel: None,
                agent_id: None,
                environment_id: Some("prod".to_owned()),
                runtime_type: None,
                runtime_identity: None,
                trust_tier: None,
                risk_tier: None,
            });
            assert!(adapter.validate_intent(&request).is_ok());
        });
    }
}
