#!/usr/bin/env bash
set -euo pipefail

readonly CRATES=(
  azums-core
  azums-redis
  azums
  azums-postgres
  azums-axum
  azums-actix
  azums-poem
  azums-rocket
)

check_only=false
if [[ "${1:-}" == "--check" ]]; then
  check_only=true
  shift
fi

version="${1:-}"
if [[ -z "${version}" ]]; then
  echo "usage: $0 [--check] <version>" >&2
  exit 2
fi

for crate in "${CRATES[@]}"; do
  manifest="crates/${crate}/Cargo.toml"
  manifest_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' "${manifest}" | head -n 1)
  if [[ "${manifest_version}" != "${version}" ]]; then
    echo "${crate} has version ${manifest_version}; expected ${version}" >&2
    exit 1
  fi
done

if [[ "${check_only}" == "true" ]]; then
  echo "All publishable crates match version ${version}."
  exit 0
fi

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
  echo "CARGO_REGISTRY_TOKEN is required." >&2
  exit 1
fi

crate_is_published() {
  local crate="$1"
  local status
  status=$(curl \
    --silent \
    --show-error \
    --output /dev/null \
    --write-out '%{http_code}' \
    --retry 3 \
    --retry-delay 2 \
    --user-agent "azums-release/${version}" \
    "https://crates.io/api/v1/crates/${crate}/${version}")

  case "${status}" in
    200) return 0 ;;
    404) return 1 ;;
    *)
      echo "crates.io returned HTTP ${status} for ${crate} ${version}" >&2
      return 2
      ;;
  esac
}

for crate in "${CRATES[@]}"; do
  if crate_is_published "${crate}"; then
    echo "${crate} ${version} is already published; skipping."
    continue
  else
    status=$?
    if [[ ${status} -ne 1 ]]; then
      exit "${status}"
    fi
  fi

  cargo publish -p "${crate}" --locked

  visible=false
  for _ in {1..30}; do
    if crate_is_published "${crate}"; then
      visible=true
      break
    else
      status=$?
      if [[ ${status} -ne 1 ]]; then
        exit "${status}"
      fi
    fi
    sleep 10
  done

  if [[ "${visible}" != "true" ]]; then
    echo "${crate} ${version} was not visible on crates.io after 5 minutes." >&2
    exit 1
  fi
done

for crate in "${CRATES[@]}"; do
  if ! crate_is_published "${crate}"; then
    echo "Final verification failed for ${crate} ${version}." >&2
    exit 1
  fi
done

echo "Published and verified all Azums ${version} crates."
