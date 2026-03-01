#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

build_image() {
  local manifest="$1"
  local bin_name="$2"
  local tag="$3"

  echo "Building ${tag} ..."
  docker build \
    -f deployments/docker/Dockerfile \
    --build-arg APP_MANIFEST="${manifest}" \
    --build-arg BIN_NAME="${bin_name}" \
    -t "${tag}" \
    .
}

build_image "apps/ingress_api/Cargo.toml" "ingress_api" "azums/ingress_api:local"
build_image "crates/status_api/Cargo.toml" "status_api" "azums/status_api:local"
build_image "apps/admin_cli/Cargo.toml" "admin_cli" "azums/execution_worker:local"
build_image "apps/operator_ui/Cargo.toml" "operator_ui" "azums/operator_ui:local"
build_image "crates/reverse-proxy/Cargo.toml" "reverse_proxy" "azums/reverse_proxy:local"
