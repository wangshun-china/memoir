#!/usr/bin/env bash
PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec bash "$PROJECT_ROOT/scripts/miniprogram-env.sh" local
