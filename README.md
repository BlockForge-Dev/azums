# Azums Execution Platform

Multi-service execution platform with a strict execution core, adapter contract boundaries, callback delivery tracking, and tenant-scoped status APIs.

Full product and technical documentation:

- [`docs/PLATFORM_DOCUMENTATION.md`](docs/PLATFORM_DOCUMENTATION.md)
- [`docs/README.md`](docs/README.md)

## Architecture

Implemented components in this repo:

1. Ingress API (`apps/ingress_api`)
2. Execution Core (`crates/execution_core`)
3. Adapter Contract (`crates/adapter_contract`)
4. Solana Adapter (`crates/adapter_solana`)
5. Callback / Delivery Core (`crates/callback_core`)
6. Status API / Query Layer (`crates/status_api`)
7. RPC Layer (`crates/rpc_layer`)
8. Reverse Proxy (`crates/reverse-proxy`)
9. Worker runtime (`apps/admin_cli`)
10. Operator UI (`apps/operator_ui`)
11. Shared Types (`crates/shared_types`)
12. Observability Helpers (`crates/observability`)

## End-to-End Flow

1. Client sends request or webhook to reverse proxy.
2. Reverse proxy forwards `/api/*` and `/webhooks/*` to ingress.
3. Ingress authenticates, validates, normalizes, and submits durable intent.
4. Execution core records submission, routes adapter, enqueues dispatch job.
5. Worker leases dispatch jobs and invokes adapter.
6. Adapter returns structured normalized outcomes.
7. Execution core records transitions + receipts and schedules retry or terminal state.
8. Callback jobs are enqueued only after terminal durable state is written.
9. Callback worker delivers outbound callbacks and records delivery attempts/history.
10. Status API serves request state, receipt, history, callback history, and replay actions.

## Local Run (Compose)

Use the pre-wired stack in [`deployments/compose`](deployments/compose):

```bash
cd deployments/compose
cp .env.example .env
docker compose up
```

Default public entrypoint: `http://localhost:8000`

Important default routing:

- `/api/*` -> ingress
- `/webhooks/*` -> ingress
- `/status/*` -> status API pool
- `REVERSE_PROXY_STRIP_STATUS_PREFIX=true` by default, so `/status/requests/:id` becomes `/requests/:id` upstream

## Smoke Test

Submit a request:

```bash
curl -X POST "http://localhost:8000/api/requests" \
  -H "authorization: Bearer dev-ingress-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: ingress-service" \
  -H "x-submitter-kind: internal_service" \
  -H "content-type: application/json" \
  -d '{
    "intent_kind":"solana.transfer.v1",
    "payload":{
      "intent_id":"intent_demo_001",
      "intent_type":"transfer",
      "to_addr":"11111111111111111111111111111111",
      "amount":1
    }
  }'
```

Query status:

```bash
curl "http://localhost:8000/status/requests/<intent_id>" \
  -H "authorization: Bearer dev-status-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: demo-operator" \
  -H "x-principal-role: admin"
```

Replay:

```bash
curl -X POST "http://localhost:8000/status/requests/<intent_id>/replay" \
  -H "authorization: Bearer dev-status-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: demo-operator" \
  -H "x-principal-role: admin" \
  -H "content-type: application/json" \
  -d '{"reason":"manual replay test"}'
```

## Examples

Runnable end-to-end examples live in [`examples/`](examples/README.md):

1. `examples/solana_flow` (API submit, status, receipt, history, replay)
2. `examples/webhook_to_solana` (webhook submit routed to Solana intent)

## Service Docs

- Ingress API: [`apps/ingress_api/README.md`](apps/ingress_api/README.md)
- Execution Core: [`crates/execution_core/README.md`](crates/execution_core/README.md)
- Status API: [`crates/status_api/README.md`](crates/status_api/README.md)
- Reverse Proxy: [`crates/reverse-proxy/README.md`](crates/reverse-proxy/README.md)
- Compose stack: [`deployments/compose/README.md`](deployments/compose/README.md)
- Operator UI: [`apps/operator_ui/README.md`](apps/operator_ui/README.md)
- Shared Types: [`crates/shared_types/README.md`](crates/shared_types/README.md)
- Config Pack: [`crates/config/README.md`](crates/config/README.md)
- Observability: [`crates/observability/README.md`](crates/observability/README.md)

## Deployment Targets

- Compose (source-mounted dev stack): [`deployments/compose/README.md`](deployments/compose/README.md)
- Docker (prebuilt images): [`deployments/docker/README.md`](deployments/docker/README.md)
- Kubernetes (baseline manifests): [`deployments/k8s/README.md`](deployments/k8s/README.md)
- CI image/deploy workflows: [`.github/workflows/docker-build.yml`](.github/workflows/docker-build.yml), [`.github/workflows/docker-publish.yml`](.github/workflows/docker-publish.yml), [`.github/workflows/k8s-deploy.yml`](.github/workflows/k8s-deploy.yml)
