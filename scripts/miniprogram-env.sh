#!/usr/bin/env bash
# Unified env switch for memoir miniprogram + Docker stacks.
#
#   ./scripts/miniprogram-env.sh local   # stop remote API stack, start local, point mini-program to LAN/local
#   ./scripts/miniprogram-env.sh remote  # stop local stack, start remote API stack, point mini-program to api.wangshun.work
#   ./scripts/miniprogram-env.sh stop    # stop BOTH local and remote stacks
#   ./scripts/miniprogram-env.sh status  # show local + remote container status
#
# Remote SSH (optional env, never commit passwords):
#   REMOTE_SSH_HOST   default: 120.26.186.0
#   REMOTE_SSH_USER   default: root
#   REMOTE_SSH_PASS   if set and sshpass exists, used for non-interactive SSH
#   REMOTE_DEPLOY_DIR default: /opt/memoir/deploy
#
# Also loads the same keys from .env.development when present.

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
COMMAND="${1:-}"
COMPOSE_LOCAL="$PROJECT_ROOT/docker-compose.dev.yml"
ENV_LOCAL="$PROJECT_ROOT/.env.development"
ENV_TEMPLATE_DIR="$PROJECT_ROOT/config/miniprogram"
ENV_DEST="$PROJECT_ROOT/miniprogram/config/env.ts"
PORT_LOCAL="${MEMOIR_API_PORT:-18081}"

log() { printf '\n==> %s\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

usage() {
  cat >&2 <<'EOF'
Usage: scripts/miniprogram-env.sh {local|remote|stop|status}

  local   Stop remote containers, start local Docker, write miniprogram env → local API
  remote  Stop local containers, start remote Docker, write miniprogram env → api.wangshun.work
  stop    Stop local AND remote containers
  status  Show local + remote compose status
EOF
  exit 1
}

case "$COMMAND" in
  local|remote|stop|status) ;;
  *) usage ;;
esac

# --- load optional overrides from .env.development ---
load_env_file() {
  [ -f "$ENV_LOCAL" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      ''|\#*) continue ;;
    esac
    key="${line%%=*}"
    val="${line#*=}"
    case "$key" in
      REMOTE_SSH_HOST|REMOTE_SSH_USER|REMOTE_SSH_PASS|REMOTE_DEPLOY_DIR|MEMOIR_LAN_IP|MEMOIR_API_PORT)
        if [ -z "${!key:-}" ]; then
          export "$key=$val"
        fi
        ;;
      LLM_API_BASE|LLM_API_KEY|LLM_MODEL|WECHAT_APP_ID|WECHAT_APP_SECRET|JWT_SECRET|ADMIN_RECOVERY_SECRET)
        if [ -z "${!key:-}" ]; then
          export "$key=$val"
        fi
        ;;
    esac
  done < "$ENV_LOCAL"
  PORT_LOCAL="${MEMOIR_API_PORT:-18081}"
}

load_env_file

REMOTE_HOST="${REMOTE_SSH_HOST:-120.26.186.0}"
REMOTE_USER="${REMOTE_SSH_USER:-root}"
REMOTE_DIR="${REMOTE_DEPLOY_DIR:-/opt/memoir/deploy}"

# --- SSH ---
ssh_remote() {
  # BatchMode=yes: never hang on password prompt when key/sshpass missing.
  local opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=10 -o BatchMode=yes)
  if [ -n "${REMOTE_SSH_PASS:-}" ] && command -v sshpass >/dev/null 2>&1; then
    # Password auth: drop BatchMode so sshpass can supply the password.
    opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=10)
    sshpass -p "$REMOTE_SSH_PASS" ssh "${opts[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "$@"
  else
    ssh "${opts[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "$@"
  fi
}

