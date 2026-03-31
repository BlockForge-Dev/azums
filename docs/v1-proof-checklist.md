# Azums v1 Proof Checklist

This is the gate before calling Azums truly production-ready.

## 1. Core Contract Proof

### Acceptance

- [ ] A valid request is authenticated, authorized, normalized, and durably recorded.
- [ ] A unique request ID is returned immediately after durable acceptance.
- [ ] Duplicate/idempotent requests do not create unsafe duplicate execution.
- [ ] Invalid requests fail early with explicit reason.

### Lifecycle Truth

- [ ] Every request has an explicit lifecycle state.
- [ ] Every state transition is durably recorded.
- [ ] No state exists only in memory.
- [ ] Terminal states are unambiguous.
- [ ] In-progress states can be queried at any time.

### Receipt Truth

- [ ] Every request produces a queryable receipt.
- [ ] The receipt shows the full execution journey, not just final status.
- [ ] Failures are classified by stage.
- [ ] Retry history is visible.
- [ ] Replay lineage is visible where applicable.
- [ ] Callback history is visible.

Pass condition:

An operator can inspect one request and understand exactly what happened without checking raw logs.

## 2. Golden Path Proof

Recommended v1 workflow:

`API/Webhook -> durable acceptance -> Solana adapter execution -> receipt finalized -> callback delivered`

Must prove:

- [ ] Request enters through ingress.
- [ ] Billing/entitlement gate allows it correctly.
- [ ] Intent is normalized into core shape.
- [ ] Durable record is created before execution starts.
- [ ] Execution core routes to Solana adapter correctly.
- [ ] Adapter result is mapped into normalized outcome.
- [ ] Receipt is committed before callback is emitted.
- [ ] Status API returns the exact final truth.

Pass condition:

You can demo this live end to end, repeatedly, with the same clear result.

## 3. Failure-Path Proof

### Crash and Restart Tests

- [ ] Kill worker before execution starts.
- [ ] Kill worker during execution.
- [ ] Kill worker after adapter response but before receipt finalization.
- [ ] Restart Postgres during active processing.
- [ ] Restart API service during intake burst.

Expected result:

- [ ] No silent loss.
- [ ] State remains queryable.
- [ ] Safe recovery path is visible.
- [ ] Reprocessing does not corrupt truth.

### External Dependency Tests

- [ ] Solana RPC timeout.
- [ ] Solana RPC partial/ambiguous response.
- [ ] Callback endpoint returns 500.
- [ ] Callback endpoint times out.
- [ ] Delivery retries exhaust.
- [ ] Adapter returns structured permanent failure.
- [ ] Adapter returns retryable failure.

Expected result:

- [ ] Failure is classified clearly.
- [ ] Retry policy behaves correctly.
- [ ] Receipt shows what is known vs uncertain.
- [ ] No fake success state appears.

### Duplicate and Replay Tests

- [ ] Same idempotency key sent twice.
- [ ] Same webhook delivered multiple times.
- [ ] Manual replay of safe request.
- [ ] Replay attempt of unsafe/non-replayable request.

Expected result:

- [ ] Duplicates do not create duplicate side effects.
- [ ] Replay policy is explicit.
- [ ] Unsafe replay is blocked with reason.

Pass condition:

You can break the system on purpose and still explain every outcome cleanly.

## 4. Retry and Idempotency Proof

- [ ] Retryable failures are distinguishable from terminal failures.
- [ ] Backoff rules are deterministic.
- [ ] Max attempts are enforced.
- [ ] DLQ/dead-letter behavior is explicit.
- [ ] External effects are protected by idempotency or reconciliation checks.
- [ ] Callback retries do not mutate execution truth.
- [ ] Retrying delivery does not re-run execution.

Questions Azums must answer clearly:

- [ ] Is it safe to retry?
- [ ] What already happened?
- [ ] What is uncertain?
- [ ] What was the last known external reference?
- [ ] Did a retry create a duplicate effect?

Pass condition:

An operator never has to guess whether retry is safe.

## 5. Billing Boundary Proof

- [ ] Billing can deny unauthorized or over-limit requests before acceptance.
- [ ] Billing does not own execution truth.
- [ ] Billing failure does not corrupt lifecycle truth.
- [ ] Usage records are derived from durable platform events.
- [ ] Entitlements affect access, not receipt correctness.
- [ ] Callback history and execution state remain valid even if billing subsystem is degraded.

Pass condition:

You can disable or degrade billing and execution truth still stays coherent.

## 6. Operator Proof

Operator must be able to:

- [ ] Search by request ID.
- [ ] View current lifecycle state.
- [ ] View full receipt timeline.
- [ ] View attempt history.
- [ ] View callback history.
- [ ] See failure classification.
- [ ] See whether replay is allowed.
- [ ] Trigger authorized replay/retry where policy allows.
- [ ] Distinguish execution failure from delivery failure.

Pass condition:

A new operator can handle a failed request with the UI/API and receipt data alone.

## 7. Read Model / Query Proof

- [ ] Status API reflects committed durable state only.
- [ ] No UI/API view invents state from transient memory.
- [ ] Timeline ordering is sensible and stable.
- [ ] Partial/in-progress visibility works.
- [ ] Query performance stays usable under load.
- [ ] Old requests remain queryable after completion.

