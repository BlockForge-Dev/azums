# Docker Deployment Pack

This folder contains image-based deployment assets for Azums services.

## Files

- `Dockerfile`
- Generic multi-stage builder/runtime image definition.
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

## Build a Single Service Image

```bash
docker build \
  -f deployments/docker/Dockerfile \
  --build-arg APP_MANIFEST=apps/ingress_api/Cargo.toml \
  --build-arg BIN_NAME=ingress_api \
  -t azums/ingress_api:local \
  .
```

Manifest/binary pairs:

- `apps/ingress_api/Cargo.toml` -> `ingress_api`
- `crates/status_api/Cargo.toml` -> `status_api`
- `apps/admin_cli/Cargo.toml` -> `admin_cli`
- `apps/operator_ui/Cargo.toml` -> `operator_ui`
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