remote_stack() {
  local action="$1"
  ssh_remote env REMOTE_DIR="$REMOTE_DIR" ACTION="$action" bash -s <<'REMOTE'
set -euo pipefail
cd "$REMOTE_DIR" || { echo "missing $REMOTE_DIR" >&2; exit 1; }
[ -f docker-compose.yml ] || { echo "missing docker-compose.yml" >&2; exit 1; }
[ -f .env ] || { echo "missing .env" >&2; exit 1; }
compose() {
  if [ -f .release.env ]; then
    docker compose --env-file .env --env-file .release.env "$@"
  else
    docker compose --env-file .env "$@"
  fi
}
case "$ACTION" in
  stop)
    compose stop || true
    compose ps || true
    ;;
  start)
    # Start existing images (no rebuild on server).
    compose up -d --remove-orphans
    compose ps
    ;;
  status)
    compose ps || true
    ;;
  *)
    echo "unknown remote action: $ACTION" >&2
    exit 1
    ;;
esac
REMOTE
}

# --- local docker ---
require_local_docker() {
  command -v docker >/dev/null 2>&1 || die "Docker is not available (run in WSL with Docker)."
  docker compose version >/dev/null 2>&1 || die "Docker Compose plugin unavailable."
  [ -f "$COMPOSE_LOCAL" ] || die "Missing $COMPOSE_LOCAL"
}

local_compose() {
  if [ -f "$ENV_LOCAL" ]; then
    docker compose -f "$COMPOSE_LOCAL" --env-file "$ENV_LOCAL" "$@"
  else
    docker compose -f "$COMPOSE_LOCAL" "$@"
  fi
}

import_windows_machine_env() {
  local name="$1"
  local value=""
  [ -n "${!name:-}" ] && return
  command -v powershell.exe >/dev/null 2>&1 || return
  value="$(
    powershell.exe -NoProfile -NonInteractive -Command \
      "[Environment]::GetEnvironmentVariable('$name', 'Machine')" 2>/dev/null |
      tr -d '\r'
  )"
  if [ -n "$value" ]; then
    export "$name=$value"
  fi
}

prepare_local_ai_env() {
  import_windows_machine_env LOCAL_AGENT_BASE_URL
  import_windows_machine_env LOCAL_AGENT_MODEL
  import_windows_machine_env LOCAL_AGENT_API_KEY
  import_windows_machine_env DASHSCOPE_API_KEY

  if [ -z "${LLM_API_BASE:-}" ]; then
    export LLM_API_BASE="${LOCAL_AGENT_BASE_URL:-}"
  fi
  if [ -z "${LLM_MODEL:-}" ]; then
    export LLM_MODEL="${LOCAL_AGENT_MODEL:-gpt-4o-mini}"
  fi
  if [ -z "${LLM_API_KEY:-}" ]; then
    export LLM_API_KEY="${LOCAL_AGENT_API_KEY:-${DASHSCOPE_API_KEY:-}}"
  fi

  if [ -n "${LLM_API_BASE:-}" ]; then
    printf 'LLM_API_BASE=%s\n' "$LLM_API_BASE"
    printf 'LLM_MODEL=%s\n' "${LLM_MODEL:-}"
    if [ -n "${LLM_API_KEY:-}" ]; then
      printf 'LLM_API_KEY=set (len=%s)\n' "${#LLM_API_KEY}"
    else
      warn "LLM_API_KEY empty — interviewer may use fallback"
    fi
  else
    warn "LLM_API_BASE empty — interviewer may use fallback"
  fi
}

wait_local_api() {
  local i
  for i in $(seq 1 40); do
    if curl -fsS "http://127.0.0.1:${PORT_LOCAL}/health" >/dev/null 2>&1; then
      printf '[OK] local memoir-api healthy: http://127.0.0.1:%s\n' "$PORT_LOCAL"
      printf '     admin: miniprogram only (login as wangshun)\n'
      return 0
    fi
    sleep 1
  done
  local_compose logs --tail 80 memoir-api || true
  die "local memoir-api health check timed out"
}

stop_local() {
  require_local_docker
  log "Stopping LOCAL containers (volumes preserved)"
  cd "$PROJECT_ROOT"
  local_compose down || true
}

start_local() {
  require_local_docker
  prepare_local_ai_env
  log "Starting LOCAL PostgreSQL + API (build if needed)"
  cd "$PROJECT_ROOT"
  local_compose up -d --build
  wait_local_api
  local_compose ps
}

