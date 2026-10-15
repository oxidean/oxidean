#!/usr/bin/env bash
# e2e-actions-pipeline.sh — full Actions pipeline e2e with NO Docker:
#   API (sqlite) + oxidean-runner (host mode) + seed demo workflow → green run.
#
# Usage: ./scripts/e2e-actions-pipeline.sh
# Requires: git, curl, jq (and cargo unless OXIDEAN_PREBUILT_BINS=1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API_PORT="${OXIDEAN_E2E_API_PORT:-18081}"
API_ORIGIN="http://127.0.0.1:${API_PORT}"
REG_TOKEN="${OXIDEAN_RUNNER_REGISTRATION_TOKEN:-e2e-dev-runner-token}"
WORK="$(mktemp -d)"
API_PID=""
RUNNER_PID=""

cleanup() {
  for pid in "$RUNNER_PID" "$API_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: required command not found: $1" >&2; exit 1; }
}
need_cmd git; need_cmd curl; need_cmd jq
# cargo is only needed when compiling bins — CI runs with
# OXIDEAN_PREBUILT_BINS=1 inside the tools image, which has no toolchain.
[[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]] || need_cmd cargo

wait_http() {
  local url="$1" name="$2" tries="${3:-60}"
  for ((i = 1; i <= tries; i++)); do
    if curl -4 -fsS "$url" >/dev/null 2>&1; then
      echo "==> $name ready ($url)"
      return 0
    fi
    sleep 1
  done
  echo "error: timed out waiting for $name at $url" >&2
  exit 1
}

if [[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]]; then
  echo "==> using prebuilt oxidean-api + oxidean-runner (rust-build artifact)"
  for bin in oxidean-api oxidean-runner; do
    [[ -x "$ROOT/target/debug/$bin" ]] || {
      echo "error: prebuilt bin missing: target/debug/$bin" >&2
      exit 1
    }
  done
else
  echo "==> building oxidean-api + oxidean-runner"
  cargo build -q -p oxidean-api --bin oxidean-api -p oxidean-runner --bin oxidean-runner
fi

echo "==> starting API on :$API_PORT (sqlite, actions enabled)"
export OXIDEAN_ENV=development
export API_BIND="127.0.0.1:${API_PORT}"
export DATABASE_URL="sqlite:${WORK}/oxidean.db"
export OXIDEAN_AUTO_MIGRATE=true
export OXIDEAN_PUBLIC_ORIGIN="$API_ORIGIN"
export OXIDEAN_CORS_ORIGINS="$API_ORIGIN"
export OXIDEAN_REPOS_DIR="${WORK}/repos"
export OXIDEAN_ACTIONS_ENABLED=true
export OXIDEAN_ACTIONS_LOG_DIR="${WORK}/actions-logs"
export OXIDEAN_ACTIONS_SECRETS_KEY="e2e-actions-secrets-key"
export OXIDEAN_RUNNER_REGISTRATION_TOKEN="$REG_TOKEN"
export OXIDEAN_ADMIN_EMAIL="admin@oxidean.local"
export OXIDEAN_ADMIN_PASSWORD="password1"
export RUST_LOG="${RUST_LOG:-info,oxidean=debug}"

"$ROOT/target/debug/oxidean-api" >"$WORK/api.log" 2>&1 &
API_PID=$!
wait_http "$API_ORIGIN/health" "oxidean-api" 90

echo "==> starting oxidean-runner (host mode)"
export OXIDEAN_PUBLIC_ORIGIN="$API_ORIGIN"
export OXIDEAN_RUNNER_NAME="e2e-runner"
export OXIDEAN_RUNNER_LABELS="ubuntu-latest"
export OXIDEAN_RUNNER_STATE="${WORK}/runner.json"
export OXIDEAN_RUNNER_WORK_DIR="${WORK}/runner-work"
export OXIDEAN_RUNNER_POLL_MS=500

"$ROOT/target/debug/oxidean-runner" >"$WORK/runner.log" 2>&1 &
RUNNER_PID=$!
sleep 1
kill -0 "$RUNNER_PID" 2>/dev/null || { echo "runner exited early:"; cat "$WORK/runner.log"; exit 1; }

echo "==> seeding ci-demo + pushing workflow"
OXIDEAN_ORIGIN="$API_ORIGIN" \
OXIDEAN_SEED_USER="admin@oxidean.local" \
OXIDEAN_SEED_PASSWORD="password1" \
OXIDEAN_SEED_TIMEOUT_S="${OXIDEAN_SEED_TIMEOUT_S:-120}" \
OXIDEAN_SEED_REQUIRE_MARKER=1 \
  ./scripts/dev-auth/seed-actions-demo.sh

echo "==> PASS — pipeline run green end to end"
