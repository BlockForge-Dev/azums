use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::OnceCell;

use crate::provider::dedupe_provider_urls;

#[derive(Debug, Clone)]
pub struct RpcErrorObj {
    pub code: i64,
    pub message: String,
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct RpcSigStatus {
    pub err: Option<Value>,
    pub confirmation_status: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RpcCallError {
    Transport(String),
    Provider(RpcErrorObj),
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub outcome: String,
    pub err: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct RpcRouteSelection<T> {
    pub value: T,
    pub provider_used: String,
}

#[derive(Debug, Clone)]
pub struct SolanaRpcClientConfig {
    pub timeout_ms: u64,
}

impl Default for SolanaRpcClientConfig {
    fn default() -> Self {
        Self { timeout_ms: 15_000 }
    }
}

pub struct SolanaRpcClient {
    client: Client,
}

impl SolanaRpcClient {
    pub fn new(cfg: SolanaRpcClientConfig) -> Result<Self, RpcCallError> {
        let timeout_ms = cfg.timeout_ms.max(1);
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|err| RpcCallError::Transport(format!("rpc client build failed: {err}")))?;
        Ok(Self { client })
    }

    async fn post_result(&self, rpc_url: &str, body: Value) -> Result<Value, RpcCallError> {
        let response = self
            .client
            .post(rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|err| RpcCallError::Transport(format!("rpc send failed: {err}")))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|err| RpcCallError::Transport(format!("rpc response read failed: {err}")))?;

        if status.as_u16() == 429 {
            return Err(RpcCallError::Transport("rpc rate limited (429)".to_owned()));
        }

        if !status.is_success() {
            return Err(RpcCallError::Transport(format!(
                "rpc http {}: {}",
                status.as_u16(),
                truncate_detail(text, 220)
            )));
        }

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|err| RpcCallError::Transport(format!("invalid rpc json payload: {err}")))?;

        if let Some(error_value) = parsed.get("error").cloned() {
            let code = error_value
                .get("code")
                .and_then(Value::as_i64)
                .unwrap_or(-1);
            let message = error_value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown rpc error")
                .to_owned();
            return Err(RpcCallError::Provider(RpcErrorObj {
                code,
                message,
                raw: error_value,
            }));
        }

        parsed
            .get("result")
            .cloned()
            .ok_or_else(|| RpcCallError::Transport("rpc response missing `result`".to_owned()))
    }

