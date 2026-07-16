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
export JWT_SECRET="${JWT_SECRET:-dev-only-change-me-jwt-secret}"

"$PROJECT_ROOT/scripts/dev-up.sh"

cd "$PROJECT_ROOT/server"
cargo test "$@"
