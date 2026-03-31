# Benchmark Runbook

## Purpose

This runbook benchmarks Azums from the outside using durable request, receipt, and callback data.

The goal is operational truth, not vanity throughput.

It measures:

- request acceptance latency
- queue enqueue latency
- worker pickup delay
- adapter execution window
- receipt finalization window
- callback dispatch latency
- accepted -> final state latency
- accepted -> callback delivered latency

## Benchmark Script

Use:

`scripts/benchmark_platform.ps1`

## Baseline Example

Synthetic success is the safest repeatable benchmark because it exercises the durable core path without requiring live-chain signing.

```powershell
pwsh -File scripts/benchmark_platform.ps1 `
  -BaseUrl http://127.0.0.2:8000 `
  -Scenario synthetic_success `
  -RequestCount 10 `
  -SubmitConcurrency 4 `
  -ConfigureCallbackDestination `
  -CallbackDeliveryUrl http://reverse-proxy:8000/healthz
```

## Scenario Map

| Scenario | Purpose | Notes |
|---|---|---|
| `synthetic_success` | Golden-path benchmark | Uses playground-scoped synthetic success |
| `retry_then_success` | Retry amplification benchmark | Exercises retry scheduling and recovery path |
| `terminal_failure` | Terminal failure timing benchmark | Confirms failure path remains queryable |
| `rpc_timeout` | Degraded provider benchmark | Forces provider failure classification path |

## Duplicate-Rate Benchmark

Use `-DuplicateGroupSize` to simulate repeated requests with the same idempotency key.

Example:

```powershell
pwsh -File scripts/benchmark_platform.ps1 `
  -Scenario synthetic_success `
  -RequestCount 20 `
  -SubmitConcurrency 8 `
  -DuplicateGroupSize 4
```

That means every group of 4 submissions shares one idempotency key.

## Callback Failure Benchmark

Point the callback destination at a known failing route or timeout target.

Example:

```powershell
pwsh -File scripts/benchmark_platform.ps1 `
  -Scenario synthetic_success `
  -RequestCount 10 `
  -SubmitConcurrency 4 `
  -ConfigureCallbackDestination `
  -CallbackDeliveryUrl http://reverse-proxy:8000/status/requests/does-not-exist
```

This should keep execution truth intact while callback delivery degrades.

## What The Output Means

| Metric | Meaning |
|---|---|
| `AcceptanceLatency*` | POST `/api/requests` response time |
| `QueueEnqueueLatency*` | `received` -> first `queued` |
| `WorkerPickupDelay*` | first `queued` -> first `leased` |
| `AdapterExecutionWindow*` | first `executing` -> final receipt entry |
| `ReceiptFinalizationWindow*` | last `executing` -> final receipt entry |
| `AcceptedToFinal*` | `received` -> final receipt entry |
| `FinalToCallback*` | final receipt entry -> callback delivered |
| `AcceptedToCallback*` | `received` -> callback delivered |
| `RetryAmplificationAverage` | average count of `retry_scheduled` entries per accepted request |

## Safe Operating Range

Use the benchmark to learn:

- p50 / p95 / p99 latency
- throughput under steady and burst traffic
- retry amplification under degraded provider behavior
- callback backlog growth under degraded callback targets
- first bottleneck before public launch

The benchmark is only credible if you run it alongside:

- `scripts/check_platform_health.ps1`
- cluster/DB metrics
- durable receipt inspection for outliers

## Production Benchmark Rule

Do not treat synthetic success as the only production benchmark.

Before launch, also benchmark:

1. real customer-signed success path
2. degraded RPC path
3. degraded callback path
4. duplicate/idempotency-heavy path

That is the minimum benchmark set for a trustworthy v1 claim.
