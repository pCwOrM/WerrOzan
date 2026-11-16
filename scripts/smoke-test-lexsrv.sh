#!/bin/bash
# WerrOzan Phase 0–4 End-to-End Verification & Smoke Test for lexsrv + Cloudflare Tunnel
set -euo pipefail

KEY_FILE="${WERROZAN_ACCESS_KEY_FILE:-$HOME/.config/werrozan/studio_access.key}"
BASE_LOCAL="${WERROZAN_LOCAL_URL:-http://127.0.0.1:8791}"
BASE_PUBLIC="${WERROZAN_PUBLIC_URL:-https://ozan-api.werr.io}"
ORIGIN="https://ozan.werr.io"

if [ ! -f "$KEY_FILE" ]; then
  echo "[ERROR] Missing access key file: $KEY_FILE" >&2
  exit 1
fi
ACCESS_KEY="$(tr -d '\r\n' < "$KEY_FILE")"

echo "[1/7] Local loopback health..."
curl -fsS "$BASE_LOCAL/health" | grep -q '"engine_bundle_present":true'

echo "[2/7] Negative auth guard (must reject unauthenticated external requests)..."
CODE=$(curl -sS -o /dev/null -w "%{http_code}" "$BASE_PUBLIC/health")
if [ "$CODE" != "401" ] && [ "$CODE" != "403" ]; then
  echo "[ERROR] Expected 401/403 without auth, got $CODE" >&2
  exit 1
fi

echo "[3/7] Negative origin guard (must reject untrusted Origin)..."
CODE_ORIG=$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: https://untrusted.invalid" "$BASE_PUBLIC/health")
if [ "$CODE_ORIG" != "403" ]; then
  echo "[ERROR] Expected 403 for untrusted Origin, got $CODE_ORIG" >&2
  exit 1
fi

echo "[4/7] Phase 1: Authenticated public endpoints via Cloudflare Tunnel..."
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" "$BASE_PUBLIC/health" | grep -q '"status":"ok"'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" "$BASE_PUBLIC/v1/system/resources" | grep -q '"ram_total_mb"'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" "$BASE_PUBLIC/setup/catalog" | grep -q '"recommended_profile_id":"light"'

echo "[5/7] Phase 2: Güneş Dili + WERR Z/9Z + Gemini status..."
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" "$BASE_PUBLIC/v1/werrozan/status" | grep -q '"werr_zmod9":true'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" -H "Content-Type: application/json" \
  -d '{"lyrics":"[Verse 1]\nAkdeniz kıyısı sessiz ve serin\n[Chorus]\nMavi gecelerde yükselir sesim","duration_seconds":30}' \
  "$BASE_PUBLIC/v1/werrozan/analyze" | grep -q '"vram_bytes":0'

echo "[6/7] Phase 3: 53-Koma Makam catalog & symbolic MIDI Pitch-Bend export..."
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" "$BASE_PUBLIC/v1/werrozan/makams" | grep -q '"makams"'
curl -fsS -H "Authorization: Bearer $ACCESS_KEY" -H "Origin: $ORIGIN" -H "Content-Type: application/json" \
  -d '{"abc":"X:1\nM:4/4\nL:1/16\nQ:1/4=80\nK:Dm\nd2e2d3dc2B2|","makam":"hicaz"}' \
  "$BASE_PUBLIC/v1/werrozan/koma/score" | grep -q '"ok":true'

echo "[7/7] Phase 4: Public Demo Track & Membership Mailer status..."
curl -fsS -H "Origin: $ORIGIN" "$BASE_PUBLIC/v1/demo" | grep -q '"available_on_server":true'
MAGIC=$(curl -fsS -r 0-3 -H "Origin: $ORIGIN" "$BASE_PUBLIC/v1/demo/audio")
if [ "$MAGIC" != "fLaC" ]; then
  echo "[ERROR] Expected fLaC header from /v1/demo/audio, got $MAGIC" >&2
  exit 1
fi

echo "[OK] WerrOzan Phase 0–4 smoke tests passed."
