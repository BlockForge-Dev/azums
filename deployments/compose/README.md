# Local Compose Stack

This stack wires the full request lifecycle locally:

1. `reverse_proxy` receives public traffic.
2. `ingress_api` normalizes requests and submits intents.
3. `execution_worker` leases/dispatches jobs and processes callbacks.
4. `status_api` exposes request/job/receipt/history/query endpoints.
5. `operator_ui` provides dashboard query/replay controls for operators.
6. `postgres` stores durable truth.

## Start

```bash
cd deployments/compose
cp .env.example .env
docker compose up
```

Alternative: use the centralized config profile and copy
`crates/config/profiles/dev-compose.env.example` to `deployments/compose/.env`.

Proxy is exposed at `http://localhost:8000`.
Operator UI is exposed at `http://localhost:8083`.

## Smoke Test

Submit through proxy:

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

Then query status through proxy (note `/status/*` path):

```bash
curl "http://localhost:8000/status/requests/<intent_id>" \
  -H "authorization: Bearer dev-status-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: demo-operator" \
  -H "x-principal-role: admin"
```

Replay through proxy:

```bash
curl -X POST "http://localhost:8000/status/requests/<intent_id>/replay" \
  -H "authorization: Bearer dev-status-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: demo-operator" \
  -H "x-principal-role: admin" \
  -H "content-type: application/json" \
  -d '{"reason":"manual replay test"}'
```

Configure tenant callback destination (admin only):

```bash
curl -X POST "http://localhost:8000/status/tenant/callback-destination" \
  -H "authorization: Bearer dev-status-token" \
  -H "x-tenant-id: tenant_demo" \
  -H "x-principal-id: demo-operator" \
  -H "x-principal-role: admin" \
  -H "content-type: application/json" \
  -d '{
    "delivery_url":"https://example.com/callback",
    "timeout_ms":10000,
    "allow_private_destinations":false,
    "allowed_hosts":["example.com"],
    "enabled":true
  }'
```

Visit operator dashboard: `http://localhost:8083`

## Notes

- `REVERSE_PROXY_STRIP_STATUS_PREFIX=true` is enabled by default, so `/status/requests/:id` is forwarded to status API as `/requests/:id`.
- The sample Solana request may end in a blocked/manual state unless signing secrets and signer runtime are configured in the worker.
