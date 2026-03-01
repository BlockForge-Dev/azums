use crate::error::CallbackCoreError;
use crate::model::{DeliveryFailureClass, DispatchFailure, DispatchOutcome};
use async_trait::async_trait;
use execution_core::CallbackJob;
use hmac::{Hmac, Mac};
use reqwest::header::RETRY_AFTER;
use sha2::Sha256;
use std::collections::HashSet;
use std::net::IpAddr;
use url::Url;

type HmacSha256 = Hmac<Sha256>;

#[async_trait]
pub trait CallbackDispatcher: Send + Sync {
    async fn dispatch(&self, callback: &CallbackJob) -> Result<DispatchOutcome, DispatchFailure>;
}

#[derive(Default)]
pub struct StdoutCallbackDispatcher;

#[async_trait]
impl CallbackDispatcher for StdoutCallbackDispatcher {
    async fn dispatch(&self, callback: &CallbackJob) -> Result<DispatchOutcome, DispatchFailure> {
        let body = serde_json::to_string(callback).map_err(|err| DispatchFailure {
            class: DeliveryFailureClass::Serialization,
            code: "callback.serialize_failed".to_owned(),
            message: format!("failed to serialize callback payload: {err}"),
            retryable: false,
            http_status: None,
            retry_after_secs: None,
            response_excerpt: None,
        })?;

        println!("callback_core delivery: {body}");
        Ok(DispatchOutcome {
            http_status: 200,
            response_excerpt: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HttpCallbackDispatcherConfig {
    pub delivery_url: String,
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
    pub signature_secret: Option<String>,
    pub signature_key_id: Option<String>,
    pub allowed_hosts: Option<HashSet<String>>,
    pub allow_private_destinations: bool,
}

impl HttpCallbackDispatcherConfig {
    pub fn new(delivery_url: impl Into<String>) -> Self {
        Self {
            delivery_url: delivery_url.into(),
            bearer_token: None,
            timeout_ms: 10_000,
            signature_secret: None,
            signature_key_id: None,
            allowed_hosts: None,
            allow_private_destinations: false,
        }
    }
}

pub struct HttpCallbackDispatcher {
    client: reqwest::Client,
    cfg: HttpCallbackDispatcherConfig,
}

impl HttpCallbackDispatcher {
    pub fn new(mut cfg: HttpCallbackDispatcherConfig) -> Result<Self, CallbackCoreError> {
        normalize_allowed_hosts(&mut cfg);
        validate_destination(&cfg)?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(cfg.timeout_ms.max(1)))
            .build()
            .map_err(|err| {
                CallbackCoreError::Configuration(format!(
                    "failed to build callback http client: {err}"
                ))
            })?;

        Ok(Self { client, cfg })
    }
}

#[async_trait]
impl CallbackDispatcher for HttpCallbackDispatcher {
    async fn dispatch(&self, callback: &CallbackJob) -> Result<DispatchOutcome, DispatchFailure> {
        let payload = serde_json::to_vec(callback).map_err(|err| DispatchFailure {
            class: DeliveryFailureClass::Serialization,
            code: "callback.serialize_failed".to_owned(),
            message: format!("failed to serialize callback payload: {err}"),
            retryable: false,
            http_status: None,
            retry_after_secs: None,
            response_excerpt: None,
        })?;

        let mut req = self
            .client
            .post(&self.cfg.delivery_url)
            .header("content-type", "application/json")
            .header("x-callback-id", callback.callback_id.as_str())
            .body(payload.clone());

        if let Some(token) = &self.cfg.bearer_token {
            req = req.bearer_auth(token);
        }

        if let Some(secret) = &self.cfg.signature_secret {
            match sign_payload(secret, callback.callback_id.as_str(), &payload) {
                Ok((timestamp_ms, signature)) => {
                    req = req
                        .header("x-callback-timestamp-ms", timestamp_ms.to_string())
                        .header("x-callback-signature", signature);
                    if let Some(key_id) = &self.cfg.signature_key_id {
                        req = req.header("x-callback-signature-key-id", key_id);
                    }
                }
                Err(err) => {
                    return Err(DispatchFailure {
                        class: DeliveryFailureClass::Internal,
                        code: "callback.signature_failed".to_owned(),
                        message: err.to_string(),
                        retryable: false,
                        http_status: None,
                        retry_after_secs: None,
                        response_excerpt: None,
                    });
                }
            }
        }

        let response = req.send().await.map_err(|err| {
            let class = if err.is_timeout() {
                DeliveryFailureClass::Timeout
            } else {
                DeliveryFailureClass::Transport
            };
            DispatchFailure {
                class,
                code: "callback.request_failed".to_owned(),
                message: format!("callback request failed: {err}"),
                retryable: true,
                http_status: None,
                retry_after_secs: None,
                response_excerpt: None,
            }
        })?;

        let status = response.status();
        let retry_after_secs = parse_retry_after_seconds(response.headers().get(RETRY_AFTER));
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<unreadable body>".to_owned());
        let excerpt = truncate_body(&body);

        if status.is_success() {
            return Ok(DispatchOutcome {
                http_status: status.as_u16(),
                response_excerpt: excerpt,
            });
        }

        let status_u16 = status.as_u16();
        let retryable = status.is_server_error() || matches!(status_u16, 408 | 409 | 425 | 429);
        let class = if status.is_server_error() {
            DeliveryFailureClass::Http5xx
        } else {
            DeliveryFailureClass::Http4xx
        };

        Err(DispatchFailure {
            class,
            code: format!("callback.http_{status_u16}"),
            message: format!("callback endpoint returned status {status_u16}"),
            retryable,
            http_status: Some(status_u16),
            retry_after_secs,
            response_excerpt: excerpt,
        })
    }
}

fn normalize_allowed_hosts(cfg: &mut HttpCallbackDispatcherConfig) {
    if let Some(hosts) = &cfg.allowed_hosts {
        let normalized: HashSet<String> = hosts
            .iter()
            .map(|host| host.trim().to_ascii_lowercase())
            .filter(|host| !host.is_empty())
            .collect();
        cfg.allowed_hosts = if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        };
    }
}

fn validate_destination(cfg: &HttpCallbackDispatcherConfig) -> Result<(), CallbackCoreError> {
    let url = Url::parse(&cfg.delivery_url).map_err(|err| {
        CallbackCoreError::Configuration(format!("invalid callback delivery url: {err}"))
    })?;

    let scheme = url.scheme().to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(CallbackCoreError::Configuration(
            "callback url must use http or https".to_owned(),
        ));
    }

    let host = url.host_str().ok_or_else(|| {
        CallbackCoreError::Configuration("callback url must include a host".to_owned())
    })?;
    let host_lower = host.to_ascii_lowercase();

    if let Some(allowed_hosts) = &cfg.allowed_hosts {
        if !allowed_hosts.contains(&host_lower) {
            return Err(CallbackCoreError::Security(format!(
                "callback host `{host}` is not in the allowed host list"
            )));
        }
    }

    if !cfg.allow_private_destinations {
        if host_lower == "localhost" || host_lower.ends_with(".local") {
            return Err(CallbackCoreError::Security(format!(
                "callback host `{host}` is not allowed"
            )));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_or_local_ip(ip) {
                return Err(CallbackCoreError::Security(format!(
                    "callback ip `{host}` is not allowed"
                )));
            }
        }
    }

    Ok(())
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

fn sign_payload(
    secret: &str,
    callback_id: &str,
    payload: &[u8],
) -> Result<(u64, String), CallbackCoreError> {
    let timestamp_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|err| {
        CallbackCoreError::Configuration(format!("invalid signature secret: {err}"))
    })?;

    mac.update(timestamp_ms.to_string().as_bytes());
    mac.update(b".");
    mac.update(callback_id.as_bytes());
    mac.update(b".");
    mac.update(payload);
    let digest = hex::encode(mac.finalize().into_bytes());

    Ok((timestamp_ms, format!("v1={digest}")))
}

fn parse_retry_after_seconds(retry_after: Option<&reqwest::header::HeaderValue>) -> Option<i64> {
    retry_after
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .map(|seconds| seconds.clamp(1, 900))
}

fn truncate_body(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut out: String = trimmed.chars().take(512).collect();
    if trimmed.chars().count() > out.chars().count() {
        out.push_str("...");
    }
    Some(out)
}
