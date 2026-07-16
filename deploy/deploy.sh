#!/usr/bin/env bash

set -Eeuo pipefail

DEPLOY_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
COMPOSE_FILE="$DEPLOY_DIR/docker-compose.yml"
RUNTIME_ENV="$DEPLOY_DIR/.env"
RELEASE_ENV="$DEPLOY_DIR/.release.env"
NEXT_RELEASE_ENV="$DEPLOY_DIR/.release.env.next"
PREVIOUS_RELEASE_ENV="$DEPLOY_DIR/.release.env.previous"

die() { printf 'ERROR: %s\n' "$1" >&2; exit 1; }

[ -f "$COMPOSE_FILE" ] || die "Missing docker-compose.yml"
[ -f "$RUNTIME_ENV" ] || die "Missing runtime .env"

if [ -f "$NEXT_RELEASE_ENV" ]; then
  if [ -f "$RELEASE_ENV" ]; then
    cp "$RELEASE_ENV" "$PREVIOUS_RELEASE_ENV"
  fi
  mv "$NEXT_RELEASE_ENV" "$RELEASE_ENV"
fi

[ -f "$RELEASE_ENV" ] || die "Missing .release.env"

compose() {
  docker compose \
    -f "$COMPOSE_FILE" \
    --env-file "$RUNTIME_ENV" \
    --env-file "$RELEASE_ENV" \
    "$@"
}

rollback() {
  if [ ! -f "$PREVIOUS_RELEASE_ENV" ]; then
    return
  fi

  printf 'New release failed; rolling back to the previous image.\n' >&2
  cp "$PREVIOUS_RELEASE_ENV" "$RELEASE_ENV"
  compose up -d --remove-orphans
}

compose pull memoir-api
compose up -d --remove-orphans

for i in $(seq 1 40); do
  if compose exec -T memoir-api \
    curl -fsS http://127.0.0.1:8080/health >/dev/null 2>&1; then
    printf '[OK] memoir-api is healthy inside its container\n'
    compose ps
    exit 0
  fi
  sleep 2
done

compose logs --tail 120 memoir-api >&2
rollback
die "memoir-api failed its deployment health check"
