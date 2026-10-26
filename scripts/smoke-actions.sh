#!/usr/bin/env bash
# Compose Actions / Runners smoke (ACT-04 / ACT-05 / D-ACT-11 / D-ACT-14).
#
# Env knobs:
#   OXIDEAN_SMOKE_URL   default http://localhost
#
# Operator hosts without Docker/stack: exits 0 with a skip message.
# CI=true or SMOKE_REQUIRE_STACK=1 fails closed.
# Never prints registration tokens or other secrets.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
SMOKE_NAME="smoke-actions"

BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"
BASE_URL="${BASE_URL%/}"

echo "==> check official runner Dockerfile"
if [[ ! -f docker/oxidean-runner/Dockerfile ]]; then
  echo "missing docker/oxidean-runner/Dockerfile" >&2
  exit 1
fi
if ! grep -q 'oxidean-runner' docker/oxidean-runner/Dockerfile; then
  echo "Dockerfile must build/install the oxidean-runner binary" >&2
  exit 1
fi
if ! grep -qE 'register|ORIGIN|token|label' docker/oxidean-runner/README.md; then
  echo "runner README missing register docs" >&2
  exit 1
fi
if ! grep -qE 'oxidean-runner|profiles:.*actions' docker-compose.yml; then
  echo "docker-compose.yml missing actions runner service" >&2
  exit 1
fi
echo "OK: runner image + compose profile present"

smoke_require_docker

if docker info >/dev/null 2>&1; then
  echo "==> docker build oxidean-runner (best-effort, repo-root context)"
  if ! docker build -q -t oxidean-runner:smoke -f docker/oxidean-runner/Dockerfile .; then
    smoke_require_or_skip "docker build oxidean-runner failed; skipping further Actions smoke"
  fi
else
  smoke_require_or_skip "Docker daemon not available; runner Dockerfile checks already passed"
fi

if ! smoke_compose_api_running; then
  smoke_require_or_skip "Compose API not running; skipping live Actions health (run make up to exercise)"
fi

echo "==> wait for ${BASE_URL}/health"
ok=0
for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null "${BASE_URL}/health" 2>/dev/null; then
    ok=1
    break
  fi
  sleep 2
done
if [[ "$ok" -ne 1 ]]; then
  echo "health check failed at ${BASE_URL}/health — is the stack up? (make up)" >&2
  exit 1
fi

echo "==> probe /api/actions/register reachability (expect 401 for bogus token)"
code="$(
  curl -sS -o /dev/null -w '%{http_code}' \
    -X POST "${BASE_URL}/api/actions/register" \
    -H 'content-type: application/json' \
    -d '{"name":"smoke","labels":["ubuntu-latest"],"token":"reg_SMOKE_INVALID"}' \
    || true
)"
case "$code" in
  401|403) echo "OK: runner protocol path responded ${code}" ;;
  000)
    smoke_require_or_skip "could not reach ${BASE_URL}/api/actions/register; skipping protocol probe"
    ;;
  *)
    echo "unexpected HTTP ${code} from /api/actions/register (expected 401/403)" >&2
    exit 1
    ;;
esac

echo "==> Actions smoke OK (runner artifacts + protocol reachable at ${BASE_URL})"
exit 0
