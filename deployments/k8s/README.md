# Kubernetes Deployment Pack

This folder contains baseline Kubernetes manifests for the full Azums platform stack.

## Included Resources

- Namespace: `azums`
- ConfigMap: shared non-secret runtime config
- Secret template: database + bearer tokens + optional tenant keys
- Postgres StatefulSet + Service
- Deployments/Services:
- `ingress-api`
- `status-api`
- `operator-ui-backend` (Rust backend API proxy)
- `operator-ui` (Next.js frontend)
- `reverse-proxy`
- Deployment:
- `execution-worker`
- Public ingress route: `azums-public`
- Optional monitoring overlay: `deployments/k8s/monitoring/*`
- Kustomize entrypoint: `kustomization.yaml`

## Prerequisites

1. Build and push service images from `deployments/docker`.
2. Replace placeholders in `secret.example.yaml`.
3. Install an ingress controller if you plan to use `ingress-public.yaml`.
4. Create a TLS secret for the public ingress host.

## Image Convention

Kubernetes manifests use this convention:

- `ghcr.io/blockforge-dev/azums/<service>:main`

Service image names:

- `ingress_api`
- `status_api`
- `execution_worker`
- `operator_ui`
- `operator_ui_next`
- `reverse_proxy`

If your registry owner differs, update these image references or apply a kustomize image override before deployment.

The publish workflow supports a repository variable:

- `IMAGE_NAMESPACE` (example: `my-org/azums`)

K8s deploy workflow also supports:

- `K8S_NAMESPACE` (default `azums`)

Required repository secrets for `.github/workflows/k8s-deploy.yml`:

- `KUBE_CONFIG_DATA` (base64-encoded kubeconfig)
- `DATABASE_URL`
- `POSTGRES_DB`
- `POSTGRES_USER`
- `POSTGRES_PASSWORD`
- `INGRESS_BEARER_TOKEN`
- `STATUS_API_BEARER_TOKEN`
- `OPERATOR_UI_STATUS_BEARER_TOKEN`

Optional secrets:

- `INGRESS_TENANT_TOKENS`
- `INGRESS_API_KEY`
- `INGRESS_TENANT_API_KEYS`
- `INGRESS_WEBHOOK_SIGNATURE_SECRETS`
- `STATUS_API_TENANT_TOKENS`
- `OPERATOR_UI_INGRESS_BEARER_TOKEN` (if unset, operator UI can reuse `INGRESS_BEARER_TOKEN`)
- `OPERATOR_UI_FLUTTERWAVE_SECRET_KEY` (required for server-side Flutterwave transaction verification)
- `OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH` (required to accept Flutterwave webhook callbacks)
- `OPERATOR_UI_SMTP_USERNAME` / `OPERATOR_UI_SMTP_PASSWORD` (required for email verification and password reset)
- `EXECUTION_CALLBACK_DELIVERY_TOKEN` (optional fallback callback bearer token)
- `EXECUTION_CALLBACK_SIGNING_SECRET` (optional fallback callback signing secret)
- `REVERSE_PROXY_METRICS_BEARER_TOKEN` (recommended if `/metrics` is exposed beyond the cluster)
- `SOLANA_PAYER_SECRET_BASE58` (optional; only needed for non-customer-signed/devnet-sponsored flows)

Optional config map values:

- `OPERATOR_UI_FLUTTERWAVE_BASE_URL` (default `https://api.flutterwave.com/v3`)
- `OPERATOR_UI_FLUTTERWAVE_EXPECTED_CURRENCY` (example `NGN`)
- `OPERATOR_UI_FLUTTERWAVE_FX_RATES_USD` (example `USD=1;NGN=0.00066;GBP=1.27;CAD=0.74;JPY=0.0067`)
- `OPERATOR_UI_STATUS_PRINCIPAL_MODE` (`workspace` recommended; fallback `service`)
- `OPERATOR_UI_INGRESS_FALLBACK_PRINCIPAL_ID` / `OPERATOR_UI_INGRESS_FALLBACK_SUBMITTER_KIND`
  (optional ingress retry fallback when custom ingress principal bindings are incomplete)
- `OPERATOR_UI_REQUIRE_DURABLE_METERING` (`true` recommended in production to fail closed on metering outages)
- `OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC` (`true` recommended so workspace network policy is enforced)
- `OPERATOR_UI_SANDBOX_SOLANA_RPC_URL` (default `https://api.devnet.solana.com` for Playground/Sandbox)
- `OPERATOR_UI_STAGING_SOLANA_RPC_URL` (optional staging override)
- `OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL` (optional production override; supports ordered comma-separated hybrid RPC list)
- `INGRESS_DEFAULT_EXECUTION_POLICY` (`customer_signed` recommended in production)
- `INGRESS_DEFAULT_SPONSORED_MONTHLY_CAP_REQUESTS` (cap for sponsored mode)
- `INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED` (`false` for rollout bootstrap, then `true`)
- `INGRESS_EXECUTION_POLICY_CANARY_TENANTS` (comma-separated tenant ids for canary enablement)
- `SOLANA_PLATFORM_SIGNING_ENABLED` (`false` recommended in production for customer-signed mode)
- `SOLANA_RPC_PRIMARY_URL` (managed/external RPC recommended as primary in production)
- `SOLANA_RPC_FALLBACK_URLS` (optional comma-separated self-hosted or secondary provider failover list)
- `SOLANA_RPC_URLS` (optional full ordered list if you want to set primary+fallback in one value)

