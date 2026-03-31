# Public Edge Verification Runbook

This runbook is the repeatable path for deploying a known-good image set to Kubernetes and verifying the public production surfaces in this order:

1. readiness/runtime posture
2. public HTTPS
3. external callback receiver
4. billing provider
5. email flows
6. UI smoke flow

## Script Entry Point

Use:

- `scripts/verify_public_edge_surfaces.ps1`

This orchestrates:

- `scripts/redeploy_latest_images.ps1`
- `scripts/check_production_readiness.ps1`
- `scripts/run_full_flow.ps1`
- `scripts/verify_billing_endpoints.ps1`
- `scripts/verify_ui_smoke.ps1`

## Deploy Known-Good Images To K8s

```powershell
pwsh -File scripts/verify_public_edge_surfaces.ps1 `
  -Namespace azums `
  -ImageNamespace ghcr.io/blockforge-dev/azums `
  -Tag freeze-20260313-verified `
  -DeployKnownGoodImages `
  -ExpectedHost app.example.com `
  -PublicBaseUrl https://app.example.com `
  -ExternalCallbackUrl https://callbacks.example.com/azums `
  -OperatorUiBaseUrl https://app.example.com `
  -Email admin@example.com `
  -Password YOUR_PASSWORD `
  -FlutterwaveTransactionId REAL_TX_ID `
  -AttemptBillingWebhook `
  -FlutterwaveWebhookHash YOUR_HASH `
  -SignupEmail smoke+verify@example.com `
  -SignupPassword YOUR_SIGNUP_PASSWORD `
  -ExercisePasswordReset
```

## What Is Automated

- deploy/update current image tag to k8s
- rollout wait
- production posture check
- public HTTPS health checks
- callback end-to-end flow against a real public callback receiver
- billing verification path
- HTTP-level UI smoke checks
- login/session/operator/billing API smoke after login
- signup request and password reset request submission

## What Is Still Manual

Two production surfaces still require a human or inbox integration:

- verification email delivery and link redemption
- password reset email delivery and link redemption

HTTP acceptance of signup/reset is not enough. You still need to confirm:

- email actually arrives
- links use the public host
- verify-email link succeeds
- reset-password link succeeds
- tokens cannot be reused

## Suggested Verification Order

1. `check_production_readiness.ps1`
2. public HTTPS checks
3. `run_full_flow.ps1` with real callback receiver
4. `verify_billing_endpoints.ps1`
5. `verify_ui_smoke.ps1`
6. inbox-driven email verification/reset proof
