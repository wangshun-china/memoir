#!/usr/bin/env bash
# Update production deploy/.env WECHAT_* keys on the server (does not print secrets).
# Usage (from WSL):
#   REMOTE_SSH_HOST=120.26.186.0 REMOTE_SSH_USER=root REMOTE_SSH_PASS='...' \
#   WECHAT_APP_ID=... WECHAT_APP_SECRET=... ./scripts/set_wechat_env_remote.sh

set -euo pipefail

HOST="${REMOTE_SSH_HOST:?set REMOTE_SSH_HOST}"
USER="${REMOTE_SSH_USER:-root}"
REMOTE_ENV="${REMOTE_ENV:-/opt/memoir/deploy/.env}"
APP_ID="${WECHAT_APP_ID:?set WECHAT_APP_ID}"
APP_SECRET="${WECHAT_APP_SECRET:?set WECHAT_APP_SECRET}"

ssh_cmd() {
  if [ -n "${REMOTE_SSH_PASS:-}" ] && command -v sshpass >/dev/null 2>&1; then
    sshpass -p "$REMOTE_SSH_PASS" ssh -o StrictHostKeyChecking=no "$@"
  else
    ssh -o StrictHostKeyChecking=no "$@"
  fi
}

ssh_cmd "${USER}@${HOST}" bash -s <<REMOTE
set -euo pipefail
ENV_FILE='$REMOTE_ENV'
if [ ! -f "\$ENV_FILE" ]; then
  echo "Missing \$ENV_FILE — create deploy runtime env first" >&2
  exit 1
fi
tmp=\$(mktemp)
# drop old wechat keys
grep -vE '^(WECHAT_APP_ID|WECHAT_APP_SECRET)=' "\$ENV_FILE" > "\$tmp" || true
printf 'WECHAT_APP_ID=%s\n' '$APP_ID' >> "\$tmp"
printf 'WECHAT_APP_SECRET=%s\n' '$APP_SECRET' >> "\$tmp"
install -m 600 "\$tmp" "\$ENV_FILE"
rm -f "\$tmp"
echo "Updated WECHAT keys in \$ENV_FILE"
# restart API if compose stack exists
if [ -f /opt/memoir/deploy/docker-compose.yml ]; then
  cd /opt/memoir/deploy
  if [ -f .release.env ]; then
    docker compose --env-file .env --env-file .release.env up -d --force-recreate memoir-api
  else
    docker compose --env-file .env up -d --force-recreate memoir-api 2>/dev/null || true
  fi
  sleep 3
  docker compose --env-file .env --env-file .release.env ps 2>/dev/null || docker compose --env-file .env ps 2>/dev/null || true
  # verify env inside container without printing secret
  docker exec memoir-api printenv WECHAT_APP_ID 2>/dev/null || docker exec memoir-api-dev printenv WECHAT_APP_ID 2>/dev/null || true
fi
REMOTE
echo "Remote WeChat env configured."
