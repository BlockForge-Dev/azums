#!/usr/bin/env bash
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

INGRESS_BASE_URL="${INGRESS_BASE_URL:-http://127.0.0.2:8000}"
INGRESS_TOKEN="${INGRESS_TOKEN:-dev-ingress-token}"
INGRESS_PRINCIPAL_ID="${INGRESS_PRINCIPAL_ID:-ingress-service}"
FREE_PLAY_LIMIT="${FREE_PLAY_LIMIT:-1}"
TO_WALLET="${TO_WALLET:-GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC}"
TENANT_ID="${TENANT_ID:-tenant_ws_quota_$(date +%s)}"

BASE="${INGRESS_BASE_URL%/}"
TMP_BODY="$(mktemp)"
trap 'rm -f "$TMP_BODY"' EXIT

request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  local status

  : >"$TMP_BODY"
  if [[ -n "$body" ]]; then
    status="$(curl -sS -o "$TMP_BODY" -w "%{http_code}" \
      -X "$method" "$url" \
      -H "content-type: application/json" \
      -H "authorization: Bearer $INGRESS_TOKEN" \
      -H "x-tenant-id: $TENANT_ID" \
      -H "x-principal-id: $INGRESS_PRINCIPAL_ID" \
      -H "x-submitter-kind: internal_service" \
      --data "$body")"
  else
    status="$(curl -sS -o "$TMP_BODY" -w "%{http_code}" \
      -X "$method" "$url" \
      -H "authorization: Bearer $INGRESS_TOKEN" \
      -H "x-tenant-id: $TENANT_ID" \
      -H "x-principal-id: $INGRESS_PRINCIPAL_ID" \
      -H "x-submitter-kind: internal_service")"
  fi
  printf "%s" "$status"
}

echo "Ingress API : $BASE"
echo "Tenant      : $TENANT_ID"
echo "Limit       : $FREE_PLAY_LIMIT"
echo

echo "1) Upsert tenant quota profile..."
QUOTA_BODY="$(jq -n --arg limit "$FREE_PLAY_LIMIT" '{
  plan: "developer",
  access_mode: "free_play",
  free_play_limit: ($limit | tonumber),
  updated_by_principal_id: "proof:quota_submit_enforcement"
}')"
QUOTA_STATUS="$(request "PUT" "$BASE/api/internal/tenants/$TENANT_ID/quota" "$QUOTA_BODY")"
if [[ "$QUOTA_STATUS" -lt 200 || "$QUOTA_STATUS" -ge 300 ]]; then
  echo "Quota upsert failed: status=$QUOTA_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi

INTENT_ONE="intent_quota_$(date +%s)_1"
INTENT_TWO="intent_quota_$(date +%s)_2"
SUBMIT_ONE_BODY="$(jq -n --arg intent "$INTENT_ONE" --arg to "$TO_WALLET" '{
  intent_kind: "solana.transfer.v1",
  payload: { intent_id: $intent, type: "transfer", to_addr: $to, amount: 1 }
}')"
SUBMIT_TWO_BODY="$(jq -n --arg intent "$INTENT_TWO" --arg to "$TO_WALLET" '{
  intent_kind: "solana.transfer.v1",
  payload: { intent_id: $intent, type: "transfer", to_addr: $to, amount: 1 }
}')"

echo "2) Submit first request (should be accepted)..."
FIRST_STATUS="$(request "POST" "$BASE/api/requests" "$SUBMIT_ONE_BODY")"
if [[ "$FIRST_STATUS" -lt 200 || "$FIRST_STATUS" -ge 300 ]]; then
  echo "First submit expected success but got status=$FIRST_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi
FIRST_INTENT_ID="$(jq -r '.intent_id // empty' "$TMP_BODY")"

echo "3) Submit second request (should be quota blocked)..."
SECOND_STATUS="$(request "POST" "$BASE/api/requests" "$SUBMIT_TWO_BODY")"
if [[ "$SECOND_STATUS" -ne 429 ]]; then
  echo "Second submit expected 429 but got status=$SECOND_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi

echo
echo "=== Proof Summary ==="
jq -n \
  --arg tenant_id "$TENANT_ID" \
  --arg limit "$FREE_PLAY_LIMIT" \
  --arg first_status "$FIRST_STATUS" \
  --arg first_intent "$FIRST_INTENT_ID" \
  --arg second_status "$SECOND_STATUS" \
  '{
    tenant_id: $tenant_id,
    free_play_limit: ($limit | tonumber),
    first_submit_status: ($first_status | tonumber),
    first_submit_intent_id: $first_intent,
    second_submit_status: ($second_status | tonumber),
    second_submit_quota_blocked: true
  }'

echo
echo "PASS: submit_request enforces tenant free_play quota."
