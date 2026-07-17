#!/usr/bin/env bash
# Ensure Docker Postgres is up, then run Stage-1 cargo tests.

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
# shellcheck disable=SC1091
source "$PROJECT_ROOT/.env.development" 2>/dev/null || true

export DATABASE_URL="${DATABASE_URL:-postgres://memoir:memoir@127.0.0.1:5433/memoir}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgres://memoir:memoir@127.0.0.1:5433/memoir_test}"
export JWT_SECRET="${JWT_SECRET:-dev-only-change-me-jwt-secret}"
export ALLOW_DEV_LOGIN=1

"$PROJECT_ROOT/scripts/dev-up.sh"
"$PROJECT_ROOT/scripts/ensure_test_db.sh"

cd "$PROJECT_ROOT/server"
# Tests force DATABASE_URL → memoir_test so the live app DB stays clean.
DATABASE_URL="$TEST_DATABASE_URL" cargo test "$@"
