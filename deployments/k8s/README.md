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
- `operator-ui`
- `reverse-proxy`
- Deployment:
- `execution-worker`
- Public ingress route: `azums-public`
- Kustomize entrypoint: `kustomization.yaml`

## Prerequisites

1. Build and push service images from `deployments/docker`.
2. Replace placeholders in `secret.example.yaml`.
3. Install an ingress controller if you plan to use `ingress-public.yaml`.

## Image Convention

Kubernetes manifests use this convention:

- `ghcr.io/blockforge-dev/azums/<service>:main`

Service image names:

- `ingress_api`
- `status_api`
- `execution_worker`
- `operator_ui`
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

## Apply

```bash
cd deployments/k8s
kubectl apply -k .
```

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
- Operator UI via `http://127.0.0.1:8083`

## Security Notes

- Treat `secret.example.yaml` as a template only; do not commit real values.
- Keep `DATABASE_URL` and bearer tokens in your secret manager.
- Restrict ingress host/path exposure and apply network policies in production.
