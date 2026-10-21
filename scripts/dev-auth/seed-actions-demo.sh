#!/usr/bin/env bash
# seed-actions-demo.sh — seed a ci-demo repo + workflow, push it, and poll the
# Actions run to a terminal state. Prints the run URL and a job-log tail.
#
# Works against any reachable Oxidean origin (local compose, Railway
# preview/staging). Requires curl, git, jq.
#
# Usage:
#   ./scripts/dev-auth/seed-actions-demo.sh
#   OXIDEAN_ORIGIN=https://gateway-oxidean-pr-39.up.railway.app \
#     OXIDEAN_SEED_USER=you OXIDEAN_SEED_PASSWORD=secret \
#     ./scripts/dev-auth/seed-actions-demo.sh
#
# Env:
#   OXIDEAN_ORIGIN           default http://localhost
#   OXIDEAN_SEED_USER        login identifier (default admin@oxidean.local)
#   OXIDEAN_SEED_PASSWORD    login password   (default password1 — dev stack)
#   OXIDEAN_SEED_OWNER       owner slug for the repo (default: session username)
#   OXIDEAN_SEED_REPO        repo name (default ci-demo)
#   OXIDEAN_SEED_TIMEOUT_S   max seconds to wait for the run (default 180)

set -euo pipefail

ORIGIN="${OXIDEAN_ORIGIN:-http://localhost}"
ORIGIN="${ORIGIN%/}"
USER_ID="${OXIDEAN_SEED_USER:-admin@oxidean.local}"
PASS="${OXIDEAN_SEED_PASSWORD:-password1}"
REPO="${OXIDEAN_SEED_REPO:-ci-demo}"
TIMEOUT="${OXIDEAN_SEED_TIMEOUT_S:-180}"

for bin in curl git jq; do
  command -v "$bin" >/dev/null 2>&1 || { echo "missing dependency: $bin" >&2; exit 2; }
done

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
JAR="$TMP/cookies"

rpc() {
  # rpc <procedure> <input-json> → prints .data on ok, dies on error
  local proc="$1" input="$2" res
  res="$(rpc_allow "$proc" "$input")"
  if [ "$(echo "$res" | jq -r '.ok // false')" != "true" ]; then
    echo "rpc $proc failed: $(echo "$res" | jq -c '.error // .')" >&2
    return 1
  fi
  echo "$res" | jq -c '.data'
}

rpc_allow() {
  # rpc_allow <procedure> <input-json> → prints full body (no -f: error bodies
  # are inspectable JSON)
  local proc="$1" input="$2"
  curl -sS -b "$JAR" -c "$JAR" \
    -H 'Content-Type: application/json' \
    -H 'Oxidean-RPC-Version: 1' \
    -d "{\"procedure\":\"$proc\",\"input\":$input}" \
    "$ORIGIN/api/rpc"
}

echo "==> login $USER_ID @ $ORIGIN"
rpc auth.login "{\"identifier\":\"$USER_ID\",\"password\":\"$PASS\",\"remember_me\":false}" >/dev/null

ME="$(rpc auth.me '{}')"
OWNER="${OXIDEAN_SEED_OWNER:-$(echo "$ME" | jq -r '.username // empty')}"
[ -n "$OWNER" ] || { echo "could not resolve session username" >&2; exit 1; }
echo "==> owner: $OWNER"

echo "==> ensure repo $OWNER/$REPO (public)"
CREATE_RES="$(rpc_allow repo.create "{\"name\":\"$REPO\",\"visibility\":\"public\"}")"
if [ "$(echo "$CREATE_RES" | jq -r '.ok')" != "true" ]; then
  ERR="$(echo "$CREATE_RES" | jq -r '.error.code // .error.message // ""')"
  echo "   repo.create returned $ERR — continuing (repo may already exist)"
fi

echo "==> mint classic PAT (repo scope)"
PAT="$(rpc pat.createClassic "{\"name\":\"actions-seed-$(date +%s)\",\"scopes\":[\"repo\"]}")"
TOKEN="$(echo "$PAT" | jq -r '.token')"
[ -n "$TOKEN" ] && [ "$TOKEN" != "null" ] || { echo "no token in pat response" >&2; exit 1; }

echo "==> push workflow to $ORIGIN/$OWNER/$REPO.git"
# Insert PAT basic-auth after the scheme (works for http and https origins).
# `oauth2` is one of the accepted username aliases for PAT auth.
AUTH_ORIGIN="${ORIGIN/\/\//\/\/oauth2:$TOKEN@}"
CLONE="$TMP/repo"
git clone "$AUTH_ORIGIN/$OWNER/$REPO.git" "$CLONE" 2>/dev/null \
  || git clone "$ORIGIN/$OWNER/$REPO.git" "$CLONE"
