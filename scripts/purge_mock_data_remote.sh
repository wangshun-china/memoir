#!/usr/bin/env bash
# Purge business mock data on the production server Postgres container.
set -euo pipefail
HOST="${REMOTE_SSH_HOST:?}"
PASS="${REMOTE_SSH_PASS:?}"

sshpass -p "$PASS" ssh -o StrictHostKeyChecking=no "root@${HOST}" bash -s <<'REMOTE'
set -euo pipefail
docker exec -i memoir-postgres psql -U memoir -d memoir -v ON_ERROR_STOP=1 <<'SQL'
BEGIN;
TRUNCATE TABLE
  interview_messages,
  interview_sessions,
  chapters,
  memoirs,
  llm_usage_logs,
  users
RESTART IDENTITY CASCADE;
COMMIT;
SELECT 'users' AS t, count(*) FROM users
UNION ALL SELECT 'memoirs', count(*) FROM memoirs
UNION ALL SELECT 'admin_accounts', count(*) FROM admin_accounts;
SQL
echo "[OK] remote business data purged"
REMOTE
