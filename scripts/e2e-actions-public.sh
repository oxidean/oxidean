#!/usr/bin/env bash
# e2e-actions-public.sh — Actions pipeline e2e against REAL github.com:
#   API (sqlite) + oxidean-runner (host mode) + a workflow that exercises the
#   public action ecosystem — actions/setup-node (real Node 20 download),
#   actions/cache (cache-service-absent warning path), artifact round-trip,
#   and actions/github-script calling core.getIDToken (RS256 OIDC mint).
#
# Skips (exit 0) when github.com/nodejs.org are unreachable, unless
# E2E_PUBLIC_ACTIONS_REQUIRED=1 (then unreachable is a hard failure).
#
# Usage: ./scripts/e2e-actions-public.sh
# Requires: git, curl, jq (and cargo unless OXIDEAN_PREBUILT_BINS=1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API_PORT="${OXIDEAN_E2E_API_PORT:-18083}"
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
[[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]] || need_cmd cargo

# ── reachability gate ─────────────────────────────────────────────────────
# setup-node's download host + the action-fetch fallback both need real
# egress; a missing either one is a skip, not a bug.
reachable() { curl -fsS -m 8 -o /dev/null "$1" >/dev/null 2>&1; }
UNREACHABLE=""
for host in https://github.com https://nodejs.org; do
  reachable "$host" || UNREACHABLE="$UNREACHABLE $host"
done
if [[ -n "$UNREACHABLE" ]]; then
  msg="SKIP — unreachable:$UNREACHABLE"
  if [[ "${E2E_PUBLIC_ACTIONS_REQUIRED:-0}" == "1" ]]; then
    echo "error: $msg (E2E_PUBLIC_ACTIONS_REQUIRED=1)" >&2
    exit 1
  fi
  echo "==> $msg"
  exit 0
fi
echo "==> github.com + nodejs.org reachable — running public-actions smoke"

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
  echo "==> using prebuilt oxidean-api + oxidean-runner"
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
export OXIDEAN_RUNNER_NAME="e2e-public-runner"
export OXIDEAN_RUNNER_LABELS="ubuntu-latest"
export OXIDEAN_RUNNER_STATE="${WORK}/runner.json"
export OXIDEAN_RUNNER_WORK_DIR="${WORK}/runner-work"
export OXIDEAN_RUNNER_POLL_MS=500

"$ROOT/target/debug/oxidean-runner" >"$WORK/runner.log" 2>&1 &
RUNNER_PID=$!
sleep 1
kill -0 "$RUNNER_PID" 2>/dev/null || { echo "runner exited early:"; cat "$WORK/runner.log"; exit 1; }

# ── seed repo + public-actions workflow ───────────────────────────────────
ORIGIN="$API_ORIGIN"
JAR="$WORK/cookies"
rpc() {
  local proc="$1" input="$2" res
  res="$(curl -sS -b "$JAR" -c "$JAR" \
    -H 'Content-Type: application/json' \
    -H 'Oxidean-RPC-Version: 1' \
    -d "{\"procedure\":\"$proc\",\"input\":$input}" \
    "$ORIGIN/api/rpc")"
  if [ "$(echo "$res" | jq -r '.ok // false')" != "true" ]; then
    echo "rpc $proc failed: $(echo "$res" | jq -c '.error // .')" >&2
    return 1
  fi
  echo "$res" | jq -c '.data'
}

echo "==> login + repo public-actions-smoke"
rpc auth.login '{"identifier":"admin@oxidean.local","password":"password1","remember_me":false}' >/dev/null
OWNER="$(rpc auth.me '{}' | jq -r '.username')"
rpc repo.create '{"name":"public-actions-smoke","visibility":"public"}' >/dev/null 2>&1 || true
TOKEN="$(rpc pat.createClassic "{\"name\":\"pub-smoke-$(date +%s)\",\"scopes\":[\"repo\"]}" | jq -r '.token')"
[ -n "$TOKEN" ] && [ "$TOKEN" != "null" ] || { echo "no token in pat response" >&2; exit 1; }

AUTH_ORIGIN="${ORIGIN/\/\//\/\/oauth2:$TOKEN@}"
CLONE="$WORK/repo"
git clone -q "$AUTH_ORIGIN/$OWNER/public-actions-smoke.git" "$CLONE" 2>/dev/null \
  || git clone -q "$ORIGIN/$OWNER/public-actions-smoke.git" "$CLONE"
cd "$CLONE"
git config user.email "seed@oxidean.local"
git config user.name "actions-seed"
echo "# public-actions-smoke" > README.md

