# Production Gap Closure Checklist

This document tracks the strict closure pass executed in this order:

1. Metering hardening
2. Billing durability
3. Role bootstrap automation
4. Docs and security cleanup
5. Postgres account/auth/billing + ingress quota + recovery flows
6. Production security and operations closure

## 1) Metering hardening

- [x] Durable usage metering now deduplicates by accepted request identity instead of intent-only keys.
- [x] Added deterministic fallback key order for metering:
  - `request_id`
  - `accepted_job_id`
  - `accepted_intent_id`
  - `audit_id`
- [x] Added strict fail-closed mode:
  - `OPERATOR_UI_REQUIRE_DURABLE_METERING=true`
  - If enabled, quota and usage endpoints return `503` when durable metering is unavailable.
- [x] Existing non-strict mode preserved for local/dev bootstrap.

## 2) Billing durability

- [x] Hardened account-store persistence to atomic temp-file writes with explicit flush/sync.
- [x] Added backup recovery path:
  - primary file: `<store>.json`
  - backup file: `<store>.json.bak`
- [x] Startup store normalization is now run before default-account bootstrap.
- [x] Existing payment dedup + invoice append behavior retained.

## 3) Role bootstrap automation

- [x] Startup store normalization now backfills missing workspace tenant IDs.
- [x] Added ingress fallback principal retry support for binding mismatch errors:
  - `OPERATOR_UI_INGRESS_FALLBACK_PRINCIPAL_ID`
  - `OPERATOR_UI_INGRESS_FALLBACK_SUBMITTER_KIND`
- [x] Added compose/k8s env wiring for fallback principal settings.
- [x] Compose defaults updated to wildcard-ready workspace tenant/role bindings.

## 4) Docs and security cleanup

- [x] Updated Next frontend README to remove stale localStorage-prototype wording.
- [x] Updated `operator_ui` README with:
  - strict durable metering env flag
  - ingress fallback principal env flags
  - durability notes
- [x] Updated deployment docs (Docker + K8s) to document strict metering and ingress fallback controls.
- [x] Updated deployment env/config defaults to include the new flags.

## Operational recommendation

For production:

- Set `OPERATOR_UI_REQUIRE_DURABLE_METERING=true`.
- Keep wildcard workspace bindings enabled for status/ingress tenant maps.
- Keep `OPERATOR_UI_STATUS_PRINCIPAL_MODE=workspace`.
- Use `OPERATOR_UI_INGRESS_FALLBACK_PRINCIPAL_ID=ingress-service` only as a safety net, not as a replacement for proper bindings.

## Profile enforcement status

- [x] `crates/config/profiles/production.env.template` sets `OPERATOR_UI_REQUIRE_DURABLE_METERING=true`.
- [x] Dev/local templates keep `OPERATOR_UI_REQUIRE_DURABLE_METERING=false`.

## 5) Postgres account/auth/billing + ingress quota + recovery flows

- [x] `operator_ui` now supports Postgres-backed account/auth/billing persistence:
  - `OPERATOR_UI_ACCOUNT_STORE_BACKEND=postgres`
  - `OPERATOR_UI_ACCOUNT_STORE_KEY` (row key)
  - fallback `file` backend retained for local/test.
- [x] Ingress now enforces tenant quota at ingress-level on both channels:
  - `/api/requests`
  - `/webhooks/:source`
- [x] Added ingress internal quota profile endpoint:
  - `PUT /api/internal/tenants/:tenant_id/quota`
- [x] `operator_ui` billing/signup/invite flows now sync tenant quota profiles to ingress.
- [x] Added real email verification and password reset backend flows:
  - `POST /api/ui/account/email-verification/request`
  - `POST /api/ui/account/email-verification/confirm`
  - `POST /api/ui/account/password-reset/request`
  - `POST /api/ui/account/password-reset/confirm`
- [x] Added SMTP-backed delivery configuration for auth emails:
  - `OPERATOR_UI_SMTP_HOST`
  - `OPERATOR_UI_SMTP_PORT`
  - `OPERATOR_UI_SMTP_USERNAME`
  - `OPERATOR_UI_SMTP_PASSWORD`
  - `OPERATOR_UI_EMAIL_FROM`
  - `OPERATOR_UI_PUBLIC_BASE_URL`
  - `OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION`

## 6) Production security and operations closure

- [x] Added coordinated secret + DB password rotation automation:
  - `scripts/rotate_platform_secrets.ps1`
- [x] Added DB backup + restore drill script:
  - `scripts/db_backup_restore_drill.ps1`
- [x] Added fallback runtime health checker for crash-loop + queue/callback thresholds:
  - `scripts/check_platform_health.ps1`
- [x] Enforced HTTPS ingress pattern in Kubernetes manifest:
  - TLS secret required (`azums-public-tls`)
  - force SSL redirect
  - webhook/API routes flow through TLS ingress
- [x] Added optional monitoring overlay for Prometheus Operator:
  - postgres-exporter custom queue/callback metrics
  - ServiceMonitors
  - PrometheusRule alerts
- [x] Added explicit production callback egress lock defaults:
  - `EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS=false`
  - `EXECUTION_CALLBACK_ALLOWED_HOSTS=callbacks.example.com`
- [x] Added explicit account recovery mode control:
  - `OPERATOR_UI_PASSWORD_RESET_ENABLED` (auto-disabled when SMTP is not configured)
  - frontend now labels forgot-password unsupported when disabled

## 7) Production activation runbook

- [x] Added one-pass production runtime apply automation:
  - `scripts/apply_production_runtime.ps1`
  - configures TLS ingress secret
  - patches public host/base URL
  - wires SMTP + recovery flags
  - wires Flutterwave config
  - enforces production Solana RPC + customer-signed posture
  - supports hybrid Solana RPC ordering with managed/external primary and self-hosted fallback
  - restarts workloads and waits for rollout
- [x] Added production readiness audit:
  - `scripts/check_production_readiness.ps1`
  - validates pods, images, TLS, public host, SMTP/recovery, Flutterwave, callback policy, and mainnet RPC posture
  - warns when hybrid RPC failover is not configured

## Launch sequence

1. Push immutable release images and update deployments away from `:local`.
2. Rotate or patch real secrets with `scripts/rotate_platform_secrets.ps1` if needed.
3. Apply runtime production settings with `scripts/apply_production_runtime.ps1`.
4. Run `scripts/check_production_readiness.ps1`.
5. Run `scripts/db_backup_restore_drill.ps1`.
6. Run `scripts/verify_billing_endpoints.ps1` with a real Flutterwave transaction id.
