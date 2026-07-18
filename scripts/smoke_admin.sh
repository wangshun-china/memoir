#!/usr/bin/env bash
# Smoke: password login as is_admin user + admin API (miniprogram path; no web SPA).
set -euo pipefail
BASE="${1:-http://127.0.0.1:18081}"
USER="${ADMIN_SMOKE_USER:-wangshun}"
PASS="${ADMIN_SMOKE_PASS:-SmokePass9!}"

curl -fsS "$BASE/health" | grep -q memoir-server
echo "[OK] health"

# Web admin SPA removed
code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/admin/" || true)
if [ "$code" = "200" ]; then
  echo "[FAIL] unexpected web admin still at /admin/ ($code)"
  exit 1
fi
echo "[OK] no web admin SPA (HTTP $code)"

# Ensure known password via trial recovery key (also covers first-time create path via login).
curl -s -X POST "$BASE/api/v1/auth/reset-password" \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"$USER\",\"recovery_key\":\"wangshun\",\"new_password\":\"$PASS\"}" \
  >/dev/null || true
# If user does not exist yet, password login auto-registers (wangshun → is_admin).
token=$(curl -fsS -X POST "$BASE/api/v1/auth/password" \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"$USER\",\"password\":\"$PASS\"}" \
  | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
test -n "$token"
echo "[OK] password login $USER"

# is_admin flag
me=$(curl -fsS "$BASE/api/v1/me" -H "Authorization: Bearer $token")
echo "$me" | grep -q '"is_admin":true'
echo "[OK] is_admin"

curl -fsS "$BASE/api/v1/admin/overview" \
  -H "Authorization: Bearer $token" | grep -q '"users"'
echo "[OK] overview"

# Reset password with trial recovery key, then login with new pass
NEWPASS="SmokeReset9!"
curl -fsS -X POST "$BASE/api/v1/auth/reset-password" \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"$USER\",\"recovery_key\":\"wangshun\",\"new_password\":\"$NEWPASS\"}" \
  | grep -q '"ok":true'
echo "[OK] reset-password"

token2=$(curl -fsS -X POST "$BASE/api/v1/auth/password" \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"$USER\",\"password\":\"$NEWPASS\"}" \
  | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
test -n "$token2"
echo "[OK] login after reset"

# Restore smoke password for re-runs
curl -fsS -X POST "$BASE/api/v1/auth/reset-password" \
  -H 'Content-Type: application/json' \
  --data "{\"username\":\"$USER\",\"recovery_key\":\"wangshun\",\"new_password\":\"$PASS\"}" \
  | grep -q '"ok":true'
echo "[OK] restore password"

curl -fsS -X POST "$BASE/api/v1/admin/ai-config/test" \
  -H "Authorization: Bearer $token2" \
  -H 'Content-Type: application/json' \
  --data '{"prompt":"hello"}' | grep -q '"ok"'
echo "[OK] ai test"

echo "ALL SMOKE CHECKS PASSED"
