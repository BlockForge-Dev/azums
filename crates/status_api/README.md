# Status API

`status_api` is the read/query layer for execution and delivery status.

## Endpoints

- `GET /health`
- `GET /metrics`
- `GET /requests/:id`
- `GET /requests/:id/receipt`
- `GET /requests/:id/history`
- `GET /requests/:id/callbacks`
- `GET /jobs?state=...&limit=...&offset=...`
- `POST /requests/:id/replay`
- `GET /tenant/callback-destination`
- `POST /tenant/callback-destination`
- `DELETE /tenant/callback-destination`
- `GET /tenant/intake-audits?validation_result=accepted|rejected&channel=api|webhook&limit=...&offset=...`

Reverse-proxy compatibility paths (same handlers):

- `GET /status/health`
- `GET /status/metrics`
- `GET /status/requests/:id`
- `GET /status/requests/:id/receipt`
- `GET /status/requests/:id/history`
- `GET /status/requests/:id/callbacks`
- `GET /status/jobs?state=...&limit=...&offset=...`
- `POST /status/requests/:id/replay`
- `GET /status/tenant/callback-destination`
- `POST /status/tenant/callback-destination`
- `DELETE /status/tenant/callback-destination`
- `GET /status/tenant/intake-audits?validation_result=accepted|rejected&channel=api|webhook&limit=...&offset=...`

## Auth Headers

Required headers:

- `authorization: Bearer <token>`
- `x-tenant-id`
- `x-principal-id`

Optional headers:

- `x-principal-role` (`viewer`, `operator`, `admin`; defaults to `viewer`)
- `x-request-id`

Replay is restricted to `admin` by default authorizer.
Query/replay access is also gated by auth bindings:

- bearer token checks (`STATUS_API_BEARER_TOKEN` and/or `STATUS_API_TENANT_TOKENS`)
- principal-role binding (`STATUS_API_PRINCIPAL_ROLE_BINDINGS`)
- principal-tenant binding (`STATUS_API_PRINCIPAL_TENANT_BINDINGS`)

Callback destination management (`/tenant/callback-destination`) is restricted to tenant `admin`.

## Durable Sources

- `execution_core_intents`
- `execution_core_jobs`
- `execution_core_state_transitions`
- `execution_core_receipts`
- `callback_core_deliveries` (optional)
- `callback_core_delivery_attempts` (optional)
- `ingress_api_intake_audits` (optional)

`GET /requests/:id` includes normalized identity/context fields when present:

- `request_id`
- `correlation_id`
- `idempotency_key`
- `auth_context`

## Auditing

`status_api` writes:

- `status_api_query_audit`
- `status_api_operator_action_audit`

## Run

```bash
cd crates/status_api
DATABASE_URL=postgres://... cargo run
```

Optional:

- `STATUS_API_BIND` (default `0.0.0.0:8082`)
- `STATUS_API_DB_MAX_CONNECTIONS` (default `8`)
- `STATUS_API_BEARER_TOKEN` (global bearer token)
- `STATUS_API_TENANT_TOKENS` (per-tenant tokens, e.g. `tenant_a:token_a;tenant_b:token_b`)
- `STATUS_API_REQUIRE_BEARER_AUTH` (default `true`)
- `STATUS_API_PRINCIPAL_ROLE_BINDINGS` (e.g. `alice:viewer;ops-admin:admin`)
- `STATUS_API_REQUIRE_PRINCIPAL_ROLE_BINDING` (default `true`)
- `STATUS_API_PRINCIPAL_TENANT_BINDINGS` (e.g. `alice:tenant_a|tenant_b;ops-admin:tenant_a`)
- `STATUS_API_REQUIRE_PRINCIPAL_TENANT_BINDING` (default `true`)
- `STATUS_API_REDACT_FAILURE_PROVIDER_DETAILS_FOR_VIEWER` (default `true`)
- `STATUS_API_REDACT_CALLBACK_ERROR_DETAILS_FOR_VIEWER` (default `true`)
- `OBS_ENV` (default `dev`)
- `OBS_LOG_FILTER` (default `info`)
- `OBS_LOG_JSON` (default `false`)
- `OBS_METRICS_PREFIX` (default `platform`)
- `OBS_REQUEST_ID_HEADER` (default `x-request-id`)
- `OBS_CORRELATION_ID_HEADER` (default `x-correlation-id`)
