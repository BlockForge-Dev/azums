# Docker Deployment Pack

This folder contains image-based deployment assets for Azums services.

## Files

- `Dockerfile`
- Generic multi-stage builder/runtime image definition.
- `Dockerfile.next`
- Next.js frontend image definition for `operator_ui_next`.
- `build-images.ps1`
- Builds all service images on Windows PowerShell.
- `build-images.sh`
- Builds all service images on Linux/macOS.
- `docker-compose.images.yml`
- Runs the platform from prebuilt images instead of source-mounted `cargo run`.
- `.env.example`
- Runtime environment defaults for the image-based compose stack.

## Build Images

From repository root:

```powershell
powershell -ExecutionPolicy Bypass -File deployments/docker/build-images.ps1
```

```bash
chmod +x deployments/docker/build-images.sh
./deployments/docker/build-images.sh
```

## Run Image-Based Compose Stack

```bash
cd deployments/docker
cp .env.example .env
docker compose -f docker-compose.images.yml up
```

Public entrypoint:

- `http://localhost:8000` (reverse proxy)

Operator UI:

- `http://localhost:8083`
- Served by `operator_ui_next` (Next.js frontend) and proxied to internal `operator_ui` backend.
- Customer submit flow in UI uses `operator_ui` -> `reverse_proxy` (`OPERATOR_UI_INGRESS_BASE_URL`) with ingress auth headers.
- Default env bootstrap is wildcard-ready for new workspace tenants (`tenant_ws_*`) and workspace principals (`workspace-*`).
- Optional Flutterwave billing verification is handled by `operator_ui` backend:
  - `OPERATOR_UI_FLUTTERWAVE_SECRET_KEY`
  - `OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH`
  - `OPERATOR_UI_FLUTTERWAVE_BASE_URL`
  - `OPERATOR_UI_FLUTTERWAVE_EXPECTED_CURRENCY`
  - `OPERATOR_UI_FLUTTERWAVE_FX_RATES_USD` (multi-currency to USD conversion map for billing verification)
  - `OPERATOR_UI_STATUS_PRINCIPAL_MODE` (`workspace` default with automatic service-principal fallback)
  - `OPERATOR_UI_INGRESS_FALLBACK_PRINCIPAL_ID` / `OPERATOR_UI_INGRESS_FALLBACK_SUBMITTER_KIND`
    (optional ingress retry fallback for principal/tenant binding mismatches)
  - `OPERATOR_UI_REQUIRE_DURABLE_METERING` (`false` by default in local compose; set `true` for strict fail-closed quota checks)
  - `OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC` (`true` by default; enforce workspace-specific Solana RPC routing)
  - `OPERATOR_UI_SANDBOX_SOLANA_RPC_URL` (`https://api.devnet.solana.com` by default)
  - `OPERATOR_UI_STAGING_SOLANA_RPC_URL` (optional)
  - `OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL` (optional)
  - `OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION` (`false` recommended until SMTP is configured and tested)
  - `OPERATOR_UI_PASSWORD_RESET_ENABLED` (`false` recommended if SMTP is not configured)
  - `NEXT_PUBLIC_PASSWORD_RESET_ENABLED` (frontend forgot-password label toggle)
  - webhook route: `POST /api/ui/billing/flutterwave/webhook`

Add Flutterwave secrets later by updating `.env` and restarting backend only:

```bash
docker compose -f docker-compose.images.yml up -d --force-recreate operator_ui_backend
```

Production-safe operational scripts live under `scripts/`:

- `rotate_platform_secrets.ps1` (token + DB password rotation with rollout)
- `verify_billing_endpoints.ps1` (login + billing provider/profile + verify/webhook checks)
- `db_backup_restore_drill.ps1` (backup and restore validation)
- `check_platform_health.ps1` (pod/queue/callback threshold health checks)

## Build a Single Service Image

```bash
docker build \
  -f deployments/docker/Dockerfile \
  --build-arg APP_MANIFEST=apps/ingress_api/Cargo.toml \
  --build-arg BIN_NAME=ingress_api \
  --build-arg INCLUDE_SOLANA_SIGNER=false \
  -t azums/ingress_api:local \
  .
```

For Next.js Operator UI frontend:

```bash
docker build \
  -f deployments/docker/Dockerfile.next \
  -t azums/operator_ui_next:local \
  .
```

Manifest/binary pairs:

- `apps/ingress_api/Cargo.toml` -> `ingress_api`
- `crates/status_api/Cargo.toml` -> `status_api`
- `apps/admin_cli/Cargo.toml` -> `execution_core_worker` (set `INCLUDE_SOLANA_SIGNER=true`)
- `apps/operator_ui/Cargo.toml` -> `operator_ui` (backend API proxy)
- `apps/operator_ui_next` -> `operator_ui_next` (Next.js frontend via `Dockerfile.next`)
- `crates/reverse-proxy/Cargo.toml` -> `reverse_proxy`

## CI Publishing

Workflows:

- `.github/workflows/docker-build.yml`
- Builds all service images for pull requests and main/master pushes (no push).
- `.github/workflows/docker-publish.yml`
- Builds and pushes images to GHCR on main/master and version tags.
- `.github/workflows/k8s-deploy.yml`
- Deploys Kubernetes manifests using the published `sha-<full_commit_sha>` image tag.

Published image convention:

- `ghcr.io/blockforge-dev/azums/<service>:<tag>`

Override namespace by setting repository variable `IMAGE_NAMESPACE`
(for example `my-org/azums`).

Tags include:

- `main` and `latest` on default branch
- `sha-<commit>`
- `v*` git tag names for release tags
