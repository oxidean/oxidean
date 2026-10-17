#!/usr/bin/env bash
# e2e-act-runner.sh — stock Gitea `act_runner` compatibility e2e (Phase 5).
#
#   API (sqlite) + STOCK act_runner over the runner.v1 ConnectRPC surface
#   (/api/actions/runner.v1.RunnerService/*) → seeded workflow runs green.
#
# Runner sources, in order:
#   1. ACT_RUNNER_BIN=/path/to/act_runner
#   2. act_runner on PATH
#   3. `go install gitea.com/gitea/act_runner@<pin>` (needs go + network)
#   4. otherwise SKIP (exit 0) — set E2E_ACT_RUNNER_REQUIRED=1 to fail instead.
#
# Label modes:
#   ACT_RUNNER_HOST_MODE=1 → --labels "ubuntu-latest:host" (no docker needed;
#     act_runner executes run:/uses: node steps on the host).
#   default → docker labels ("ubuntu-latest") — requires a working docker
#     daemon; skipped without `docker info`.
#
# Usage: ./scripts/e2e-act-runner.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ACT_RUNNER_PIN="${ACT_RUNNER_PIN:-v0.2.11}"
API_PORT="${OXIDEAN_E2E_API_PORT:-18082}"
API_ORIGIN="http://127.0.0.1:${API_PORT}"
REG_TOKEN="${OXIDEAN_RUNNER_REGISTRATION_TOKEN:-e2e-dev-runner-token}"
WORK="$(mktemp -d)"
API_PID=""
DAEMON_PID=""

cleanup() {
  for pid in "$DAEMON_PID" "$API_PID"; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  if [[ "${E2E_KEEP_WORK:-0}" == "1" ]]; then
    echo "==> work dir kept at $WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: required command not found: $1" >&2; exit 1; }
}
need_cmd git; need_cmd curl; need_cmd jq
[[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]] || need_cmd cargo

skip() {
  echo "==> SKIP — $1" >&2
  [[ "${E2E_ACT_RUNNER_REQUIRED:-0}" == "1" ]] && exit 1
  exit 0
}

# --- act_runner discovery ----------------------------------------------------
ACT_RUNNER="${ACT_RUNNER_BIN:-}"
if [[ -z "$ACT_RUNNER" ]] && command -v act_runner >/dev/null 2>&1; then
  ACT_RUNNER="$(command -v act_runner)"
fi
if [[ -z "$ACT_RUNNER" ]] && command -v go >/dev/null 2>&1; then
  echo "==> building act_runner $ACT_RUNNER_PIN via go install"
  GOBIN="$WORK/bin" go install "gitea.com/gitea/act_runner@$ACT_RUNNER_PIN" \
    && ACT_RUNNER="$WORK/bin/act_runner"
fi
[[ -n "$ACT_RUNNER" ]] || skip "no act_runner — set ACT_RUNNER_BIN or install go"

# --- label mode / docker gate ------------------------------------------------
if [[ "${ACT_RUNNER_HOST_MODE:-0}" == "1" ]]; then
  LABELS="ubuntu-latest:host"
  echo "==> label mode: host (no docker required for run:/node steps)"
else
  LABELS="ubuntu-latest"
  docker info >/dev/null 2>&1 || skip "docker daemon unavailable (default labels run steps in containers)"
  echo "==> label mode: docker (default image schema)"
fi

# --- API ---------------------------------------------------------------------
if [[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]]; then
  [[ -x "$ROOT/target/debug/oxidean-api" ]] || {
    echo "error: prebuilt bin missing: target/debug/oxidean-api" >&2; exit 1; }
else
  echo "==> building oxidean-api"
  cargo build -q -p oxidean-api --bin oxidean-api
fi

export OXIDEAN_ENV=development
export API_BIND="127.0.0.1:${API_PORT}"
export DATABASE_URL="sqlite:${WORK}/oxidean.db"
export OXIDEAN_AUTO_MIGRATE=true
export OXIDEAN_PUBLIC_ORIGIN="$API_ORIGIN"
export OXIDEAN_CORS_ORIGINS="$API_ORIGIN"
export OXIDEAN_REPOS_DIR="${WORK}/repos"
export OXIDEAN_ACTIONS_ENABLED=true
export OXIDEAN_ACTIONS_LOG_DIR="${WORK}/actions-logs"
export OXIDEAN_ACTIONS_ARTIFACTS_DIR="${WORK}/actions-artifacts"
export OXIDEAN_ACTIONS_SECRETS_KEY="e2e-actions-secrets-key"
export OXIDEAN_RUNNER_REGISTRATION_TOKEN="$REG_TOKEN"
export OXIDEAN_ADMIN_EMAIL="admin@oxidean.local"
export OXIDEAN_ADMIN_PASSWORD="password1"
export RUST_LOG="${RUST_LOG:-info,oxidean=debug}"

