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

OPERATOR_UI_BASE_URL="${OPERATOR_UI_BASE_URL:-http://127.0.0.2:8083}"
INGRESS_BASE_URL="${INGRESS_BASE_URL:-http://127.0.0.2:8000}"
OPERATOR_UI_EMAIL="${OPERATOR_UI_EMAIL:-demo@azums.dev}"
OPERATOR_UI_PASSWORD="${OPERATOR_UI_PASSWORD:-dev-password}"
TO_WALLET="${TO_WALLET:-GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC}"
PRINCIPAL_ID="${PRINCIPAL_ID:-}"

resolve_operator_ui_base() {
  local candidate="${1%/}"
  local headers headers_lc

  headers="$(curl -sS -I --max-time 4 "$candidate/" 2>/dev/null || true)"
  headers_lc="$(printf "%s" "$headers" | tr '[:upper:]' '[:lower:]')"
  if ! grep -q "x-powered-by: next.js" <<<"$headers_lc"; then
    printf "%s" "$candidate"
    return 0
  fi

  case "$candidate" in
    http://localhost:*|http://127.0.0.1:*|https://localhost:*|https://127.0.0.1:*)
      ;;
    *)
      printf "%s" "$candidate"
      return 0
      ;;
  esac

  local scheme rest hostport port fallback fallback_headers fallback_lc
  scheme="${candidate%%://*}"
  rest="${candidate#*://}"
  hostport="${rest%%/*}"
  if [[ "$hostport" == *:* ]]; then
    port="${hostport##*:}"
  else
    if [[ "$scheme" == "https" ]]; then
      port="443"
    else
      port="80"
    fi
  fi
  fallback="${scheme}://127.0.0.2:${port}"

  fallback_headers="$(curl -sS -I --max-time 4 "$fallback/" 2>/dev/null || true)"
  fallback_lc="$(printf "%s" "$fallback_headers" | tr '[:upper:]' '[:lower:]')"
  if ! grep -q "x-powered-by: next.js" <<<"$fallback_lc"; then
    echo "Detected Next.js at $candidate; using operator_ui backend endpoint $fallback instead." >&2
    printf "%s" "$fallback"
    return 0
  fi

  echo "Warning: Detected Next.js at $candidate. If proof fails, set OPERATOR_UI_BASE_URL to backend endpoint." >&2
  printf "%s" "$candidate"
}

UI="$(resolve_operator_ui_base "$OPERATOR_UI_BASE_URL")"
INGRESS="${INGRESS_BASE_URL%/}"
RUN_ID="$(date +%s)"
COOKIE_JAR="$(mktemp)"
TMP_BODY="$(mktemp)"
trap 'rm -f "$COOKIE_JAR" "$TMP_BODY"' EXIT

request() {
  local method="$1"
  local url="$2"
  local body="${3:-}"
  shift 3 || true
  local status

  : >"$TMP_BODY"
  if [[ -n "$body" ]]; then
    status="$(curl -sS -o "$TMP_BODY" -w "%{http_code}" \
      -X "$method" "$url" \
      -H "content-type: application/json" \
      -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
      "$@" \
      --data "$body")"
  else
    status="$(curl -sS -o "$TMP_BODY" -w "%{http_code}" \
      -X "$method" "$url" \
      -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
      "$@")"
  fi
  printf "%s" "$status"
}

echo "Operator UI : $UI"
echo "Ingress API : $INGRESS"
echo "Email       : $OPERATOR_UI_EMAIL"
echo

echo "1) Login..."
LOGIN_STATUS="$(request "POST" "$UI/api/ui/account/login" "$(jq -n --arg email "$OPERATOR_UI_EMAIL" --arg password "$OPERATOR_UI_PASSWORD" '{email:$email,password:$password}')" )"
if [[ "$LOGIN_STATUS" -lt 200 || "$LOGIN_STATUS" -ge 300 ]]; then
  echo "Login failed: status=$LOGIN_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi

echo "2) Session lookup..."
SESSION_STATUS="$(request "GET" "$UI/api/ui/account/session" "")"
if [[ "$SESSION_STATUS" -lt 200 || "$SESSION_STATUS" -ge 300 ]]; then
  echo "Session lookup failed: status=$SESSION_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi
