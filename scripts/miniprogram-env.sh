#!/usr/bin/env bash
# Unified env switch for memoir miniprogram + Docker stacks.
#
#   ./scripts/miniprogram-env.sh local         # stop remote, start local (skip if already up), env.ts → local
#   ./scripts/miniprogram-env.sh local stop    # stop LOCAL only
#   ./scripts/miniprogram-env.sh remote        # stop local, start remote (skip if already up), env.ts → remote
#   ./scripts/miniprogram-env.sh remote stop   # stop REMOTE only
#   ./scripts/miniprogram-env.sh stop          # stop BOTH local and remote
#   ./scripts/miniprogram-env.sh status        # show local + public + remote status
#
# Remote SSH (optional env, never commit secrets):
#   REMOTE_SSH_HOST   default: 120.26.186.0
#   REMOTE_SSH_USER   default: root
#   REMOTE_SSH_KEY    private key path (preferred; e.g. ~/.ssh/memoir_github_actions)
#   REMOTE_SSH_PASS   if set and sshpass exists, used when key is missing
#   REMOTE_DEPLOY_DIR default: /opt/memoir/deploy
#   MEMOIR_PUBLIC_HEALTH_URL default: https://api.wangshun.work/health
#
# Also loads the same keys from .env.development when present.

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
COMMAND="${1:-}"
SUBCMD="${2:-}"
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
Usage: scripts/miniprogram-env.sh <command> [sub]

  local           Stop remote, start local (no-op if already healthy), env.ts → local
  local stop      Stop LOCAL stack only (remote untouched)
  remote          Stop local, start remote (no-op if already up), env.ts → remote
  remote stop     Stop REMOTE stack only (local untouched)
  stop            Stop LOCAL and REMOTE
  status          Show local + public health + remote compose

Examples:
  ./scripts/miniprogram-env.sh local
  ./scripts/miniprogram-env.sh local stop
  ./scripts/miniprogram-env.sh remote
  ./scripts/miniprogram-env.sh remote stop
  ./scripts/miniprogram-env.sh stop
  ./scripts/miniprogram-env.sh status
EOF
  exit 1
}

case "$COMMAND" in
  local|remote)
    case "$SUBCMD" in
      ''|stop|start) ;;
      *) usage ;;
    esac
    ;;
  stop|status)
    [ -z "$SUBCMD" ] || usage
    ;;
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
      REMOTE_SSH_HOST|REMOTE_SSH_USER|REMOTE_SSH_PASS|REMOTE_SSH_KEY|REMOTE_DEPLOY_DIR|MEMOIR_LAN_IP|MEMOIR_API_PORT|MEMOIR_PUBLIC_HEALTH_URL)
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