    pub async fn get_latest_blockhash(&self, rpc_url: &str) -> Result<String, RpcCallError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": [{ "commitment": "confirmed" }]
        });

        let result = self.post_result(rpc_url, body).await?;
        result
            .get("value")
            .and_then(|v| v.get("blockhash"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                RpcCallError::Transport("rpc latest blockhash missing in result".to_owned())
            })
    }

    pub async fn simulate_transaction(
        &self,
        rpc_url: &str,
        signed_tx_base64: &str,
    ) -> Result<SimulationResult, RpcCallError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": [
                signed_tx_base64,
                {
                    "encoding": "base64",
                    "sigVerify": false,
                    "replaceRecentBlockhash": false,
                    "commitment": "processed"
                }
            ]
        });

        let result = self.post_result(rpc_url, body).await?;
        let value = result.get("value").cloned().unwrap_or(Value::Null);
        let err = value.get("err").cloned().filter(|v| !v.is_null());
        let units = value.get("unitsConsumed").and_then(Value::as_u64);

        let outcome = match (&err, units) {
            (None, Some(units)) => format!("ok_units_{units}"),
            (None, None) => "ok".to_owned(),
            (Some(err), _) => format!("error_{}", truncate_detail(err.to_string(), 120)),
        };

        Ok(SimulationResult { outcome, err })
    }

    pub async fn send_transaction(
        &self,
        rpc_url: &str,
        signed_tx_base64: &str,
        skip_preflight: bool,
    ) -> Result<String, RpcCallError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                signed_tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": skip_preflight,
                    "preflightCommitment": "processed",
                    "maxRetries": 0
                }
            ]
        });

        let result = self.post_result(rpc_url, body).await?;
        result
            .as_str()
            .map(ToOwned::to_owned)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| {
                RpcCallError::Transport("rpc sendTransaction missing signature".to_owned())
            })
    }

    pub async fn get_signature_status(
        &self,
        rpc_url: &str,
        signature: &str,
    ) -> Result<Option<RpcSigStatus>, RpcCallError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [
                [signature],
                { "searchTransactionHistory": true }
            ]
        });

        let result = self.post_result(rpc_url, body).await?;
        let maybe_status = result
            .get("value")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .cloned();

        let Some(status_value) = maybe_status else {
            return Ok(None);
        };

        let err = status_value.get("err").cloned().filter(|v| !v.is_null());
        let confirmation_status = status_value
            .get("confirmationStatus")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        Ok(Some(RpcSigStatus {
            err,
            confirmation_status,
        }))
    }

    pub async fn get_latest_blockhash_with_failover(
        &self,
        rpc_urls: &[String],
    ) -> Result<RpcRouteSelection<String>, RpcCallError> {
        let targets = normalize_rpc_targets(rpc_urls);
        let mut last_error = None;

        for (index, rpc_url) in targets.iter().enumerate() {
            match self.get_latest_blockhash(rpc_url).await {
                Ok(value) => {
                    return Ok(RpcRouteSelection {
                        value,
                        provider_used: rpc_url.clone(),
                    })
                }
                Err(error) => {
                    let error = annotate_rpc_error(rpc_url, error);
                    let can_failover =
                        index + 1 < targets.len() && should_failover_on_error(&error);
                    if !can_failover {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            RpcCallError::Transport("no Solana RPC endpoints configured".to_owned())
        }))
    }

    pub async fn simulate_transaction_with_failover(
        &self,
        rpc_urls: &[String],
        signed_tx_base64: &str,
    ) -> Result<RpcRouteSelection<SimulationResult>, RpcCallError> {
        let targets = normalize_rpc_targets(rpc_urls);
        let mut last_error = None;

        for (index, rpc_url) in targets.iter().enumerate() {
            match self.simulate_transaction(rpc_url, signed_tx_base64).await {
                Ok(value) => {
                    return Ok(RpcRouteSelection {
                        value,
                        provider_used: rpc_url.clone(),
                    })
                }
                Err(error) => {
                    let error = annotate_rpc_error(rpc_url, error);
                    let can_failover =
                        index + 1 < targets.len() && should_failover_on_error(&error);
                    if !can_failover {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            RpcCallError::Transport("no Solana RPC endpoints configured".to_owned())
        }))
    }

    pub async fn send_transaction_with_failover(
        &self,
        rpc_urls: &[String],
        signed_tx_base64: &str,
        skip_preflight: bool,
    ) -> Result<RpcRouteSelection<String>, RpcCallError> {
        let targets = normalize_rpc_targets(rpc_urls);
        let mut last_error = None;

        for (index, rpc_url) in targets.iter().enumerate() {
            match self
                .send_transaction(rpc_url, signed_tx_base64, skip_preflight)
                .await
            {
                Ok(value) => {
                    return Ok(RpcRouteSelection {
                        value,
                        provider_used: rpc_url.clone(),
                    })
                }
                Err(error) => {
                    let error = annotate_rpc_error(rpc_url, error);
                    let can_failover =
                        index + 1 < targets.len() && should_failover_on_error(&error);
                    if !can_failover {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            RpcCallError::Transport("no Solana RPC endpoints configured".to_owned())
        }))
    }

    pub async fn get_signature_status_with_failover(
        &self,
        rpc_urls: &[String],
        signature: &str,
    ) -> Result<RpcRouteSelection<Option<RpcSigStatus>>, RpcCallError> {
        let targets = normalize_rpc_targets(rpc_urls);
        let mut last_error = None;
        let mut last_provider_without_status = None;

        for rpc_url in &targets {
            match self.get_signature_status(rpc_url, signature).await {
                Ok(Some(value)) => {
                    return Ok(RpcRouteSelection {
                        value: Some(value),
                        provider_used: rpc_url.clone(),
                    })
                }
                Ok(None) => {
                    last_provider_without_status = Some(rpc_url.clone());
                }
                Err(error) => {
                    let error = annotate_rpc_error(rpc_url, error);
                    if !should_failover_on_error(&error) {
                        return Err(error);
                    }
                    last_error = Some(error);
                }
            }
        }

        if let Some(provider_used) = last_provider_without_status.or_else(|| targets.first().cloned())
        {
            return Ok(RpcRouteSelection {
                value: None,
                provider_used,
            });
        }

        Err(last_error.unwrap_or_else(|| {
            RpcCallError::Transport("no Solana RPC endpoints configured".to_owned())
        }))
    }
}

static SOLANA_RPC_CLIENT: OnceCell<SolanaRpcClient> = OnceCell::const_new();

pub async fn shared_solana_rpc_client() -> Result<&'static SolanaRpcClient, RpcCallError> {
    SOLANA_RPC_CLIENT
        .get_or_try_init(|| async {
            let timeout_ms = std::env::var("SOLANA_RPC_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(15_000);
            SolanaRpcClient::new(SolanaRpcClientConfig { timeout_ms })
        })
        .await
}

pub async fn rpc_get_latest_blockhash(rpc_url: &str) -> Result<String, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .get_latest_blockhash(rpc_url)
        .await
}

pub async fn rpc_get_latest_blockhash_with_failover(
    rpc_urls: &[String],
) -> Result<RpcRouteSelection<String>, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .get_latest_blockhash_with_failover(rpc_urls)
        .await
}