TENANT_ID="$(jq -r '.session.tenant_id // empty' "$TMP_BODY")"
AUTHED="$(jq -r '.authenticated // false' "$TMP_BODY")"
if [[ "$AUTHED" != "true" || -z "$TENANT_ID" ]]; then
  echo "Session not authenticated or tenant missing: $(cat "$TMP_BODY")" >&2
  exit 1
fi
echo "   tenant_id=$TENANT_ID"

echo "3) Create API key from UI..."
CREATE_STATUS="$(request "POST" "$UI/api/ui/account/api-keys" "$(jq -n --arg name "proof-$RUN_ID" '{name:$name}')" )"
if [[ "$CREATE_STATUS" -lt 200 || "$CREATE_STATUS" -ge 300 ]]; then
  echo "Create API key failed: status=$CREATE_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi
KEY_ID="$(jq -r '.key.id // empty' "$TMP_BODY")"
API_TOKEN="$(jq -r '.token // empty' "$TMP_BODY")"
if [[ -z "$KEY_ID" || -z "$API_TOKEN" ]]; then
  echo "Create API key response missing key.id/token: $(cat "$TMP_BODY")" >&2
  exit 1
fi
echo "   key_id=$KEY_ID"

INTENT_OK="intent_api_key_proof_${RUN_ID}_ok"
INTENT_REVOKED="intent_api_key_proof_${RUN_ID}_revoked"
REQUEST_BODY_OK="$(jq -n --arg intent "$INTENT_OK" --arg to "$TO_WALLET" '{intent_kind:"solana.transfer.v1",payload:{intent_id:$intent,type:"transfer",to_addr:$to,amount:1}}')"
REQUEST_BODY_REVOKED="$(jq -n --arg intent "$INTENT_REVOKED" --arg to "$TO_WALLET" '{intent_kind:"solana.transfer.v1",payload:{intent_id:$intent,type:"transfer",to_addr:$to,amount:1}}')"

echo "4) Submit with created key (should succeed)..."
SUBMIT_HEADERS=(-H "x-tenant-id: $TENANT_ID" -H "x-submitter-kind: api_key_holder" -H "x-api-key: $API_TOKEN")
if [[ -n "$PRINCIPAL_ID" ]]; then
  SUBMIT_HEADERS+=(-H "x-principal-id: $PRINCIPAL_ID")
fi
SUBMIT_STATUS="$(request "POST" "$INGRESS/api/requests" "$REQUEST_BODY_OK" "${SUBMIT_HEADERS[@]}")"
if [[ "$SUBMIT_STATUS" -lt 200 || "$SUBMIT_STATUS" -ge 300 ]]; then
  echo "Submit with new key failed: status=$SUBMIT_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi
SUBMIT_INTENT="$(jq -r '.intent_id // empty' "$TMP_BODY")"
echo "   intent_id=${SUBMIT_INTENT:-unknown}"

echo "5) Revoke API key from UI..."
REVOKE_STATUS="$(request "POST" "$UI/api/ui/account/api-keys/$KEY_ID/revoke" "")"
if [[ "$REVOKE_STATUS" -lt 200 || "$REVOKE_STATUS" -ge 300 ]]; then
  echo "Revoke failed: status=$REVOKE_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi

echo "6) Submit with revoked key (should fail)..."
REVOKED_STATUS="$(request "POST" "$INGRESS/api/requests" "$REQUEST_BODY_REVOKED" "${SUBMIT_HEADERS[@]}")"
if [[ "$REVOKED_STATUS" -ne 401 && "$REVOKED_STATUS" -ne 403 ]]; then
  echo "Expected 401/403 after revoke, got status=$REVOKED_STATUS body=$(cat "$TMP_BODY")" >&2
  exit 1
fi

echo
echo "=== Proof Summary ==="
jq -n \
  --arg tenant_id "$TENANT_ID" \
  --arg key_id "$KEY_ID" \
  --arg submit_status "$SUBMIT_STATUS" \
  --arg submit_intent "${SUBMIT_INTENT:-}" \
  --arg revoke_status "$REVOKE_STATUS" \
  --arg revoked_status "$REVOKED_STATUS" \
  '{
    tenant_id: $tenant_id,
    created_key_id: $key_id,
    submit_with_new_key_status: ($submit_status|tonumber),
    submit_with_new_key_intent_id: $submit_intent,
    revoke_status: ($revoke_status|tonumber),
    submit_with_revoked_key_status: ($revoked_status|tonumber),
    revoked_key_blocked: true
  }'

echo
echo "PASS: create -> submit works, revoke -> submit is blocked."
