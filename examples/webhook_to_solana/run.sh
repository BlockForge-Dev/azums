#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BASE_URL="${BASE_URL:-http://localhost:8000}"
TENANT_ID="${TENANT_ID:-tenant_demo}"
INGRESS_TOKEN="${INGRESS_TOKEN:-dev-ingress-token}"
STATUS_TOKEN="${STATUS_TOKEN:-dev-status-token}"
WEBHOOK_SOURCE="${WEBHOOK_SOURCE:-demo_partner}"
WEBHOOK_INTENT_KIND="${WEBHOOK_INTENT_KIND:-solana.transfer.v1}"
INGRESS_PRINCIPAL_ID="${INGRESS_PRINCIPAL_ID:-ingress-service}"
WEBHOOK_SUBMITTER_KIND="${WEBHOOK_SUBMITTER_KIND:-internal_service}"
STATUS_PRINCIPAL_ID="${STATUS_PRINCIPAL_ID:-demo-operator}"
STATUS_PRINCIPAL_ROLE="${STATUS_PRINCIPAL_ROLE:-admin}"
WEBHOOK_PAYLOAD="${WEBHOOK_PAYLOAD:-$SCRIPT_DIR/webhook-payload.json}"
WEBHOOK_ID="${WEBHOOK_ID:-webhook-example-$(date +%s)}"
WEBHOOK_SECRET="${WEBHOOK_SECRET:-}"
WEBHOOK_SIGNATURE="${WEBHOOK_SIGNATURE:-}"

if [[ -z "${WEBHOOK_SIGNATURE}" && -n "${WEBHOOK_SECRET}" ]]; then
  if ! command -v openssl >/dev/null 2>&1; then
    echo "WEBHOOK_SECRET is set but openssl was not found. Install openssl or set WEBHOOK_SIGNATURE manually." >&2
    exit 1
  fi
  DIGEST="$(openssl dgst -sha256 -hmac "${WEBHOOK_SECRET}" "${WEBHOOK_PAYLOAD}" | awk '{print $NF}')"
  WEBHOOK_SIGNATURE="v1=${DIGEST}"
fi

echo "Submitting webhook intent..."
if [[ -n "${WEBHOOK_SIGNATURE}" ]]; then
  SUBMIT_RESPONSE="$(curl -sS -X POST "${BASE_URL}/webhooks/${WEBHOOK_SOURCE}" \
    -H "authorization: Bearer ${INGRESS_TOKEN}" \
    -H "x-tenant-id: ${TENANT_ID}" \
    -H "x-principal-id: ${INGRESS_PRINCIPAL_ID}" \
    -H "x-submitter-kind: ${WEBHOOK_SUBMITTER_KIND}" \
    -H "x-intent-kind: ${WEBHOOK_INTENT_KIND}" \
    -H "x-webhook-id: ${WEBHOOK_ID}" \
    -H "x-webhook-signature: ${WEBHOOK_SIGNATURE}" \
    -H "content-type: application/json" \
    -d @"${WEBHOOK_PAYLOAD}")"
else
  SUBMIT_RESPONSE="$(curl -sS -X POST "${BASE_URL}/webhooks/${WEBHOOK_SOURCE}" \
    -H "authorization: Bearer ${INGRESS_TOKEN}" \
    -H "x-tenant-id: ${TENANT_ID}" \
    -H "x-principal-id: ${INGRESS_PRINCIPAL_ID}" \
    -H "x-submitter-kind: ${WEBHOOK_SUBMITTER_KIND}" \
    -H "x-intent-kind: ${WEBHOOK_INTENT_KIND}" \
    -H "x-webhook-id: ${WEBHOOK_ID}" \
    -H "content-type: application/json" \
    -d @"${WEBHOOK_PAYLOAD}")"
fi
echo "${SUBMIT_RESPONSE}"

INTENT_ID=""
if command -v jq >/dev/null 2>&1; then
  INTENT_ID="$(echo "${SUBMIT_RESPONSE}" | jq -r '.intent_id // empty')"
fi
if [[ -z "${INTENT_ID}" ]]; then
  INTENT_ID="$(echo "${SUBMIT_RESPONSE}" | sed -n 's/.*"intent_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
fi
if [[ -z "${INTENT_ID}" ]]; then
  echo "Could not extract intent_id from webhook submit response." >&2
  exit 1
fi

echo
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