mkdir -p .github/workflows
cat > .github/workflows/public.yml <<'YAML'
name: Public Actions smoke
on: [push]
jobs:
  fetch:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - name: node is v20
        run: |
          node --version
          node --version | grep -q '^v20\.' || { echo "node not v20"; exit 1; }
      - uses: actions/cache@v4
        with:
          path: .cache-key
          key: public-smoke-${{ github.sha }}
      - name: write payload
        run: |
          echo "cache-marker" > .cache-key
          mkdir -p dist
          echo "artifact-content-${{ github.sha }}" > dist/payload.txt
      - uses: actions/upload-artifact@v4
        with:
          name: public-smoke-payload
          path: dist/payload.txt
      - name: mint OIDC token via @actions/core
        uses: actions/github-script@v7
        with:
          script: |
            const token = await core.getIDToken('sts.amazonaws.com');
            const [h, p] = token.split('.');
            const header = JSON.parse(Buffer.from(h, 'base64url').toString());
            const claims = JSON.parse(Buffer.from(p, 'base64url').toString());
            console.log(`oidc header alg: ${header.alg}`);
            console.log(`oidc aud: ${claims.aud}`);
            console.log(`oidc iss: ${claims.iss}`);
            if (header.alg !== 'RS256') throw new Error(`expected RS256, got ${header.alg}`);
            if (claims.aud !== 'sts.amazonaws.com') throw new Error(`bad aud ${claims.aud}`);
  consume:
    needs: fetch
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: public-smoke-payload
          path: dl/
      - name: verify artifact
        run: |
          cat dl/payload.txt
          grep -q "artifact-content" dl/payload.txt || { echo "artifact payload mismatch"; exit 1; }
          echo "artifact round-trip OK"
YAML

date +%s > .seed-ts
git add -A
git commit -qm "seed: public actions smoke $(date +%s)"
git push -q "$AUTH_ORIGIN/$OWNER/public-actions-smoke.git" HEAD:refs/heads/main 2>/dev/null \
  || git push -q origin HEAD:refs/heads/main
cd "$ROOT"

echo "==> waiting for the run (real github.com fetches take a while)"
TIMEOUT="${OXIDEAN_SEED_TIMEOUT_S:-600}"
DEADLINE=$(( $(date +%s) + TIMEOUT ))
RUN_ID="" STATUS=""
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  RUNS="$(rpc repo.actions.listRuns '{"owner":"'"$OWNER"'","name":"public-actions-smoke","per_page":1}' 2>/dev/null || echo '{"runs":[]}')"
  RUN_ID="$(echo "$RUNS" | jq -r '.runs[0].id // empty')"
  STATUS="$(echo "$RUNS" | jq -r '.runs[0].status // empty')"
  case "$STATUS" in success|failure|cancelled) break ;; esac
  sleep 5
done

[ -n "$RUN_ID" ] || { echo "no run appeared within ${TIMEOUT}s" >&2; exit 1; }
echo "==> run $RUN_ID status: $STATUS  ($ORIGIN/$OWNER/public-actions-smoke/actions/$RUN_ID)"

DETAIL="$(rpc repo.actions.getRun '{"owner":"'"$OWNER"'","name":"public-actions-smoke","run_id":"'"$RUN_ID"'"}')"
LOG_DIR="${E2E_PUBLIC_LOG_DIR:-$WORK}"
mkdir -p "$LOG_DIR"
for JOB_ID in $(echo "$DETAIL" | jq -r '.jobs[].id'); do
  LOG="$(rpc repo.actions.getJobLog '{"owner":"'"$OWNER"'","name":"public-actions-smoke","run_id":"'"$RUN_ID"'","job_id":"'"$JOB_ID"'"}' | jq -r '.content // empty')"
  echo "==> job $JOB_ID log (tail):"
  echo "$LOG" | tail -n 15 | sed 's/^/    /'
  echo "$LOG" > "$LOG_DIR/log-$JOB_ID"
done
cp "$WORK/api.log" "$WORK/runner.log" "$LOG_DIR/" 2>/dev/null || true

ALL="$(cat "$LOG_DIR"/log-* 2>/dev/null || true)"
for marker in "oidc header alg: RS256" "oidc aud: sts.amazonaws.com" "artifact round-trip OK"; do
  echo "$ALL" | grep -q "$marker" || { echo "missing marker: $marker" >&2; exit 1; }
done
[ "$STATUS" = "success" ] || { echo "run did not succeed ($STATUS)" >&2; exit 1; }
echo "==> PASS — public-actions smoke green (checkout/setup-node/cache/artifacts/OIDC)"