cd "$CLONE"
git config user.email "seed@oxidean.local"
git config user.name "actions-seed"

mkdir -p .github/workflows
cat > .github/workflows/ci.yml <<'YAML'
name: CI
on: [push, workflow_dispatch]
permissions:
  contents: read
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: marker
        run: |
          echo "oxidean-hello from actions"
          test -f README.md || { echo "workspace missing README.md"; exit 1; }
          test -n "$GITHUB_SHA" || { echo "GITHUB_SHA unset"; exit 1; }
          echo "checkout ok at $GITHUB_SHA"
      - name: oxidean env contract
        run: |
          echo "sha=$OXIDEAN_SHA alias=$GITHUB_SHA repo=$OXIDEAN_REPOSITORY ref=$OXIDEAN_REF_NAME"
          test -n "$OXIDEAN_TOKEN" || { echo "OXIDEAN_TOKEN unset"; exit 1; }
          test "$OXIDEAN_SHA" = "$GITHUB_SHA" || { echo "sha alias mismatch"; exit 1; }
      - name: env file write
        run: echo "FOO=bar" >> "$OXIDEAN_ENV_FILE"
      - name: env file read next step
        run: test "$FOO" = "bar" || { echo "OXIDEAN_ENV_FILE propagation broken"; exit 1; }
      - name: job token authenticated read
        run: |
          resp="$(curl -fsS -H "Authorization: Bearer $OXIDEAN_TOKEN" "$OXIDEAN_API_URL/repos/$OXIDEAN_REPOSITORY")"
          test "$(echo "$resp" | jq -r '.name')" = "${OXIDEAN_REPOSITORY##*/}" \
            && echo "job token read OK"
YAML
[ -f README.md ] || echo "# $REPO" > README.md
# Always produce a fresh commit so re-runs trigger a new Actions run.
date +%s > .seed-ts
git add -A
git commit -qm "seed: ci workflow $(date +%s)"
HEAD_SHA="$(git rev-parse HEAD)"
git push -q "$AUTH_ORIGIN/$OWNER/$REPO.git" HEAD:refs/heads/main 2>/dev/null \
  || git push -q origin HEAD:refs/heads/main
cd - >/dev/null

echo "==> wait for run to reach a terminal state (timeout ${TIMEOUT}s)"
DEADLINE=$(( $(date +%s) + TIMEOUT ))
RUN_ID="" STATUS=""
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  RUNS="$(rpc repo.actions.listRuns "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"per_page\":1}" 2>/dev/null || echo '{"runs":[]}')"
  RUN_ID="$(echo "$RUNS" | jq -r '.runs[0].id // empty')"
  STATUS="$(echo "$RUNS" | jq -r '.runs[0].status // empty')"
  case "$STATUS" in
    success|failure|cancelled) break ;;
  esac
  sleep 2
done

[ -n "$RUN_ID" ] || { echo "no run appeared within ${TIMEOUT}s" >&2; exit 1; }
echo "==> run $RUN_ID status: $STATUS"
echo "==> $ORIGIN/$OWNER/$REPO/actions/$RUN_ID"

DETAIL="$(rpc repo.actions.getRun "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"run_id\":\"$RUN_ID\"}")"
JOB_ID="$(echo "$DETAIL" | jq -r '.jobs[0].id // empty')"
if [ -n "$JOB_ID" ]; then
  LOG="$(rpc repo.actions.getJobLog "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"run_id\":\"$RUN_ID\",\"job_id\":\"$JOB_ID\"}" | jq -r '.content // empty')"
  echo "==> job $JOB_ID log (tail):"
  echo "$LOG" | tail -n 20 | sed 's/^/    /'
  if [ "$STATUS" = "success" ] && ! echo "$LOG" | grep -q "oxidean-hello"; then
    if [ "${OXIDEAN_SEED_REQUIRE_MARKER:-0}" = "1" ]; then
      echo "log lacks the oxidean-hello marker" >&2
      exit 1
    fi
    echo "WARN: run succeeded but log lacks the oxidean-hello marker"
  fi
  # OXIDEAN_* env contract + job-token authenticated read must have run.
  if [ "$STATUS" = "success" ]; then
    echo "$LOG" | grep -q "sha=$HEAD_SHA" || {
      echo "log lacks the pushed head sha marker (sha=$HEAD_SHA)" >&2
      exit 1
    }
    echo "$LOG" | grep -q "job token read OK" || {
      echo "log lacks the OXIDEAN_TOKEN authenticated-read marker" >&2
      exit 1
    }
  fi
fi

[ "$STATUS" = "success" ] || { echo "run did not succeed ($STATUS)" >&2; exit 1; }
echo "==> done — run green"
