#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
TARGET="${1:-}"
SOURCE="$PROJECT_ROOT/config/miniprogram/env.${TARGET}.ts"
DESTINATION="$PROJECT_ROOT/miniprogram/config/env.ts"
PORT="${MEMOIR_API_PORT:-18081}"

case "$TARGET" in
  local|remote) ;;
  *)
    printf 'Usage: %s {local|remote}\n' "$0" >&2
    exit 1
    ;;
esac

[ -f "$SOURCE" ] || {
  printf 'Missing environment template: %s\n' "$SOURCE" >&2
  exit 1
}

cp "$SOURCE" "$DESTINATION"

# Resolve a host that phones on the same Wi-Fi can actually reach.
# WSL eth0 (172.26.x) is NOT reachable from phones — use Windows LAN IP when possible.
resolve_local_api_host() {
  local host=""

  # 1) Explicit override
  if [ -n "${MEMOIR_LAN_IP:-}" ]; then
    printf '%s\n' "$MEMOIR_LAN_IP"
    return
  fi

  # 2) On WSL: ask Windows for a 192.168.x address (typical home/office Wi-Fi)
  if grep -qi microsoft /proc/version 2>/dev/null && command -v powershell.exe >/dev/null 2>&1; then
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

  # 3) Fallback: WSL IP (only works for tools on the Windows host via special routing, not phones)
  if grep -qi microsoft /proc/version 2>/dev/null; then
    hostname -I | awk '{print $1}'
    return
  fi

  printf '127.0.0.1\n'
}

if [ "$TARGET" = "local" ]; then
  HOST="$(resolve_local_api_host)"
  # Rewrite any prior host:port in the template URL to HOST:PORT
  if grep -q 'API_BASE_URL' "$DESTINATION"; then
    # Replace the authority part of the URL after http://
    sed -i -E "s|API_BASE_URL = 'http://[^']+'|API_BASE_URL = 'http://${HOST}:${PORT}/api/v1'|" "$DESTINATION"
  fi

  printf 'Miniprogram API target: local\n'
  grep 'API_BASE_URL' "$DESTINATION"
  printf '\n'
  printf 'NOTE: Phones cannot reach WSL virtual IPs (172.x). For 真机调试/预览:\n'
  printf '  1) Keep API running: ./scripts/local_dev.sh start\n'
  printf '  2) On Windows (Admin PowerShell): scripts\\\\win_expose_api.ps1\n'
  printf '  3) Phone and PC on the same Wi-Fi\n'
  printf '  4) Recompile miniprogram after this script\n'
  printf 'Or use: %s remote  (requires deployed api.wangshun.work)\n' "$0"
else
  printf 'Miniprogram API target: remote\n'
  grep 'API_BASE_URL' "$DESTINATION"
fi
