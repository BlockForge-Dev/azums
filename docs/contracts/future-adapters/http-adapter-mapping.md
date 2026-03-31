# HTTP Adapter Mapping

## First plausible intents

- internal HTTP call
- outbound webhook-like delivery with durable receipt
- partner API invocation

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `http` |
| likely normalized intent kinds | `http.call`, `http.partner.invoke`, `http.delivery.send` |
| stable execution references | request id, remote request id, response id when present |
| likely `fetch_status(...)` | optional, only for asynchronous APIs |

## Expected facts

- method
- host/path template
- destination identity
- request body hash
- idempotency key
- expected status class or code

## Observed facts

- observed status code
- response body hash
- response headers allowlist
- remote request id
- timeout/failure posture

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `status_code_mismatch` | `state_mismatch` | `high` | `investigate` |
| `response_body_hash_mismatch` | `state_mismatch` | `high` | `investigate` |
| `destination_mismatch` | `destination_mismatch` | `high` | `investigate` |
| `timeout_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `authentication_rejected` | `policy_violation` | `high` | `investigate` |
| `duplicate_delivery_signal` | `duplicate_signal` | `info` | `false_positive` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local request table | expected method, destination, body hash, idempotency key |
| adapter-local response/attempt table | observed status, remote id, timing, retry posture |
| allowlisted request/response excerpts | operator-safe body/header evidence |

## Non-goals for v1

- full semantic validation of third-party API response payloads
- arbitrary HTML or binary response inspection in the operator UI
