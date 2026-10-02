#!/usr/bin/env bash
# CI / local fail-closed runner for forge protocol smokes (D-QH-04).
# Brings up Compose, asserts Traefik/.git/LFS/SSH/packages routing (and SSH TCP),
# then tears down. Client ls-remote/push/LFS transfer need a seeded repo — those
# stay optional via SMOKE_SKIP_* (routing+TCP still prove protocol edges in CI).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Always fail closed in this entrypoint (even if CI env is unset locally).
export CI="${CI:-true}"
export SMOKE_REQUIRE_STACK=1
export SMOKE_SKIP_LS_REMOTE="${SMOKE_SKIP_LS_REMOTE:-1}"
export SMOKE_SKIP_LFS_CLIENT="${SMOKE_SKIP_LFS_CLIENT:-1}"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
# shellcheck source=scripts/docker-wsl-creds.sh
source "${ROOT}/scripts/docker-wsl-creds.sh"
SMOKE_NAME="ci-smoke-protocol"
smoke_require_docker

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"

cleanup() {
  docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> docker compose config"
docker compose -f "$COMPOSE_FILE" config >/dev/null

if [[ "${OXIDEAN_COMPOSE_SKIP_BUILD:-}" == "1" ]]; then
  echo "==> docker compose up -d --wait (OXIDEAN_COMPOSE_SKIP_BUILD=1; using preloaded images)"
  docker compose -f "$COMPOSE_FILE" up -d --wait
else
  echo "==> docker compose up --build -d --wait"
  docker compose -f "$COMPOSE_FILE" up --build -d --wait
fi

echo "==> wait for ${BASE_URL}/health"
ok=0
for _ in $(seq 1 90); do
  if curl -fsS -o /dev/null "${BASE_URL}/health" 2>/dev/null; then
    ok=1
    break
  fi
  sleep 2
done
if [[ "$ok" -ne 1 ]]; then
  echo "FAIL: health check failed at ${BASE_URL}/health after compose up" >&2
  docker compose -f "$COMPOSE_FILE" ps >&2 || true
  exit 1
fi

echo "==> make smoke-git-https (routing; SMOKE_SKIP_LS_REMOTE=${SMOKE_SKIP_LS_REMOTE})"
./scripts/smoke-git-https.sh

echo "==> make smoke-git-ssh (TCP; SMOKE_SKIP_LS_REMOTE=${SMOKE_SKIP_LS_REMOTE})"
./scripts/smoke-git-ssh.sh

echo "==> make smoke-git-lfs (routing; SMOKE_SKIP_LFS_CLIENT=${SMOKE_SKIP_LFS_CLIENT})"
./scripts/smoke-git-lfs.sh

echo "==> make smoke-packages (registry PathPrefix routing)"
./scripts/smoke-packages.sh

echo "==> ci-smoke-protocol OK"
