#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BASE_URL="${BASE_URL:-http://localhost:8000}"
TENANT_ID="${TENANT_ID:-tenant_demo}"
INGRESS_TOKEN="${INGRESS_TOKEN:-dev-ingress-token}"
STATUS_TOKEN="${STATUS_TOKEN:-dev-status-token}"
INGRESS_PRINCIPAL_ID="${INGRESS_PRINCIPAL_ID:-ingress-service}"
STATUS_PRINCIPAL_ID="${STATUS_PRINCIPAL_ID:-demo-operator}"
STATUS_PRINCIPAL_ROLE="${STATUS_PRINCIPAL_ROLE:-admin}"
APPLY_CALLBACK_DESTINATION="${APPLY_CALLBACK_DESTINATION:-false}"

SUBMIT_PAYLOAD="${SUBMIT_PAYLOAD:-$SCRIPT_DIR/submit-request.json}"
REPLAY_PAYLOAD="${REPLAY_PAYLOAD:-$SCRIPT_DIR/replay-request.json}"
CALLBACK_PAYLOAD="${CALLBACK_PAYLOAD:-$SCRIPT_DIR/callback-destination.json}"

if [[ "${APPLY_CALLBACK_DESTINATION}" == "true" ]]; then
  echo "Configuring callback destination..."
  curl -sS -X POST "${BASE_URL}/status/tenant/callback-destination" \
    -H "authorization: Bearer ${STATUS_TOKEN}" \
    -H "x-tenant-id: ${TENANT_ID}" \
    -H "x-principal-id: ${STATUS_PRINCIPAL_ID}" \
    -H "x-principal-role: ${STATUS_PRINCIPAL_ROLE}" \
    -H "content-type: application/json" \
    -d @"${CALLBACK_PAYLOAD}"
  echo
fi

echo "Submitting Solana intent..."
SUBMIT_RESPONSE="$(curl -sS -X POST "${BASE_URL}/api/requests" \
  -H "authorization: Bearer ${INGRESS_TOKEN}" \
  -H "x-tenant-id: ${TENANT_ID}" \
  -H "x-principal-id: ${INGRESS_PRINCIPAL_ID}" \
  -H "x-submitter-kind: internal_service" \
  -H "content-type: application/json" \
  -d @"${SUBMIT_PAYLOAD}")"
echo "${SUBMIT_RESPONSE}"

INTENT_ID=""
if command -v jq >/dev/null 2>&1; then
  INTENT_ID="$(echo "${SUBMIT_RESPONSE}" | jq -r '.intent_id // empty')"
fi
if [[ -z "${INTENT_ID}" ]]; then
  INTENT_ID="$(echo "${SUBMIT_RESPONSE}" | sed -n 's/.*"intent_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi
if [[ -z "${INTENT_ID}" ]]; then
  echo "Could not extract intent_id from submit response." >&2
  exit 1
fi

echo "Intent ID: ${INTENT_ID}"
echo

echo "Request status:"
curl -sS "${BASE_URL}/status/requests/${INTENT_ID}" \
  -H "authorization: Bearer ${STATUS_TOKEN}" \
  -H "x-tenant-id: ${TENANT_ID}" \
  -H "x-principal-id: ${STATUS_PRINCIPAL_ID}" \
  -H "x-principal-role: ${STATUS_PRINCIPAL_ROLE}"
echo
echo

echo "Receipt:"
curl -sS "${BASE_URL}/status/requests/${INTENT_ID}/receipt" \
  -H "authorization: Bearer ${STATUS_TOKEN}" \
  -H "x-tenant-id: ${TENANT_ID}" \
  -H "x-principal-id: ${STATUS_PRINCIPAL_ID}" \
  -H "x-principal-role: ${STATUS_PRINCIPAL_ROLE}"
echo
echo

echo "History:"
curl -sS "${BASE_URL}/status/requests/${INTENT_ID}/history" \
  -H "authorization: Bearer ${STATUS_TOKEN}" \
  -H "x-tenant-id: ${TENANT_ID}" \
  -H "x-principal-id: ${STATUS_PRINCIPAL_ID}" \
  -H "x-principal-role: ${STATUS_PRINCIPAL_ROLE}"
echo
echo

echo "Replay:"
curl -sS -X POST "${BASE_URL}/status/requests/${INTENT_ID}/replay" \
  -H "authorization: Bearer ${STATUS_TOKEN}" \
  -H "x-tenant-id: ${TENANT_ID}" \
  -H "x-principal-id: ${STATUS_PRINCIPAL_ID}" \
  -H "x-principal-role: ${STATUS_PRINCIPAL_ROLE}" \
  -H "content-type: application/json" \
  -d @"${REPLAY_PAYLOAD}"
echo
