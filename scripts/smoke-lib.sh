#!/usr/bin/env bash
# Shared helpers for forge protocol smoke scripts (D-QH-04 / T-11.1-40).
# Local operator runs may skip when Docker/stack is absent (exit 0).
# CI=true or SMOKE_REQUIRE_STACK=1 must fail closed — never greenwash.

smoke_require_or_skip() {
  local msg="$1"
  if [[ "${CI:-}" == "true" || "${SMOKE_REQUIRE_STACK:-0}" == "1" ]]; then
    echo "FAIL: ${msg} (CI/SMOKE_REQUIRE_STACK fails closed — no skip-as-pass)" >&2
    exit 1
  fi
  echo "${msg}"
  exit 0
}

smoke_require_docker() {
  local name="${SMOKE_NAME:-smoke}"
  if ! command -v docker >/dev/null 2>&1; then
    smoke_require_or_skip "docker not found on PATH; skipping ${name} (operator without Compose)"
  fi
  if ! docker info >/dev/null 2>&1; then
    smoke_require_or_skip "docker engine not reachable; skipping ${name}"
  fi
}

# Retries the `compose ps` fast-skip probe — a single invocation can return
# empty on a transient daemon/CLI hiccup while the stack is serving, which
# would fail CI closed for the wrong reason.
smoke_compose_api_running() {
  local file="${1:-docker-compose.yml}"
  local i
  for i in $(seq 1 10); do
    if docker compose -f "$file" ps --status running 2>/dev/null | grep -qE 'api|oxidean-api'; then
      return 0
    fi
    sleep 2
  done
  return 1
}
