#!/usr/bin/env bash
# Remove application mock/test rows from the live memoir database.
# Keeps admin_accounts and app_settings. Truncates business tables.
set -euo pipefail

PSQL_CMD=(docker exec -i memoir-postgres psql -U memoir -d memoir -v ON_ERROR_STOP=1)

if ! docker ps --format '{{.Names}}' | grep -qx memoir-postgres; then
  echo "memoir-postgres container not running" >&2
  exit 1
fi

echo "==> Purging business data (keeping admin_accounts / app_settings)"
"${PSQL_CMD[@]}" <<'SQL'
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

SELECT 'users' AS table, count(*) FROM users
UNION ALL SELECT 'memoirs', count(*) FROM memoirs
UNION ALL SELECT 'chapters', count(*) FROM chapters
UNION ALL SELECT 'interview_sessions', count(*) FROM interview_sessions
UNION ALL SELECT 'interview_messages', count(*) FROM interview_messages
UNION ALL SELECT 'llm_usage_logs', count(*) FROM llm_usage_logs
UNION ALL SELECT 'admin_accounts', count(*) FROM admin_accounts;
SQL
echo "[OK] business tables empty"
