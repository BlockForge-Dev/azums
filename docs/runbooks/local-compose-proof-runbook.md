# Local Compose Proof Runbook

## Purpose

This is the pinned local proof workflow for the `azums-proof` compose project.

Use it when you want to:

- rebuild the local proof images
- start a clean proof stack
- run the benchmark harness
- run the crash-injection harness

## Frozen Proof Surface

Compose project:

- `azums-proof`

Compose file:

- `deployments/docker/docker-compose.images.yml`

Direct service endpoints used by proof scripts:

- reverse proxy: `http://127.0.0.1:18000`
- ingress api: `http://127.0.0.1:18081`
- status api: `http://127.0.0.1:18082`

Immutable local proof tag:

- `freeze-local-proof-20260311-0915`

## Image Set

Required for proof:

- `azums/ingress_api:freeze-local-proof-20260311-0915`
- `azums/status_api:freeze-local-proof-20260311-0915`
- `azums/execution_worker:freeze-local-proof-20260311-0915`
- `azums/reverse_proxy:freeze-local-proof-20260311-0915`
- `postgres:16-alpine`

Present in the compose file but not required by the proof scripts:

- `azums/operator_ui:freeze-local-proof-20260311-0915`
- `azums/operator_ui_next:freeze-local-proof-20260311-0915`

## Rebuild Commands

Run from repo root: `C:\Users\HP\azums`

```powershell
docker build -f deployments/docker/Dockerfile `
  --build-arg APP_MANIFEST=apps/ingress_api/Cargo.toml `
  --build-arg BIN_NAME=ingress_api `
  --build-arg INCLUDE_SOLANA_SIGNER=false `
  -t azums/ingress_api:local .

docker build -f deployments/docker/Dockerfile `
  --build-arg APP_MANIFEST=crates/status_api/Cargo.toml `
  --build-arg BIN_NAME=status_api `
  --build-arg INCLUDE_SOLANA_SIGNER=false `
  -t azums/status_api:local .

docker build -f deployments/docker/Dockerfile `
  --build-arg APP_MANIFEST=apps/admin_cli/Cargo.toml `
  --build-arg BIN_NAME=execution_core_worker `
  --build-arg INCLUDE_SOLANA_SIGNER=true `
  -t azums/execution_worker:local .

docker build -f deployments/docker/Dockerfile `
  --build-arg APP_MANIFEST=crates/reverse-proxy/Cargo.toml `
  --build-arg BIN_NAME=reverse_proxy `
  --build-arg INCLUDE_SOLANA_SIGNER=false `
  -t azums/reverse_proxy:local .
```

## Clean Start

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
$env:AZUMS_POSTGRES_DB="azums"
$env:AZUMS_POSTGRES_USER="app"
$env:AZUMS_POSTGRES_PASSWORD="app"
$env:AZUMS_DATABASE_URL="postgres://app:app@postgres:5432/azums"
docker compose -p azums-proof -f deployments/docker/docker-compose.images.yml down -v --remove-orphans
docker volume rm azums-proof_postgres_data 2>$null
docker compose -p azums-proof -f deployments/docker/docker-compose.images.yml up -d
```

## Health Check

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
pwsh -File scripts/check_platform_health.ps1 `
  -Runtime compose `
  -ComposeProject azums-proof `
  -BaseUrl http://127.0.0.1:18000
```

## Benchmarks

Synthetic success:

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
pwsh -File scripts/benchmark_platform.ps1 `
  -Runtime compose `
  -ComposeProject azums-proof `
  -BaseUrl http://127.0.0.1:18000 `
  -Scenario synthetic_success `
  -RequestCount 20 `
  -SubmitConcurrency 8 `
  -TerminalTimeoutSec 300
```

Duplicate/idempotency:

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
pwsh -File scripts/benchmark_platform.ps1 `
  -Runtime compose `
  -ComposeProject azums-proof `
  -BaseUrl http://127.0.0.1:18000 `
  -Scenario synthetic_success `
  -RequestCount 20 `
  -SubmitConcurrency 8 `
  -DuplicateGroupSize 4 `
  -TerminalTimeoutSec 300
```

Degraded RPC:

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
pwsh -File scripts/benchmark_platform.ps1 `
  -Runtime compose `
  -ComposeProject azums-proof `
  -BaseUrl http://127.0.0.1:18000 `
  -Scenario rpc_timeout `
  -RequestCount 4 `
  -SubmitConcurrency 2 `
  -TerminalTimeoutSec 420
```

## Crash Injection

```powershell
$env:AZUMS_IMAGE_TAG="freeze-local-proof-20260311-0915"
pwsh -File scripts/run_crash_injection.ps1 `
  -Runtime compose `
  -ComposeProject azums-proof `
  -BaseUrl http://127.0.0.1:18000
```

Expected result:

- concise scenario lines during execution
- one summary table at the end
- `Crash-injection suite passed.`

## Notes

- The proof scripts submit directly to `18081` and query directly from `18082`.
- Reverse proxy `18000` is still used for platform health checks.
- `benchmark_platform.ps1` now waits for full compose readiness before it starts submitting.
- `run_crash_injection.ps1` now suppresses successful `docker compose` churn and only prints Docker output when a compose action fails.
- The compose proof stack ignores generic host env like `DATABASE_URL`; use the `AZUMS_*` variables above if you need to override the local proof database config.

## Current Baseline

Synthetic success, `20` requests, concurrency `8`:

- accepted: `20/20`
- terminal: `20/20`
- throughput: `0.88 req/s`
- acceptance latency: `p50 623 ms`, `p95 1151 ms`
- worker pickup delay: `p50 3681 ms`, `p95 7345 ms`
- accepted to final: `p50 4443 ms`, `p95 8135 ms`
- accepted to callback delivered: `p50 5241 ms`, `p95 8625 ms`

Duplicate/idempotency, `20` requests, duplicate groups of `4`:

- accepted: `20/20`
- unique accepted executions: `5`
- duplicate groups stable: `5/5`
- duplicate groups with multi-execution: `0`
- accepted to final: `p50 678 ms`, `p95 749 ms`

Degraded RPC, `4` requests, concurrency `2`:

- accepted: `4/4`
- terminal: `4/4 dead_lettered`
- throughput: `0.11 req/s`
- acceptance latency: `p50 650 ms`, `p95 1072 ms`
- worker pickup delay: `p50 5827 ms`, `p95 7288 ms`
- accepted to final: `p50 30199 ms`, `p95 33057 ms`
- retry amplification average: `4`
- attempt count average: `5`

Crash-injection suite:

- `worker_during_execution`: passed
- `postgres_during_processing`: passed
- `ingress_during_intake`: passed
