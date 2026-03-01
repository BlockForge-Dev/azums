# Examples

Runnable examples for the Azums platform APIs.

| Folder | Flow | What It Covers |
|---|---|---|
| `solana_flow/` | Direct API request to Solana intent | Submit, query status/receipt/history, replay, callback destination |
| `webhook_to_solana/` | Webhook intake routed to Solana intent | Webhook submit, optional signature header, status lookup |

## Defaults

Examples assume local compose defaults:

- Base URL: `http://localhost:8000`
- Tenant: `tenant_demo`
- Ingress token: `dev-ingress-token`
- Status token: `dev-status-token`

## Quick Start

PowerShell:

```powershell
./examples/solana_flow/run.ps1
./examples/webhook_to_solana/run.ps1
```

Bash:

```bash
chmod +x examples/solana_flow/run.sh examples/webhook_to_solana/run.sh
./examples/solana_flow/run.sh
./examples/webhook_to_solana/run.sh
```
