#!/bin/bash
# WerrOzan Phase 1 Verification & Smoke Test for lexsrv + Cloudflare Tunnel
set -euo pipefail

KEY_FILE="${WERROZAN_ACCESS_KEY_FILE:-$HOME/.config/werrozan/studio_access.key}"
BASE_LOCAL="${WERROZAN_LOCAL_URL:-http://127.0.0.1:8791}"
BASE_PUBLIC="${WERROZAN_PUBLIC_URL:-https://ozan-api.werr.io}"

if [ ! -f "$KEY_FILE" ]; then
  echo "[ERROR] Missing access key file: $KEY_FILE" >&2
  exit 1
fi
ACCESS_KEY="$(tr -d '\r\n' < "$KEY_FILE")"

echo "[1/4] Local loopback health..."
curl -fsS "$BASE_LOCAL/health" | grep -q '"engine_bundle_present":true'

echo "[2/4] Negative auth guard (must reject unauthenticated external requests)..."
CODE=$(curl -sS -o /dev/null -w "%{http_code}" "$BASE_PUBLIC/health")
if [ "$CODE" != "401" ] && [ "$CODE" != "403" ]; then
  echo "[ERROR] Expected 401/403 without auth, got $CODE" >&2
  exit 1
fi

echo "[3/4] Negative origin guard (must reject untrusted Origin)..."
CODE_ORIG=$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: https://untrusted.invalid" "$BASE_PUBLIC/health")
if [ "$CODE_ORIG" != "403" ]; then
  echo "[ERROR] Expected 403 for untrusted Origin, got $CODE_ORIG" >&2
  exit 1
fi

echo "[4/4] Authenticated public endpoints via Cloudflare Tunnel..."
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" "$BASE_PUBLIC/health" | grep -q '"status":"ok"'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" "$BASE_PUBLIC/v1/system/resources" | grep -q '"ram_total_mb"'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" "$BASE_PUBLIC/setup/catalog" | grep -q '"recommended_profile_id":"light"'

echo "[OK] WerrOzan Phase 1 smoke tests passed."
