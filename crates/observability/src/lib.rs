use http::{HeaderMap, HeaderName, HeaderValue};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Duration;
use thiserror::Error;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

static TRACING_INIT: OnceCell<()> = OnceCell::new();
static METRICS_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub service_name: String,
    pub environment: String,
    pub log_filter: String,
    pub json_logs: bool,
    pub metrics_prefix: String,
    pub request_id_header: String,
    pub correlation_id_header: String,
}

impl ObservabilityConfig {
    pub fn from_env(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_owned(),
            environment: env_or("OBS_ENV", "dev"),
            log_filter: env_or("OBS_LOG_FILTER", "info"),
            json_logs: env_bool("OBS_LOG_JSON", false),
            metrics_prefix: env_or("OBS_METRICS_PREFIX", "platform"),
            request_id_header: env_or("OBS_REQUEST_ID_HEADER", "x-request-id"),
            correlation_id_header: env_or("OBS_CORRELATION_ID_HEADER", "x-correlation-id"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestContext {
    pub request_id: String,
    pub correlation_id: String,
}

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("invalid header name `{header}`")]
    InvalidHeaderName { header: String },
    #[error("invalid header value for `{header}`")]
    InvalidHeaderValue { header: String },
    #[error("failed to initialize tracing subscriber: {details}")]
    TracingInit { details: String },
    #[error("failed to initialize metrics recorder: {details}")]
    MetricsInit { details: String },
}

pub fn init_tracing(config: &ObservabilityConfig) -> Result<(), ObservabilityError> {
    if TRACING_INIT.get().is_some() {
        return Ok(());
    }

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.log_filter.clone()))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let _ = config.json_logs;
    let init_result = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .compact()
        .try_init();

    init_result.map_err(|err| ObservabilityError::TracingInit {
        details: err.to_string(),
    })?;

    let _ = TRACING_INIT.set(());
    Ok(())
}

pub fn init_metrics() -> Result<PrometheusHandle, ObservabilityError> {
    if let Some(handle) = METRICS_HANDLE.get() {
        return Ok(handle.clone());
    }

    let handle = PrometheusBuilder::new().install_recorder().map_err(|err| {
        ObservabilityError::MetricsInit {
            details: err.to_string(),
        }
    })?;
    let _ = METRICS_HANDLE.set(handle.clone());
    Ok(METRICS_HANDLE.get().cloned().unwrap_or(handle))
}

pub fn render_metrics() -> Option<String> {
    METRICS_HANDLE.get().map(PrometheusHandle::render)
}

pub fn derive_request_context(headers: &HeaderMap, cfg: &ObservabilityConfig) -> RequestContext {
    let request_id =
        header_value(headers, &cfg.request_id_header).unwrap_or_else(|| Uuid::new_v4().to_string());
    let correlation_id =
        header_value(headers, &cfg.correlation_id_header).unwrap_or_else(|| request_id.clone());

    RequestContext {
        request_id,
        correlation_id,
    }
}

pub fn apply_request_context(
    headers: &mut HeaderMap,
    cfg: &ObservabilityConfig,
    ctx: &RequestContext,
) -> Result<(), ObservabilityError> {
    let request_id_name = parse_header_name(&cfg.request_id_header)?;
    let correlation_id_name = parse_header_name(&cfg.correlation_id_header)?;

    headers.insert(
        request_id_name,
        HeaderValue::from_str(&ctx.request_id).map_err(|_| {
            ObservabilityError::InvalidHeaderValue {
                header: cfg.request_id_header.clone(),
            }
        })?,
    );
    headers.insert(
        correlation_id_name,
        HeaderValue::from_str(&ctx.correlation_id).map_err(|_| {
            ObservabilityError::InvalidHeaderValue {
                header: cfg.correlation_id_header.clone(),
            }
        })?,
    );
    Ok(())
}

pub fn record_http_request(
    cfg: &ObservabilityConfig,
    method: &str,
    path: &str,
    status: u16,
    latency: Duration,
) {
    let normalized_path = normalize_path(path).into_owned();
    let status_code = status.to_string();
    let status_class = format!("{}xx", status / 100);

    let counter_name = format!("{}_http_requests_total", cfg.metrics_prefix);
    let histogram_name = format!("{}_http_request_duration_seconds", cfg.metrics_prefix);

    metrics::counter!(
        counter_name,
        "service" => cfg.service_name.clone(),
        "environment" => cfg.environment.clone(),
        "method" => method.to_uppercase(),
        "path" => normalized_path.clone(),
        "status" => status_code,
        "status_class" => status_class
    )
    .increment(1);

    metrics::histogram!(
        histogram_name,
        "service" => cfg.service_name.clone(),
        "environment" => cfg.environment.clone(),
        "method" => method.to_uppercase(),
        "path" => normalized_path
    )
    .record(latency.as_secs_f64());
}

pub fn normalize_path(path: &str) -> Cow<'_, str> {
    if path.is_empty() || path == "/" {
        return Cow::Borrowed("/");
    }

    let trimmed = path.split('?').next().unwrap_or(path);
    let mut segments = Vec::new();
    for segment in trimmed.trim_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        if is_dynamic_segment(segment) {
            segments.push(":id".to_owned());
        } else {
            segments.push(segment.to_owned());
        }
    }

    if segments.is_empty() {
        Cow::Borrowed("/")
    } else {
        Cow::Owned(format!("/{}", segments.join("/")))
    }
}

fn is_dynamic_segment(segment: &str) -> bool {
    if segment.parse::<u64>().is_ok() {
        return true;
    }

    if Uuid::parse_str(segment).is_ok() {
        return true;
    }

    let looks_hex = segment.len() >= 16 && segment.chars().all(|c| c.is_ascii_hexdigit());
    if looks_hex {
        return true;
    }

    false
}

fn parse_header_name(raw: &str) -> Result<HeaderName, ObservabilityError> {
    HeaderName::from_bytes(raw.as_bytes()).map_err(|_| ObservabilityError::InvalidHeaderName {
        header: raw.to_owned(),
    })
}

fn header_value(headers: &HeaderMap, header_name: &str) -> Option<String> {
    let name = HeaderName::from_bytes(header_name.as_bytes()).ok()?;
    let value = headers.get(name)?;
    let raw = value.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_owned())
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
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
    use super::*;

    #[test]
    fn normalizes_dynamic_segments() {
        assert_eq!(normalize_path("/requests/123"), "/requests/:id");
        assert_eq!(
            normalize_path("/requests/9f8b2209-3524-4095-a5fa-c8de15b7e84e/history"),
            "/requests/:id/history"
        );
        assert_eq!(
            normalize_path("/solana/abcdef0123456789abcdef0123456789"),
            "/solana/:id"
        );
    }

    #[test]
    fn keeps_stable_path_segments() {
        assert_eq!(normalize_path("/status/health"), "/status/health");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("/status/jobs?state=queued"), "/status/jobs");
    }

    #[test]
    fn derives_request_and_correlation_ids() {
        let cfg = ObservabilityConfig::from_env("test-service");
        let headers = HeaderMap::new();
        let ctx = derive_request_context(&headers, &cfg);
        assert!(!ctx.request_id.is_empty());
        assert_eq!(ctx.request_id, ctx.correlation_id);
    }
}
