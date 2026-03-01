# Operator UI

`operator_ui` is the operator-facing dashboard for durable execution truth.

It reads from `status_api` through a server-side proxy and exposes:

- job list and state filtering
- request status + receipt + history + callback history
- replay trigger (`POST /requests/:id/replay`)
- tenant callback destination management
- tenant ingress intake audit history

## Run

```bash
cd apps/operator_ui
cargo run
```

Default bind: `http://0.0.0.0:8083`

## Environment

- `OPERATOR_UI_BIND` (default `0.0.0.0:8083`)
- `OPERATOR_UI_STATUS_BASE_URL` (default `http://127.0.0.1:8000/status`)
- `OPERATOR_UI_STATUS_BEARER_TOKEN` (optional bearer token forwarded to status API)
- `OPERATOR_UI_TENANT_ID` (default `tenant_demo`)
- `OPERATOR_UI_PRINCIPAL_ID` (default `demo-operator`)
- `OPERATOR_UI_PRINCIPAL_ROLE` (default `admin`)
- `OPERATOR_UI_STATUS_TIMEOUT_MS` (default `15000`)

## Notes

- The browser does not call `status_api` directly; it calls typed endpoints under `/api/ui/status/*` on this service.
- Auth headers are injected by the server from environment config.
- Operator UI backend deserializes status responses using `shared_types::status_api` DTOs for a shared end-to-end contract.
