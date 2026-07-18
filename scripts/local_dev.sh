#!/usr/bin/env bash
# Thin wrapper → scripts/miniprogram-env.sh (single entry for local/remote stacks).
#
#   ./scripts/local_dev.sh start|rebuild  → miniprogram-env local
#   ./scripts/local_dev.sh stop           → miniprogram-env stop  (both sides)
#   ./scripts/local_dev.sh status|logs    → local-only helpers
#   ./scripts/local_dev.sh remote         → miniprogram-env remote

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
ENV_SCRIPT="$PROJECT_ROOT/scripts/miniprogram-env.sh"
COMMAND="${1:-start}"

die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

[ -x "$ENV_SCRIPT" ] || chmod +x "$ENV_SCRIPT" 2>/dev/null || true
[ -f "$ENV_SCRIPT" ] || die "Missing $ENV_SCRIPT"

case "$COMMAND" in
  start|rebuild|local)
    exec bash "$ENV_SCRIPT" local
    ;;
  remote)
    exec bash "$ENV_SCRIPT" remote
    ;;
  stop)
    exec bash "$ENV_SCRIPT" stop
    ;;
  status)
    exec bash "$ENV_SCRIPT" status
    ;;
  logs)
    command -v docker >/dev/null 2>&1 || die "Docker unavailable"
    cd "$PROJECT_ROOT"
    if [ -f "$PROJECT_ROOT/.env.development" ]; then
      exec docker compose -f docker-compose.dev.yml --env-file .env.development logs -f --tail 200 memoir-api
    else
      exec docker compose -f docker-compose.dev.yml logs -f --tail 200 memoir-api
    fi
    ;;
  *)
    cat >&2 <<'EOF'
Usage: scripts/local_dev.sh {start|rebuild|stop|status|logs|remote}

  start|rebuild  Start local stack, stop remote, point mini-program to local
  remote         Start remote stack, stop local, point mini-program to remote
  stop           Stop local AND remote containers
  status         Show both stacks + env.ts
  logs           Tail local memoir-api logs

Preferred alias: ./scripts/miniprogram-env.sh {local|remote|stop|status}
EOF
    exit 1
    ;;
esac
