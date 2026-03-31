# Reconciliation Backfill Runbook

## Purpose

Materialize deterministic recon intake signals from recent durable execution receipts without rewriting execution truth.

## Safety Rules

- Backfill only uses `execution_core_receipts` that are already marked `reconciliation_eligible=true`.
- Signal IDs are deterministic: `backfill:<receipt_id>:<signal_kind>`.
- Re-running the script is safe because inserts use `ON CONFLICT DO NOTHING`.
- Backfill creates downstream intake only. It does not mutate execution state, recon outcomes, or exception state directly.

## Procedure

1. Run dry-run first.

```powershell
pwsh -File scripts/backfill_reconciliation_intake.ps1 `
  -Namespace azums `
  -LookbackHours 168
```

2. Verify candidate counts by signal kind.
3. Apply the insert.

```powershell
pwsh -File scripts/backfill_reconciliation_intake.ps1 `
  -Namespace azums `
  -LookbackHours 168 `
  -Apply
```

4. Confirm rollout summary improves and dirty subject count begins draining.

## Expected Signal Mapping

- `succeeded` receipts -> `finalized`
- `failed_terminal`, `dead_lettered`, `rejected` receipts -> `terminal_failure`
- receipts with durable external observation references -> `submitted_with_reference`
- all other eligible receipts -> `adapter_completed`
