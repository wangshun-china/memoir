#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
COMPOSE_FILE="$PROJECT_ROOT/docker-compose.dev.yml"
ENV_FILE="$PROJECT_ROOT/.env.development"
COMMAND="${1:-start}"

log() { printf '\n==> %s\n' "$1"; }
die() { printf 'ERROR: %s\n' "$1" >&2; exit 1; }

compose() {
  if [ -f "$ENV_FILE" ]; then
    docker compose -f "$COMPOSE_FILE" --env-file "$ENV_FILE" "$@"
  else
    docker compose -f "$COMPOSE_FILE" "$@"
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

load_dotenv_file_values() {
  # Load KEY/BASE/MODEL from .env.development without clobbering non-empty shell values.
  [ -f "$ENV_FILE" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      ''|\#*) continue ;;
    esac
    key="${line%%=*}"
    val="${line#*=}"
    case "$key" in
      LLM_API_BASE)
        if [ -z "${LLM_API_BASE:-}" ]; then export LLM_API_BASE="$val"; fi
        ;;
      LLM_API_KEY)
        if [ -z "${LLM_API_KEY:-}" ]; then export LLM_API_KEY="$val"; fi
        ;;
      LLM_MODEL)
        if [ -z "${LLM_MODEL:-}" ]; then export LLM_MODEL="$val"; fi
        ;;
    esac
  done < "$ENV_FILE"
}

prepare_local_ai_env() {
  import_windows_machine_env LOCAL_AGENT_BASE_URL
  import_windows_machine_env LOCAL_AGENT_MODEL
  import_windows_machine_env LOCAL_AGENT_API_KEY
  import_windows_machine_env DASHSCOPE_API_KEY
  load_dotenv_file_values

  # Prefer non-empty values: empty LLM_API_BASE= in .env must not block LOCAL_AGENT_*.
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
      printf 'WARN: LLM_API_KEY empty — interviewer will use local fallback\n'
    fi
  else
    printf 'WARN: LLM_API_BASE empty — interviewer will use local fallback\n'
  fi
}

wait_for_api() {
  local port="${MEMOIR_API_PORT:-18081}"
  local i

  if [ -f "$ENV_FILE" ]; then
    # shellcheck disable=SC1090
    set -a
    source "$ENV_FILE"
    set +a
    port="${MEMOIR_API_PORT:-18081}"
  fi

  for i in $(seq 1 40); do
    if curl -fsS "http://127.0.0.1:${port}/health" >/dev/null 2>&1; then
      printf '[OK] memoir-api is healthy at http://127.0.0.1:%s\n' "$port"
      printf '    admin console: http://127.0.0.1:%s/admin/\n' "$port"
      printf '    miniprogram API: http://127.0.0.1:%s/api/v1\n' "$port"
      return
    fi
    sleep 1
  done

  compose logs --tail 100 memoir-api
  die "memoir-api health check timed out"
}

command -v docker >/dev/null 2>&1 || die "Docker is not available in WSL."
docker compose version >/dev/null 2>&1 || die "Docker Compose plugin is unavailable."
[ -f "$COMPOSE_FILE" ] || die "Missing $COMPOSE_FILE"

cd "$PROJECT_ROOT"

case "$COMMAND" in
  start|rebuild)
    prepare_local_ai_env
    "$PROJECT_ROOT/scripts/miniprogram-env.sh" local
    log "Building and starting local PostgreSQL and API containers"
    compose up -d --build
    wait_for_api
    compose ps
    ;;
  stop)
    log "Stopping local containers (database volume is preserved)"
    compose down
    ;;
  logs)
    compose logs -f --tail 200 memoir-api
    ;;
  status)
    compose ps
    ;;
  *)
    die "Usage: $0 {start|stop|rebuild|logs|status}"
    ;;
esac