stop_remote() {
  log "Stopping REMOTE containers on ${REMOTE_USER}@${REMOTE_HOST}"
  if ! remote_stack stop; then
    warn "Could not stop remote stack (SSH or deploy dir issue). Continuing."
    return 1
  fi
  return 0
}

start_remote() {
  log "Starting REMOTE containers on ${REMOTE_USER}@${REMOTE_HOST}"
  remote_stack start || die "Failed to start remote stack"
  # Public health (best-effort)
  if curl -fsS --connect-timeout 5 "http://api.wangshun.work/health" >/dev/null 2>&1; then
    printf '[OK] public health: http://api.wangshun.work/health\n'
  else
    warn "Public health check failed or unreachable; stack may still be starting behind nginx."
  fi
}

# --- miniprogram env.ts ---
resolve_local_api_host() {
  if [ -n "${MEMOIR_LAN_IP:-}" ]; then
    printf '%s\n' "$MEMOIR_LAN_IP"
    return
  fi
  if grep -qi microsoft /proc/version 2>/dev/null && command -v powershell.exe >/dev/null 2>&1; then
    local host
    host="$(
      powershell.exe -NoProfile -NonInteractive -Command \
        "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { \$_.IPAddress -like '192.168.*' } | Sort-Object InterfaceMetric | Select-Object -First 1 -ExpandProperty IPAddress)" \
        2>/dev/null | tr -d '\r' | tr -d ' ' | tail -n 1
    )"
    if [[ "$host" =~ ^192\.168\.[0-9]+\.[0-9]+$ ]]; then
      printf '%s\n' "$host"
      return
    fi
  fi
  if grep -qi microsoft /proc/version 2>/dev/null; then
    hostname -I | awk '{print $1}'
    return
  fi
  printf '127.0.0.1\n'
}

write_miniprogram_env() {
  local target="$1"
  local src="$ENV_TEMPLATE_DIR/env.${target}.ts"
  [ -f "$src" ] || die "Missing template $src"
  mkdir -p "$(dirname "$ENV_DEST")"
  cp "$src" "$ENV_DEST"

  if [ "$target" = "local" ]; then
    local host
    host="$(resolve_local_api_host)"
    sed -i -E "s|API_BASE_URL = 'http://[^']+'|API_BASE_URL = 'http://${host}:${PORT_LOCAL}/api/v1'|" "$ENV_DEST"
    log "Miniprogram → LOCAL"
    grep 'API_BASE_URL' "$ENV_DEST"
    printf '\nNOTE: 真机调试请在 Windows 管理员 PowerShell 执行:\n'
    printf '  powershell -ExecutionPolicy Bypass -File scripts\\win_expose_api.ps1\n'
    printf '手机与电脑同一 Wi-Fi 后重新编译小程序。\n'
  else
    log "Miniprogram → REMOTE"
    grep 'API_BASE_URL' "$ENV_DEST"
  fi
}

# --- main ---
case "$COMMAND" in
  local)
    stop_remote || true
    start_local
    write_miniprogram_env local
    log "Done: local stack up, remote stopped (best-effort), env.ts = local"
    ;;
  remote)
    stop_local
    start_remote
    write_miniprogram_env remote
    log "Done: remote stack up, local stopped, env.ts = remote"
    ;;
  stop)
    stop_local || true
    stop_remote || true
    log "Done: local + remote containers stopped"
    ;;
  status)
    log "LOCAL"
    if command -v docker >/dev/null 2>&1 && [ -f "$COMPOSE_LOCAL" ]; then
      cd "$PROJECT_ROOT"
      local_compose ps || true
    else
      warn "local docker unavailable"
    fi
    log "REMOTE (${REMOTE_USER}@${REMOTE_HOST})"
    remote_stack status || warn "remote status unavailable"
    if [ -f "$ENV_DEST" ]; then
      log "miniprogram/config/env.ts"
      grep 'API_BASE_URL' "$ENV_DEST" || true
    fi
    ;;
esac
