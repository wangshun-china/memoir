#!/usr/bin/env bash
# Ensure isolated memoir_test database exists for cargo tests.
set -euo pipefail
docker exec memoir-postgres psql -U memoir -d postgres -tc \
  "SELECT 1 FROM pg_database WHERE datname='memoir_test'" | grep -q 1 \
  || docker exec memoir-postgres psql -U memoir -d postgres -c "CREATE DATABASE memoir_test OWNER memoir;"
echo "[OK] memoir_test database ready"
