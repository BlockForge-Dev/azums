# Reconciliation Launch Readiness Checklist

- [ ] Rollout summary endpoint returns stable data for the target tenant.
- [ ] Dirty subject backlog drains under normal traffic.
- [ ] Recon retries do not impact execution retry behavior.
- [ ] Backfill script has been dry-run and applied safely where needed.
- [ ] Exception rate is understood for the launch window.
- [ ] False positive rate is acceptable for launch.
- [ ] Stale rate is acceptable for launch.
- [ ] Operators can acknowledge, investigate, resolve, and mark false positives without DB access.
- [ ] Replay review remains core-authorized only.
- [ ] Sampled unified request query latency is acceptable.
- [ ] Exception index query latency is acceptable.
- [ ] Customer-facing confidence remains disabled until the operator-only review passes.
- [ ] `OPERATOR_UI_RECONCILIATION_ROLLOUT_MODE` is set intentionally for the launch stage.