"$ROOT/target/debug/oxidean-api" >"$WORK/api.log" 2>&1 &
API_PID=$!
for i in $(seq 1 90); do
  curl -4 -fsS "$API_ORIGIN/health" >/dev/null 2>&1 && break
  sleep 1
done

# --- act_runner register + daemon --------------------------------------------
mkdir -p "$WORK/runner"
cd "$WORK/runner"
# NB: config `runner.labels` overrides `--labels` at register time (and
# generate-config ships docker:// defaults) — always write our own config.
cat > config.yaml <<YAML
log:
  level: info
runner:
  file: .runner
  capacity: 1
  timeout: 3h
  labels:
    - "$LABELS"
cache:
  enabled: false
container:
  workdir_parent: "$WORK/runner/work"
host:
  workdir_parent: "$WORK/runner/host-work"
YAML

"$ACT_RUNNER" register \
  --no-interactive \
  --instance "$API_ORIGIN" \
  --token "$REG_TOKEN" \
  --name "e2e-act-runner" \
  --labels "$LABELS"

"$ACT_RUNNER" --config config.yaml daemon >"$WORK/daemon.log" 2>&1 &
DAEMON_PID=$!
sleep 2
kill -0 "$DAEMON_PID" 2>/dev/null || { echo "act_runner daemon exited early:"; cat "$WORK/daemon.log"; exit 1; }
cd "$ROOT"

# --- seed a run-only workflow -------------------------------------------------
# (no `uses:` — act_runner resolves `actions/*` against the instance, so a
# checkout step would need an actions/checkout repo to exist here)
echo "==> seeding run-only workflow"
JAR="$WORK/cookies"
rpc() {
  curl -sS -b "$JAR" -c "$JAR" -H 'Content-Type: application/json' \
    -H 'Oxidean-RPC-Version: 1' -d "{\"procedure\":\"$1\",\"input\":$2}" \
    "$API_ORIGIN/api/rpc"
}
rpc auth.login '{"identifier":"admin@oxidean.local","password":"password1","remember_me":false}' >/dev/null
OWNER="$(rpc auth.me '{}' | jq -r '.data.username')"
rpc repo.create '{"name":"act-e2e","visibility":"public"}' >/dev/null || true
PAT="$(rpc pat.createClassic "{\"name\":\"act-e2e-$(date +%s)\",\"scopes\":[\"repo\"]}")"
TOKEN="$(echo "$PAT" | jq -r '.data.token')"
CLONE="$WORK/repo"
git clone "http://oauth2:$TOKEN@127.0.0.1:${API_PORT}/$OWNER/act-e2e.git" "$CLONE" 2>/dev/null
cd "$CLONE"
git config user.email e2e@oxidean.local
git config user.name e2e
mkdir -p .github/workflows
cat > .github/workflows/ci.yml <<'YAML'
name: CI
on: [push]
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: env contract
        run: |
          test -n "$GITHUB_SHA" || { echo "GITHUB_SHA unset"; exit 1; }
          test -n "$GITHUB_REPOSITORY" || { echo "GITHUB_REPOSITORY unset"; exit 1; }
          echo "sha=$GITHUB_SHA repo=$GITHUB_REPOSITORY ref=$GITHUB_REF"
      - name: file commands
        run: |
          echo "FOO=bar" >> "$GITHUB_ENV"
          echo "step_out=baz" >> "$GITHUB_OUTPUT"
      - name: env propagated
        run: test "$FOO" = "bar"
      - name: marker
        run: echo "act_runner e2e OK"
YAML
git add -A
git commit -qm "e2e: act_runner workflow"
git push -q "http://oauth2:$TOKEN@127.0.0.1:${API_PORT}/$OWNER/act-e2e.git" HEAD:refs/heads/main
cd "$ROOT"

# --- wait for the run ----------------------------------------------------------
echo "==> waiting for run terminal state"
DEADLINE=$(( $(date +%s) + ${OXIDEAN_SEED_TIMEOUT_S:-180} ))
STATUS=""
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  RUNS="$(rpc repo.actions.listRuns "{\"owner\":\"$OWNER\",\"name\":\"act-e2e\",\"per_page\":1}" 2>/dev/null)"
  STATUS="$(echo "$RUNS" | jq -r '.data.runs[0].status // empty')"
  case "$STATUS" in
    success|failure|cancelled) break ;;
  esac
  sleep 2
done
if [ "$STATUS" != "success" ]; then
  echo "run did not succeed ($STATUS) — daemon log tail:" >&2
  tail -30 "$WORK/daemon.log" >&2 || true
  tail -20 "$WORK/api.log" >&2 || true
  exit 1
fi

echo "==> PASS — stock act_runner ran the workflow green via runner.v1"
