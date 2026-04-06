# Ingress API

`ingress_api` is the normalized-intent entrypoint for execution core.

## Endpoints

- `GET /health`
- `GET /metrics`
- `POST /api/requests`
- `POST /webhooks/:source`
- `POST /webhooks/paystack/:tenant_id`

## `POST /api/requests`

Body:

```json
{
  "intent_kind": "solana.transfer.v1",
  "payload": {},
  "metadata": {}
}
```

Metering note:

- Requests tagged with `metadata["metering.scope"]="playground"` are accepted/executed normally
  but excluded from free-play quota and billable usage counting.

Required headers:

- `x-tenant-id: <tenant>`
- `x-principal-id: <principal id>`
- `x-submitter-kind: api_key_holder|signed_webhook_sender|internal_service|wallet_backend`
- `Authorization: Bearer <token>` for non-`api_key_holder` submitters
- `x-api-key: <key>` when `x-submitter-kind=api_key_holder` and API-key auth is enabled

## `POST /webhooks/:source`

Body: arbitrary JSON payload.

Required headers:

- `x-tenant-id: <tenant>`
- `x-principal-id: <principal id>`
- `x-submitter-kind: api_key_holder|signed_webhook_sender|internal_service|wallet_backend`
- `Authorization: Bearer <token>` for non-`api_key_holder` submitters
- `x-api-key: <key>` when `x-submitter-kind=api_key_holder` and API-key auth is enabled

Optional:

- `x-intent-kind` to override `webhook.<source>.v1`.
- `x-webhook-signature` when tenant webhook secret is configured.
  If `x-submitter-kind=signed_webhook_sender`, webhook signature validation is required.
- `x-idempotency-key` to attach a stable idempotency identifier to normalized intent.

## `POST /webhooks/paystack/:tenant_id`

Dedicated Paystack evidence-ingestion route.

- Verifies `x-paystack-signature` using `INGRESS_PAYSTACK_WEBHOOK_SECRETS`.
- Stores the webhook durably in `paystack.webhook_events`.
- Correlates the event back to `paystack.executions` and the latest receipt when possible.
- Does not submit execution truth or invoke adapters directly.

## Env

- `DATABASE_URL` (required)
- `INGRESS_API_BIND` (default `0.0.0.0:8081`)
- `INGRESS_DB_MAX_CONNECTIONS` (default `8`)
- `INGRESS_BEARER_TOKEN` (global bearer token)
- `INGRESS_TENANT_TOKENS` (`tenant_a:token_a,tenant_b:token_b`)
- `INGRESS_API_KEY` (global API key)
- `INGRESS_TENANT_API_KEYS` (`tenant_a:key_a,tenant_b:key_b`)
- `INGRESS_WEBHOOK_SIGNATURE_SECRETS` (`tenant_a:secret_a,...`)
- `INGRESS_PAYSTACK_WEBHOOK_SECRETS` (`tenant_a:paystack_secret_a,...`)
- `INGRESS_PRINCIPAL_SUBMITTER_BINDINGS` (`svc_a=internal_service;tenant_client=api_key_holder`; supports wildcard principal keys, e.g. `workspace-*=internal_service`)
- `INGRESS_REQUIRE_PRINCIPAL_SUBMITTER_BINDING` (default `true`)
- `INGRESS_PRINCIPAL_TENANT_BINDINGS` (`svc_a=tenant_a|tenant_b;tenant_client=tenant_a`; supports wildcard principal keys and tenant wildcards, e.g. `workspace-*=tenant_ws_*`)
- `INGRESS_REQUIRE_PRINCIPAL_TENANT_BINDING` (default `true`)
- `INGRESS_REQUIRE_PRINCIPAL_ID` (default `true`)
- `INGRESS_REQUIRE_SUBMITTER_KIND` (default `true`)
- `INGRESS_REQUIRE_API_KEY_FOR_API_KEY_HOLDER` (default `true`)
- `INGRESS_API_ALLOWED_SUBMITTERS` (default `api_key_holder,internal_service,wallet_backend`)
- `INGRESS_WEBHOOK_ALLOWED_SUBMITTERS` (default `signed_webhook_sender,internal_service`)
- `INGRESS_INTENT_ROUTES` (`kind=adapter;kind2=adapter2`)
- `INGRESS_INTENT_SCHEMAS` (`kind=schema_id;kind2=schema_id2`)
- `INGRESS_REQUIRE_SCHEMA_FOR_ALL_ROUTES` (default `true`)
- `INGRESS_DEFAULT_QUOTA_PLAN` (default `developer`)
- `INGRESS_DEFAULT_QUOTA_ACCESS_MODE` (default `free_play`)
- `INGRESS_DEFAULT_FREE_PLAY_LIMIT` (default `500`)
- `INGRESS_DEFAULT_EXECUTION_POLICY` (default `customer_signed`)
- `INGRESS_DEFAULT_SPONSORED_MONTHLY_CAP_REQUESTS` (default `10000`)
- `INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED` (default `false`)
- `INGRESS_EXECUTION_POLICY_CANARY_TENANTS` (optional CSV tenant canary allow-list)
- `EXECUTION_DISPATCH_QUEUE` (default `execution.dispatch`)
- `EXECUTION_CALLBACK_QUEUE` (default `execution.callback`)
- `OBS_ENV` (default `dev`)
- `OBS_LOG_FILTER` (default `info`)
- `OBS_LOG_JSON` (default `false`)
- `OBS_METRICS_PREFIX` (default `platform`)
- `OBS_REQUEST_ID_HEADER` (default `x-request-id`)
- `OBS_CORRELATION_ID_HEADER` (default `x-correlation-id`)