# Prefer explicit key, then common deploy key under ~/.ssh (copied from Windows if needed).
resolve_remote_ssh_key() {
  local candidates=()
  if [ -n "${REMOTE_SSH_KEY:-}" ]; then
    candidates+=("$REMOTE_SSH_KEY")
  fi
  candidates+=(
    "$HOME/.ssh/memoir_github_actions"
    "/mnt/c/Users/wangshun/.ssh/memoir_github_actions"
  )
  local c dst
  for c in "${candidates[@]}"; do
    [ -f "$c" ] || continue
    # OpenSSH rejects world-readable keys on /mnt/c; copy into ~/.ssh with 600.
    if [[ "$c" == /mnt/c/* ]] || [[ "$c" == /mnt/d/* ]]; then
      dst="$HOME/.ssh/memoir_github_actions"
      mkdir -p "$HOME/.ssh"
      cp "$c" "$dst"
      chmod 600 "$dst"
      printf '%s\n' "$dst"
      return 0
    fi
    chmod 600 "$c" 2>/dev/null || true
    printf '%s\n' "$c"
    return 0
  done
  return 1
}

REMOTE_KEY_FILE=""
if REMOTE_KEY_FILE="$(resolve_remote_ssh_key)"; then
  :
else
  REMOTE_KEY_FILE=""
fi

# --- SSH ---
ssh_auth_hint() {
  if [ -n "${REMOTE_KEY_FILE:-}" ]; then
    printf 'key:%s' "$REMOTE_KEY_FILE"
  elif [ -n "${REMOTE_SSH_PASS:-}" ] && command -v sshpass >/dev/null 2>&1; then
    printf 'sshpass+password'
  elif [ -n "${REMOTE_SSH_PASS:-}" ]; then
    printf 'password-set-but-no-sshpass'
  else
    printf 'no-key-no-password'
  fi
}

ssh_remote() {
  # BatchMode=yes: never hang on password prompt when key/sshpass missing.
  local opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=12 -o BatchMode=yes)
  if [ -n "${REMOTE_KEY_FILE:-}" ]; then
    opts+=(-i "$REMOTE_KEY_FILE" -o IdentitiesOnly=yes)
    ssh "${opts[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "$@"
  elif [ -n "${REMOTE_SSH_PASS:-}" ] && command -v sshpass >/dev/null 2>&1; then
    opts=(-o StrictHostKeyChecking=no -o ConnectTimeout=12)
    sshpass -p "$REMOTE_SSH_PASS" ssh "${opts[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "$@"
  else
    ssh "${opts[@]}" "${REMOTE_USER}@${REMOTE_HOST}" "$@"
  fi
}

# Public health without SSH (nginx → memoir-api). Does NOT prove compose layout.
check_public_api() {
  local url="${MEMOIR_PUBLIC_HEALTH_URL:-https://api.wangshun.work/health}"
  local body
  body="$(curl -fsS --connect-timeout 5 --max-time 8 -L "$url" 2>/dev/null || true)"
  if printf '%s' "$body" | grep -q memoir-server; then
    printf '[OK] public health: %s (API reachable; SSH not required)\n' "$url"
    return 0
  fi
  if [ -n "$body" ]; then
    printf '[OK] public health: %s (HTTP body received)\n' "$url"
    return 0
  fi
  warn "public health failed: $url (API may be down, or network/DNS blocked here)"
  return 1
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
  running)
    # Exit 0 if memoir-api container is running.
    cid="$(compose ps -q memoir-api 2>/dev/null || true)"
    if [ -z "$cid" ]; then
      exit 1
    fi
    running="$(docker inspect -f '{{.State.Running}}' "$cid" 2>/dev/null || echo false)"
    [ "$running" = "true" ]
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

local_is_healthy() {
  curl -fsS --connect-timeout 2 --max-time 4 \
    "http://127.0.0.1:${PORT_LOCAL}/health" >/dev/null 2>&1
}

remote_is_running() {
  remote_stack running >/dev/null 2>&1
}

stop_local() {
  require_local_docker
  log "Stopping LOCAL containers (volumes preserved)"
  cd "$PROJECT_ROOT"
  local_compose down || true
}

ensure_portproxy() {
  command -v powershell.exe >/dev/null 2>&1 || return 0
  local wsl_ip
  wsl_ip="$(hostname -I | awk '{print $1}')"
  [ -n "$wsl_ip" ] || return 0
  local current
  current="$(powershell.exe -NoProfile -NonInteractive -Command \
    "netsh interface portproxy show v4tov4 | Select-String '${PORT_LOCAL}'" 2>/dev/null | tr -d '\r' || true)"
  if [ -n "$current" ] && echo "$current" | grep -q "$wsl_ip"; then
    return 0
  fi
  log "Syncing Windows portproxy 0.0.0.0:${PORT_LOCAL} → ${wsl_ip}:${PORT_LOCAL}"
  if powershell.exe -NoProfile -NonInteractive -Command \
    "netsh interface portproxy delete v4tov4 listenaddress=0.0.0.0 listenport=${PORT_LOCAL} 2>\$null; netsh interface portproxy add v4tov4 listenaddress=0.0.0.0 listenport=${PORT_LOCAL} connectaddress=${wsl_ip} connectport=${PORT_LOCAL}" 2>/dev/null; then
    printf '[OK] portproxy updated\n'
  else
    warn "portproxy update failed (run scripts/win_expose_api.ps1 as Admin once)"
  fi
}

start_local() {
  require_local_docker
  cd "$PROJECT_ROOT"
  if local_is_healthy; then
    log "LOCAL already healthy on :${PORT_LOCAL} — skip start"
    local_compose ps || true
    ensure_portproxy
    return 0
  fi
  prepare_local_ai_env
  log "Starting LOCAL PostgreSQL + API (build if needed)"
  local_compose up -d --build
  wait_local_api
  local_compose ps
  ensure_portproxy
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
  if remote_is_running; then
    log "REMOTE memoir-api already running on ${REMOTE_HOST} — skip start"
    remote_stack status || true
    check_public_api || true
    return 0
  fi
  log "Starting REMOTE containers on ${REMOTE_USER}@${REMOTE_HOST}"
  remote_stack start || die "Failed to start remote stack"
  # Public health (best-effort)
  if check_public_api; then
    :
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
    case "$SUBCMD" in
      stop)
        stop_local
        log "Done: LOCAL stopped (remote unchanged)"
        ;;
      start)
        # start-only: do not touch remote; still point mini-program to local
        start_local
        write_miniprogram_env local
        log "Done: LOCAL up (remote unchanged), env.ts = local"
        ;;
      '')
        stop_remote || true
        start_local
        write_miniprogram_env local
        log "Done: local up (skip if already), remote stopped (best-effort), env.ts = local"
        ;;
    esac
    ;;
  remote)
    case "$SUBCMD" in
      stop)
        stop_remote || true
        log "Done: REMOTE stopped (local unchanged)"
        ;;
      start)
        start_remote
        write_miniprogram_env remote
        log "Done: REMOTE up (local unchanged), env.ts = remote"
        ;;
      '')
        stop_local
        start_remote
        write_miniprogram_env remote
        log "Done: remote up (skip if already), local stopped, env.ts = remote"
        ;;
    esac
    ;;
  stop)
    stop_local || true
    stop_remote || true
    log "Done: local + remote containers stopped"
    ;;
  status)
    log "LOCAL (docker compose on this machine)"
    if command -v docker >/dev/null 2>&1 && [ -f "$COMPOSE_LOCAL" ]; then
      cd "$PROJECT_ROOT"
      local_compose ps || true
      if local_is_healthy; then
        printf '[OK] local API health: http://127.0.0.1:%s/health\n' "$PORT_LOCAL"
      else
        warn "local containers may be up but API health failed on port ${PORT_LOCAL}"
      fi
    else
      warn "local docker unavailable"
    fi

    log "PUBLIC (no SSH — proves production API is up from here)"
    check_public_api || true

    log "REMOTE compose (${REMOTE_USER}@${REMOTE_HOST}, auth=$(ssh_auth_hint))"
    printf 'NOTE: listing remote containers needs SSH. Public health above does not.\n'
    if remote_stack status; then
      :
    else
      warn "SSH to ${REMOTE_USER}@${REMOTE_HOST} failed — cannot list remote containers."
      if [ -z "${REMOTE_KEY_FILE:-}" ] && [ -z "${REMOTE_SSH_PASS:-}" ]; then
        warn "Fix: set REMOTE_SSH_KEY=~/.ssh/memoir_github_actions in .env.development,"
        warn "  or REMOTE_SSH_PASS + sshpass, or install the deploy private key under ~/.ssh/."
      elif [ -n "${REMOTE_SSH_PASS:-}" ] && ! command -v sshpass >/dev/null 2>&1; then
        warn "REMOTE_SSH_PASS is set but sshpass is missing: sudo apt install -y sshpass"
      else
        warn "Check key path, password, host/user, or server authorized_keys."
      fi
    fi

    if [ -f "$ENV_DEST" ]; then
      log "miniprogram/config/env.ts (which API the mini-program calls)"
      grep 'API_BASE_URL' "$ENV_DEST" || true
    fi
    ;;
esac
