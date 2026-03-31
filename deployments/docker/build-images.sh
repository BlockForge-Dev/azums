#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

build_image() {
  local manifest="$1"
  local bin_name="$2"
  local tag="$3"
  local include_signer="${4:-false}"

  echo "Building ${tag} ..."
  docker build \
    -f deployments/docker/Dockerfile \
    --build-arg APP_MANIFEST="${manifest}" \
    --build-arg BIN_NAME="${bin_name}" \
    --build-arg INCLUDE_SOLANA_SIGNER="${include_signer}" \
    -t "${tag}" \
    .
}

build_image "apps/ingress_api/Cargo.toml" "ingress_api" "azums/ingress_api:local" "false"
build_image "crates/status_api/Cargo.toml" "status_api" "azums/status_api:local" "false"
build_image "apps/admin_cli/Cargo.toml" "execution_core_worker" "azums/execution_worker:local" "true"
build_image "apps/operator_ui/Cargo.toml" "operator_ui" "azums/operator_ui:local" "false"
build_image "crates/reverse-proxy/Cargo.toml" "reverse_proxy" "azums/reverse_proxy:local" "false"

echo "Building azums/operator_ui_next:local ..."
docker build \
  -f deployments/docker/Dockerfile.next \
  -t azums/operator_ui_next:local \
  .
