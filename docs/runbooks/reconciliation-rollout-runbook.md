# Reconciliation Rollout Runbook

## Stages

1. `hidden`
   - Customer confidence UI stays off.
   - Operator exception dashboard stays off.
   - Recon and exception workers run in the background only.
2. `operator_only`
   - Operator exception dashboard and rollout summary are visible.
   - Customer dashboards and request detail pages still suppress reconciliation-backed confidence.
3. `customer_visible`
   - Customer dashboards, request detail pages, and receipt pages show reconciliation-backed confidence.
   - Operator surfaces remain enabled.

`OPERATOR_UI_RECONCILIATION_ROLLOUT_MODE` controls the UI stage:

- `hidden`
- `operator_only`
- `customer_visible`

## Promotion Gates

Promote from `hidden` to `operator_only` only when:

- recon worker is stable across restarts
- backfill completed without duplicate-subject churn
- exception rate is explainable
- false positives are being reviewed

Promote from `operator_only` to `customer_visible` only when:

- false positive rate is acceptable for launch
- stale rate is understood
- sampled unified-query performance is acceptable
- operators can close the common exception workflows without manual DB work

## Commands

Backfill recent receipts safely:

```powershell
pwsh -File scripts/backfill_reconciliation_intake.ps1 `
  -Namespace azums `
  -LookbackHours 168 `
  -Apply
```

Generate rollout report:

```powershell
pwsh -File scripts/report_reconciliation_rollout.ps1 `
  -BaseUrl http://127.0.0.1:8082/status `
  -TenantId tenant_demo `
  -StatusToken dev-status-token
```

Benchmark dashboard paths:

```powershell
pwsh -File scripts/benchmark_reconciliation_rollout.ps1 `
  -BaseUrl http://127.0.0.1:8082/status `
  -TenantId tenant_demo `
  -StatusToken dev-status-token `
  -Iterations 10
```
