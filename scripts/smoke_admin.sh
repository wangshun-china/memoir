#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-http://127.0.0.1:18081}"
USER="smoke_admin_$$"
PASS="SmokePass9!"

curl -fsS "$BASE/health" | grep -q memoir-server
echo "[OK] health"

code=$(curl -fsS -o /dev/null -w '%{http_code}' "$BASE/admin/")
test "$code" = "200"
echo "[OK] admin page $code"

status_json=$(curl -fsS "$BASE/api/v1/admin/setup-status")
echo "[OK] setup-status $status_json"

if echo "$status_json" | grep -q '"needs_setup":true'; then
  token=$(curl -fsS -X POST "$BASE/api/v1/admin/setup" \
    -H 'Content-Type: application/json' \
    --data "{\"username\":\"$USER\",\"password\":\"$PASS\",\"display_name\":\"Smoke\"}" \
    | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
  test -n "$token"
  echo "[OK] first-time setup created admin $USER"
else
  # Login with credentials from env if provided, else create is already done —
  # for CI/local re-runs use last known test user when possible.
  if [ -n "${ADMIN_SMOKE_USER:-}" ] && [ -n "${ADMIN_SMOKE_PASS:-}" ]; then
    USER="$ADMIN_SMOKE_USER"
    PASS="$ADMIN_SMOKE_PASS"
  else
    # Create is blocked; require explicit creds for re-run
    echo "Admin already exists. Set ADMIN_SMOKE_USER / ADMIN_SMOKE_PASS to continue smoke login."
    echo "Or reset DB volume for first-time setup test."
    # Still verify login fails without real account mock
    code=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$BASE/api/v1/admin/login" \
      -H 'Content-Type: application/json' \
      --data '{"username":"nope","password":"wrong-password-xx"}')
    test "$code" = "401"
    echo "[OK] mock password rejected (401)"
    echo "SMOKE PARTIAL (setup already done)"
    exit 0
  fi
  token=$(curl -fsS -X POST "$BASE/api/v1/admin/login" \
    -H 'Content-Type: application/json' \
    --data "{\"username\":\"$USER\",\"password\":\"$PASS\"}" \
    | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
  test -n "$token"
  echo "[OK] admin login $USER"
fi

curl -fsS "$BASE/api/v1/admin/overview" \
  -H "Authorization: Bearer $token" | grep -q '"users"'
echo "[OK] overview (real DB counts)"

curl -fsS -X POST "$BASE/api/v1/admin/ai-config/test" \
  -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  --data '{"prompt":"hello"}' | grep -q '"ok":true'
echo "[OK] ai test"

curl -fsS "$BASE/api/v1/admin/ai-usage?limit=5" \
  -H "Authorization: Bearer $token" | grep -q '"summary"'
echo "[OK] usage"

echo "ALL SMOKE CHECKS PASSED"