Principal binding bootstrap defaults are wildcard-ready for new workspaces:

- `INGRESS_PRINCIPAL_TENANT_BINDINGS` includes `tenant_ws_*`
- `STATUS_API_PRINCIPAL_ROLE_BINDINGS` includes `workspace-*-{owner|admin|developer|viewer}`
- `STATUS_API_PRINCIPAL_TENANT_BINDINGS` includes `workspace-*=tenant_ws_*`

To add Flutterwave secrets later (after initial deploy):

```bash
kubectl -n azums patch secret azums-platform-secrets --type merge -p \
  "{\"stringData\":{\"OPERATOR_UI_FLUTTERWAVE_SECRET_KEY\":\"<replace>\",\"OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH\":\"<replace>\"}}"
kubectl -n azums rollout restart deployment/operator-ui-backend
```

To prepare a cluster for production in one pass after you have the real values:

```bash
pwsh -File scripts/apply_production_runtime.ps1 \
  -Namespace azums \
  -PublicHost app.example.com \
  -TlsCertPath /path/to/fullchain.pem \
  -TlsKeyPath /path/to/privkey.pem \
  -ProductionSolanaRpcPrimaryUrl https://managed-rpc.example.com \
  -ProductionSolanaRpcFallbackUrls https://self-hosted-rpc.example.com \
  -CallbackAllowedHosts callbacks.example.com \
  -FlutterwaveSecretKey FLWSECK_LIVE_xxx \
  -FlutterwaveWebhookHash your_webhook_hash \
  -SmtpHost smtp.example.com \
  -SmtpUsername smtp-user \
  -SmtpPassword smtp-password \
  -EmailFrom no-reply@example.com \
  -Apply
```

Then audit the result:

```bash
pwsh -File scripts/check_production_readiness.ps1 -Namespace azums -ExpectedHost app.example.com
```

## Apply

```bash
cd deployments/k8s
kubectl apply -k .
```

Create TLS secret before exposing public ingress:

```bash
kubectl -n azums create secret tls azums-public-tls \
  --cert=/path/to/fullchain.pem \
  --key=/path/to/privkey.pem
```

If you use `scripts/apply_production_runtime.ps1`, it creates or updates the TLS secret for you.

`ingress-public.yaml` enforces HTTPS redirect and terminates TLS for both:

- app routes (`/`, `/app`, `/ops`, `/api/ui/*`, including Flutterwave webhook path)
- platform API/status routes (`/api`, `/webhooks`, `/status`)

Before applying manually, create `azums-platform-secrets` in-cluster from your real secret values.

## Verify

```bash
kubectl -n azums get pods
kubectl -n azums get svc
kubectl -n azums get ingress
```

## Local Access (Without External Load Balancer)

```bash
kubectl -n azums port-forward svc/reverse-proxy 8000:8000
kubectl -n azums port-forward svc/operator-ui 8083:8083
```

Then:

- API/status via `http://127.0.0.1:8000`
- Operator UI frontend via `http://127.0.0.1:8083`
- (optional) backend API proxy via `kubectl -n azums port-forward svc/operator-ui-backend 18083:8083`

## Security Notes

- Treat `secret.example.yaml` as a template only; do not commit real values.
- Keep `DATABASE_URL` and bearer tokens in your secret manager.
- Restrict ingress host/path exposure and apply network policies in production.
- Rotate all platform tokens and DB password regularly with `scripts/rotate_platform_secrets.ps1`.
- Use `scripts/check_production_readiness.ps1` as the final pre-launch gate.
- If SMTP is not configured, keep:
  - `OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION=false`
  - `OPERATOR_UI_PASSWORD_RESET_ENABLED=false`
- Lock callback egress in production:
  - `EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS=false`
  - `EXECUTION_CALLBACK_ALLOWED_HOSTS=<approved callback domains>`

## Monitoring and Alerts (Optional Overlay)

If Prometheus Operator is installed:

```bash
kubectl apply -k deployments/k8s/monitoring
```

This adds crash-loop alerts for ingress/status/execution-worker and queue/callback alerts from postgres-exporter SQL metrics.

Without Prometheus Operator, run:

```bash
pwsh -File scripts/check_platform_health.ps1 -Namespace azums
```
