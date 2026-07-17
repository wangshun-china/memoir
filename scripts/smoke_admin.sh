#!/usr/bin/env bash
set -euo pipefail
BASE="${1:-http://127.0.0.1:18081}"

curl -fsS "$BASE/health" | grep -q memoir-server
echo "[OK] health"

code=$(curl -fsS -o /dev/null -w '%{http_code}' "$BASE/admin/")
test "$code" = "200"
echo "[OK] admin page $code"

login_body='{"password":"admin123"}'
token=$(curl -fsS -X POST "$BASE/api/v1/admin/login" \
  -H 'Content-Type: application/json' \
  --data "$login_body" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
test -n "$token"
echo "[OK] admin login"

curl -fsS "$BASE/api/v1/admin/overview" \
  -H "Authorization: Bearer $token" | grep -q '"users"'
echo "[OK] overview"

curl -fsS -X POST "$BASE/api/v1/admin/ai-config/test" \
  -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' \
  --data '{"prompt":"hello"}' | grep -q '"ok":true'
echo "[OK] ai test"

curl -fsS "$BASE/api/v1/admin/ai-usage?limit=5" \
  -H "Authorization: Bearer $token" | grep -q '"summary"'
echo "[OK] usage"

echo "ALL SMOKE CHECKS PASSED"
