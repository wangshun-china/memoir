#!/usr/bin/env bash

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec "$PROJECT_ROOT/scripts/local_dev.sh" start
