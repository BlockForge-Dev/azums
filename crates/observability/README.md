# Observability

Shared observability helpers for Azums services.

## What This Crate Provides

| Capability | API |
|---|---|
| Standardized service observability config from env | `ObservabilityConfig::from_env` |
| One-time tracing subscriber initialization | `init_tracing` |
| One-time Prometheus metrics recorder initialization | `init_metrics` |
| Render Prometheus text exposition payload | `render_metrics` |
| Request/correlation context derivation | `derive_request_context` |
| Request/correlation header propagation | `apply_request_context` |
| HTTP request metrics recording | `record_http_request` |
| Path normalization to reduce metric-cardinality | `normalize_path` |

## Environment Variables

| Variable | Default | Purpose |
|---|---|---|
| `OBS_ENV` | `dev` | Environment tag for logs/metrics |
| `OBS_LOG_FILTER` | `info` | Fallback log filter when `RUST_LOG` is unset |
| `OBS_LOG_JSON` | `false` | Reserved toggle for JSON logs; current default output is compact text |
| `OBS_METRICS_PREFIX` | `platform` | Prefix for metric names |
| `OBS_REQUEST_ID_HEADER` | `x-request-id` | Request ID header contract |
| `OBS_CORRELATION_ID_HEADER` | `x-correlation-id` | Correlation ID header contract |

## Minimal Integration Example

```rust
use observability::{
    apply_request_context, derive_request_context, init_tracing, record_http_request,
    ObservabilityConfig,
};
use std::time::Instant;

fn bootstrap() {
    let obs = ObservabilityConfig::from_env("status_api");
    init_tracing(&obs).expect("observability init");

    // per request:
    // let start = Instant::now();
    // let ctx = derive_request_context(headers, &obs);
    // apply_request_context(upstream_headers, &obs, &ctx).expect("request context headers");
    // record_http_request(&obs, "GET", "/requests/123", 200, start.elapsed());
}
```

## Intended Use

- `apps/ingress_api`
- `crates/status_api`
- `apps/operator_ui`
- `apps/admin_cli` (worker-level logging/metrics)

The reverse proxy has additional custom observability behavior and can adopt shared helpers
selectively.
