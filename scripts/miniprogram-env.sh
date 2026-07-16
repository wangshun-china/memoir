#!/usr/bin/env bash

if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -Eeuo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
TARGET="${1:-}"
SOURCE="$PROJECT_ROOT/config/miniprogram/env.${TARGET}.ts"
DESTINATION="$PROJECT_ROOT/miniprogram/config/env.ts"

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

if [ "$TARGET" = "local" ] && grep -qi microsoft /proc/version 2>/dev/null; then
  WSL_IP="$(hostname -I | awk '{print $1}')"
  [ -n "$WSL_IP" ] || {
    printf 'Could not determine the WSL IP address.\n' >&2
    exit 1
  }
  sed -i "s/127\\.0\\.0\\.1/$WSL_IP/" "$DESTINATION"
fi

printf 'Miniprogram API target: %s\n' "$TARGET"
grep 'API_BASE_URL' "$DESTINATION"