pub async fn rpc_simulate_transaction(
    rpc_url: &str,
    signed_tx_base64: &str,
) -> Result<SimulationResult, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .simulate_transaction(rpc_url, signed_tx_base64)
        .await
}

pub async fn rpc_simulate_transaction_with_failover(
    rpc_urls: &[String],
    signed_tx_base64: &str,
) -> Result<RpcRouteSelection<SimulationResult>, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .simulate_transaction_with_failover(rpc_urls, signed_tx_base64)
        .await
}

pub async fn rpc_send_transaction(
    rpc_url: &str,
    signed_tx_base64: &str,
    skip_preflight: bool,
) -> Result<String, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .send_transaction(rpc_url, signed_tx_base64, skip_preflight)
        .await
}

pub async fn rpc_send_transaction_with_failover(
    rpc_urls: &[String],
    signed_tx_base64: &str,
    skip_preflight: bool,
) -> Result<RpcRouteSelection<String>, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .send_transaction_with_failover(rpc_urls, signed_tx_base64, skip_preflight)
        .await
}

pub async fn rpc_get_signature_status(
    rpc_url: &str,
    signature: &str,
) -> Result<Option<RpcSigStatus>, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .get_signature_status(rpc_url, signature)
        .await
}

pub async fn rpc_get_signature_status_with_failover(
    rpc_urls: &[String],
    signature: &str,
) -> Result<RpcRouteSelection<Option<RpcSigStatus>>, RpcCallError> {
    shared_solana_rpc_client()
        .await?
        .get_signature_status_with_failover(rpc_urls, signature)
        .await
}

fn normalize_rpc_targets(rpc_urls: &[String]) -> Vec<String> {
    dedupe_provider_urls(rpc_urls.to_vec())
}

fn should_failover_on_error(error: &RpcCallError) -> bool {
    match error {
        RpcCallError::Transport(_) => true,
        RpcCallError::Provider(provider) => {
            let message = provider.message.to_ascii_lowercase();
            provider.code == -32005
                || message.contains("rate limit")
                || message.contains("too many requests")
                || message.contains("temporarily unavailable")
                || message.contains("unhealthy")
                || message.contains("timeout")
                || message.contains("overloaded")
        }
    }
}

fn annotate_rpc_error(rpc_url: &str, error: RpcCallError) -> RpcCallError {
    match error {
        RpcCallError::Transport(message) => {
            RpcCallError::Transport(format!("{message} [rpc_url={rpc_url}]"))
        }
        RpcCallError::Provider(provider) => RpcCallError::Provider(RpcErrorObj {
            code: provider.code,
            message: format!("{} [rpc_url={}]", provider.message, rpc_url),
            raw: provider.raw,
        }),
    }
}

fn truncate_detail(mut value: String, max_len: usize) -> String {
    if value.len() <= max_len {
        return value;
    }

    value.truncate(max_len);
    value.push_str("...");
    value
}
