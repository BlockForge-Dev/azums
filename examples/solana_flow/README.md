# Solana Flow Example

This example sends a normalized Solana transfer intent through the public proxy path.

## Files

- `submit-request.json`: request body for `POST /api/requests`
- `replay-request.json`: body for `POST /status/requests/:id/replay`
- `callback-destination.json`: body for `POST /status/tenant/callback-destination`
- `run.sh`: Bash end-to-end run
- `run.ps1`: PowerShell end-to-end run

## Run

PowerShell:

```powershell
./examples/solana_flow/run.ps1
```

Bash:

```bash
./examples/solana_flow/run.sh
```

## Optional Env Overrides

- `BASE_URL` (default `http://localhost:8000`)
- `TENANT_ID` (default `tenant_demo`)
- `INGRESS_TOKEN` (default `dev-ingress-token`)
- `STATUS_TOKEN` (default `dev-status-token`)
- `INGRESS_PRINCIPAL_ID` (default `ingress-service`)
- `STATUS_PRINCIPAL_ID` (default `demo-operator`)
- `STATUS_PRINCIPAL_ROLE` (default `admin`)
- `APPLY_CALLBACK_DESTINATION` (`true|false`, default `false`)