Pass condition:

Querying a request during and after execution always returns a coherent view.

## 8. Benchmark Proof

### Measure These First

- [ ] Request acceptance latency
- [ ] Queue enqueue latency
- [ ] Worker pickup delay
- [ ] Adapter execution latency
- [ ] Receipt finalization latency
- [ ] Callback dispatch latency
- [ ] End-to-end time: accepted -> final state
- [ ] End-to-end time: accepted -> callback delivered

### Under Load, Observe

- [ ] throughput
- [ ] p50 / p95 / p99 latency
- [ ] queue growth
- [ ] retry amplification
- [ ] DB CPU / locks / connection usage
- [ ] worker saturation
- [ ] callback backlog growth
- [ ] memory stability

### Test Scenarios

- [ ] steady traffic
- [ ] burst traffic
- [ ] degraded Solana RPC
- [ ] degraded callback target
- [ ] high duplicate request rate

Pass condition:

You know Azums’ safe operating range and first bottleneck.

## 9. Observability Proof

- [ ] request IDs everywhere
- [ ] correlation IDs across layers
- [ ] transition logs tied to durable records
- [ ] attempt metrics
- [ ] retry metrics
- [ ] callback delivery metrics
- [ ] adapter latency metrics
- [ ] queue depth metrics
- [ ] DLQ count
- [ ] replay count
- [ ] billing gate failures separated from execution failures

Pass condition:

A production issue can be explained from metrics + receipt + structured logs without guesswork.

## 10. Security and Control Proof

- [ ] authentication works for all intake paths
- [ ] tenant isolation is enforced
- [ ] replay/retry/operator actions are permissioned
- [ ] secrets are not exposed in receipts
- [ ] callback signing/verification is correct if supported
- [ ] rate limiting protects ingress
- [ ] request validation prevents malformed execution

Pass condition:

Operator power cannot bypass tenant safety or mutate truth incorrectly.

## 11. Deployment Proof

- [ ] clean startup and shutdown
- [ ] worker graceful drain
- [ ] schema migrations are safe
- [ ] env configuration is deterministic
- [ ] health checks reflect useful readiness
- [ ] one-node deploy works cleanly
- [ ] restore/restart path is documented
- [ ] local/dev/staging/prod differences are explicit

Pass condition:

Another engineer can deploy and operate Azums without tribal knowledge.

## 12. Product Proof

You should be able to say clearly:

- [ ] what Azums does
- [ ] who it is for
- [ ] what pain it removes
- [ ] why it is better than workers + logs + webhooks
- [ ] what exactly the first workflow is
- [ ] what customers can trust it for
- [ ] what it does not do yet

Suggested v1 product sentence:

`Azums turns critical Solana execution requests into durable, replay-safe, queryable receipts with no silent failure.`

Pass condition:

A serious engineer or operator immediately understands the value.

## Launch Gate

Do not call Azums v1 ready until these 6 are true:

- [ ] One golden workflow works end to end every time.
- [ ] Crash and retry behavior are proven.
- [ ] Receipt truth is complete and understandable.
- [ ] Operator can diagnose and act without raw log digging.
- [ ] Billing is clearly separated from execution truth.
- [ ] Safe operating limits are benchmarked and known.

## Suggested Scoring Sheet

Score each category from `0` to `2`.

- `0` = not ready
- `1` = partially proven
- `2` = fully proven

| Category | Score (0-2) | Evidence | Notes |
|---|---:|---|---|
| Core contract |  |  |  |
| Golden workflow |  |  |  |
| Failure handling |  |  |  |
| Retry / idempotency |  |  |  |
| Billing boundary |  |  |  |
| Operator flows |  |  |  |
| Query / read model |  |  |  |
| Benchmarks |  |  |  |
| Observability |  |  |  |
| Security |  |  |  |
| Deployment |  |  |  |
| Product clarity |  |  |  |

## Readiness Bands

| Score | Meaning |
|---|---|
| `0-10` | Architecture exists, product not proven |
| `11-18` | Strong internal alpha |
| `19-22` | Serious beta candidate |
| `23-24` | Credible v1 launch candidate |

## Existing Repo Automation You Can Use

| Proof Area | Existing Script / Doc |
|---|---|
| Golden workflow and lifecycle A/B/C/D | `scripts/run_full_flow.ps1` |
| Quota / entitlement gating | `scripts/prove_quota_submit_enforcement.ps1` |
| API key create-submit-revoke proof | `scripts/prove_api_key_create_submit_revoke.ps1` |
| Billing verification path | `scripts/verify_billing_endpoints.ps1` |
| Production readiness gate | `scripts/check_production_readiness.ps1` |
| Backup / restore drill | `scripts/db_backup_restore_drill.ps1` |
| Runtime health / queue / callback thresholds | `scripts/check_platform_health.ps1` |
| Benchmark harness | `scripts/benchmark_platform.ps1` |
| Benchmark instructions | `docs/runbooks/benchmark-runbook.md` |

## Best Next Move

Run this in order:

1. prove the golden workflow
2. run failure injection
3. benchmark the bottlenecks
4. clean up operator receipt visibility
5. write the v1 launch narrative