## Schema Enforcement

Ingress validates payloads per intent kind before submission to execution core.

Ingress also normalizes and stores first-class contract fields on intent:

- `request_id`
- `correlation_id`
- `idempotency_key`
- `auth_context` (principal, submitter kind, auth scheme, channel)

Built-in schema ids:

- `solana.transfer.v1`
- `solana.broadcast.v1`
- `paystack.transaction.verify.v1`
- `paystack.refund.create.v1`
- `paystack.refund.verify.v1`
- `paystack.transfer.create.v1`
- `paystack.transfer.verify.v1`

Default mapping:

```text
solana.transfer.v1=solana.transfer.v1;solana.broadcast.v1=solana.broadcast.v1;paystack.transaction.verify.v1=paystack.transaction.verify.v1;paystack.refund.create.v1=paystack.refund.create.v1;paystack.refund.verify.v1=paystack.refund.verify.v1;paystack.transfer.create.v1=paystack.transfer.create.v1;paystack.transfer.verify.v1=paystack.transfer.verify.v1
```

Paystack routes are opt-in at runtime. To enable the fiat rail, append the Paystack intent kinds to
`INGRESS_INTENT_ROUTES` and configure the execution worker with either:

- `PAYSTACK_SECRET_KEY` as a worker-wide fallback, or
- the connector-broker path:
  - `EXECUTION_CONNECTOR_BROKER_BASE_URL`
  - `EXECUTION_CONNECTOR_BROKER_BEARER_TOKEN`
  - `EXECUTION_CONNECTOR_BROKER_PRINCIPAL_ID`

For tenant/environment-scoped execution, include `connector_binding_id` on the Paystack payload or
emit `connector.binding_id` metadata so the worker resolves the secret through the protected
connector-binding path instead of using a global fallback.

## Execution Policy Enforcement

Ingress tenant profile now supports:

- `customer_signed`
- `customer_managed_signer`
- `sponsored` (capped by `sponsored_monthly_cap_requests`)

When execution policy enforcement is enabled for a tenant and policy is
`customer_signed` or `customer_managed_signer`, Solana submits must include
`signed_tx_base64` (or alias `signed_tx_b64` / `signed_tx`).

## Durable Intake Audits

Ingress now records durable intake decision rows for both accepted and rejected requests in:

- `ingress_api_intake_audits`

Each row captures request/tenant/channel identity, intent/idempotency context, validation result,
rejection reason (when rejected), and accepted intent/job IDs (when accepted).
