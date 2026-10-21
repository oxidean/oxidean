#!/usr/bin/env bash
# bench-actions.sh — Actions throughput/latency harness (Phase 9).
#
# Boots API (sqlite) + N oxidean-runners (host mode), seeds a repo with two
# workflows, and measures:
#   dispatch_to_first_claim_ms — workflow_dispatch → first job claimed
#   claims_per_s               — observed claim throughput (poll granularity)
#   log_ingest_mib_s           — update_log write throughput on a claimed job
#   list_runs_p95_ms           — repo.actions.listRuns p95
#
# Prints one JSON object to stdout and appends it to
# tmp/bench-actions-history.jsonl for phase-over-phase tracking.
#
# Usage: ./scripts/bench-actions.sh
# Env:
#   OXIDEAN_BENCH_JOBS        matrix legs in the benchmark run (default 30)
#   OXIDEAN_BENCH_RUNNERS     runner processes (default 2)
#   OXIDEAN_BENCH_LIST_ITERS  listRuns samples for the p95 (default 25)
#   OXIDEAN_BENCH_LOG_MIB     update_log payload in MiB (default 8)
#   OXIDEAN_E2E_API_PORT      API port (default 18091)
#   OXIDEAN_PREBUILT_BINS=1   use existing target/debug bins (no cargo)
# Requires: git, curl, jq (and cargo unless OXIDEAN_PREBUILT_BINS=1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API_PORT="${OXIDEAN_E2E_API_PORT:-18091}"
API_ORIGIN="http://127.0.0.1:${API_PORT}"
REG_TOKEN="${OXIDEAN_RUNNER_REGISTRATION_TOKEN:-bench-dev-runner-token}"
BENCH_JOBS="${OXIDEAN_BENCH_JOBS:-30}"
BENCH_RUNNERS="${OXIDEAN_BENCH_RUNNERS:-2}"
BENCH_LIST_ITERS="${OXIDEAN_BENCH_LIST_ITERS:-25}"
BENCH_LOG_MIB="${OXIDEAN_BENCH_LOG_MIB:-8}"
WORK="$(mktemp -d)"
API_PID=""
declare -a RUNNER_PIDS=()

cleanup() {
  for pid in "${RUNNER_PIDS[@]:-}" "$API_PID"; do
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
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

wait_http() {
  local url="$1" name="$2" tries="${3:-60}"
  for ((i = 1; i <= tries; i++)); do
    if curl -4 -fsS "$url" >/dev/null 2>&1; then
      echo "==> $name ready ($url)" >&2
      return 0
    fi
    sleep 1
  done
  echo "error: timed out waiting for $name at $url" >&2
  exit 1
}

now_ms() { date +%s%3N; }

if [[ "${OXIDEAN_PREBUILT_BINS:-0}" == "1" ]]; then
  echo "==> using prebuilt oxidean-api + oxidean-runner" >&2
  for bin in oxidean-api oxidean-runner; do
    [[ -x "$ROOT/target/debug/$bin" ]] || {
      echo "error: prebuilt bin missing: target/debug/$bin" >&2
      exit 1
    }
  done
else
  echo "==> building oxidean-api + oxidean-runner" >&2
  cargo build -q -p oxidean-api --bin oxidean-api -p oxidean-runner --bin oxidean-runner >&2
fi

echo "==> starting API on :$API_PORT (sqlite, actions enabled)" >&2
export OXIDEAN_ENV=development
export API_BIND="127.0.0.1:${API_PORT}"
export DATABASE_URL="sqlite:${WORK}/oxidean.db"
export OXIDEAN_AUTO_MIGRATE=true
export OXIDEAN_PUBLIC_ORIGIN="$API_ORIGIN"
export OXIDEAN_CORS_ORIGINS="$API_ORIGIN"
export OXIDEAN_REPOS_DIR="${WORK}/repos"
export OXIDEAN_ACTIONS_ENABLED=true
export OXIDEAN_ACTIONS_LOG_DIR="${WORK}/actions-logs"
export OXIDEAN_ACTIONS_SECRETS_KEY="bench-actions-secrets-key"
export OXIDEAN_RUNNER_REGISTRATION_TOKEN="$REG_TOKEN"
export OXIDEAN_ADMIN_EMAIL="admin@oxidean.local"
export OXIDEAN_ADMIN_PASSWORD="password1"
export RUST_LOG="${RUST_LOG:-warn}"

"$ROOT/target/debug/oxidean-api" >"$WORK/api.log" 2>&1 &
API_PID=$!
wait_http "$API_ORIGIN/health" "oxidean-api" 90

echo "==> starting $BENCH_RUNNERS oxidean-runner(s) (host mode)" >&2
for i in $(seq 1 "$BENCH_RUNNERS"); do
  (
    export OXIDEAN_PUBLIC_ORIGIN="$API_ORIGIN"
    export OXIDEAN_RUNNER_NAME="bench-runner-$i"
    export OXIDEAN_RUNNER_LABELS="ubuntu-latest"
    export OXIDEAN_RUNNER_STATE="${WORK}/runner-$i.json"
    export OXIDEAN_RUNNER_WORK_DIR="${WORK}/runner-work-$i"
    export OXIDEAN_RUNNER_POLL_MS=500
    exec "$ROOT/target/debug/oxidean-runner"
  ) >"$WORK/runner-$i.log" 2>&1 &
  RUNNER_PIDS+=("$!")
done
sleep 1
for pid in "${RUNNER_PIDS[@]}"; do
  kill -0 "$pid" 2>/dev/null || { echo "runner $pid exited early:" >&2; cat "$WORK"/runner-*.log >&2; exit 1; }
done

# --- Seed: login, repo, PAT, workflow push ---------------------------------

JAR="$WORK/cookies"
rpc() {
  local proc="$1" input="$2" res
  res="$(curl -sS -b "$JAR" -c "$JAR" \
    -H 'Content-Type: application/json' -H 'Oxidean-RPC-Version: 1' \
    -d "{\"procedure\":\"$proc\",\"input\":$input}" \
    "$API_ORIGIN/api/rpc")"
  if [ "$(echo "$res" | jq -r '.ok // false')" != "true" ]; then
    echo "rpc $proc failed: $(echo "$res" | jq -c '.error // .')" >&2
    return 1
  fi
  echo "$res" | jq -c '.data'
}

echo "==> login + seed bench repo" >&2
rpc auth.login '{"identifier":"admin@oxidean.local","password":"password1","remember_me":false}' >/dev/null
ME="$(rpc auth.me '{}')"
OWNER="$(echo "$ME" | jq -r '.username')"
REPO="actions-bench"
rpc repo.create "{\"name\":\"$REPO\",\"visibility\":\"public\"}" >/dev/null 2>&1 || true
PAT="$(rpc pat.createClassic "{\"name\":\"bench-$(date +%s)\",\"scopes\":[\"repo\"]}")"
TOKEN="$(echo "$PAT" | jq -r '.token')"

# Matrix job workflow — N claimable legs per dispatch.
LEGS="$(for i in $(seq 1 "$BENCH_JOBS"); do printf '"leg-%s", ' "$i"; done | sed 's/, $//')"
SEED_DIR="$WORK/seed"
mkdir -p "$SEED_DIR/.github/workflows"
cat > "$SEED_DIR/.github/workflows/bench.yml" <<YAML
name: bench
on: workflow_dispatch
jobs:
  leg:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        leg: [$LEGS]
    steps:
      - run: "true"
YAML
# Ingest workflow — a label no real runner declares; the bench claims it
# over the runner HTTP surface to drive update_log deterministically.
cat > "$SEED_DIR/.github/workflows/ingest.yml" <<YAML
name: bench-ingest
on: workflow_dispatch
jobs:
  sink:
    runs-on: bench-ingest
    timeout-minutes: 5
    steps:
      - run: sleep 300
YAML
cd "$SEED_DIR"
git init -q . && git add -A && git -c user.email=bench@oxidean -c user.name=bench commit -qm "seed bench workflows"
git remote add origin "http://oauth2:${TOKEN}@127.0.0.1:${API_PORT}/${OWNER}/${REPO}.git"
git push -q origin HEAD:main 2>/dev/null || git push -q origin HEAD:master
cd "$ROOT"
DEFAULT_REF="$(rpc repo.get "{\"owner\":\"$OWNER\",\"name\":\"$REPO\"}" | jq -r '.default_branch // "main"')"
echo "==> seeded $OWNER/$REPO (default $DEFAULT_REF)" >&2

# --- Metric 1+2: dispatch→claim latency + claim throughput -----------------

echo "==> dispatching bench.yml ($BENCH_JOBS legs)" >&2
T0="$(now_ms)"
RUN_ID="$(rpc repo.actions.dispatchWorkflow \
  "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"workflow_id\":\".github/workflows/bench.yml\",\"git_ref\":\"$DEFAULT_REF\"}" \
  | jq -r '.run_id')"
[[ -n "$RUN_ID" && "$RUN_ID" != "null" ]] || { echo "dispatch returned no run_id" >&2; exit 1; }

FIRST_CLAIM_MS=""
LAST_CLAIM_MS=""
CLAIMED=0
DEADLINE=$(( $(now_ms) + 300000 ))
while :; do
  JOBS_JSON="$(rpc repo.actions.getRun "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"run_id\":\"$RUN_ID\"}" | jq -c '.jobs')"
  CLAIMED="$(echo "$JOBS_JSON" | jq '[.[] | select(.status != "queued" and .status != "waiting")] | length')"
  NOW="$(now_ms)"
  if [[ "$CLAIMED" -gt 0 && -z "$FIRST_CLAIM_MS" ]]; then FIRST_CLAIM_MS="$NOW"; fi
  if [[ "$CLAIMED" -ge "$BENCH_JOBS" ]]; then LAST_CLAIM_MS="$NOW"; break; fi
  if [[ "$NOW" -gt "$DEADLINE" ]]; then
    echo "warn: only $CLAIMED/$BENCH_JOBS jobs claimed before deadline" >&2
    LAST_CLAIM_MS="$NOW"
    break
  fi
  sleep 0.1
done

D2C=$((FIRST_CLAIM_MS - T0))
SPAN=$((LAST_CLAIM_MS - FIRST_CLAIM_MS))
[[ "$SPAN" -lt 1 ]] && SPAN=1
CLAIMS_PER_S="$(jq -n --arg n "$CLAIMED" --arg ms "$SPAN" '($n|tonumber) * 1000.0 / ($ms|tonumber)')"

# --- Metric 3: log ingest throughput ---------------------------------------
# A dedicated runner identity (never a process) claims a `bench-ingest` job;
# the bench then drives update_log directly — a deterministic measure of the
# ingest path (CAS + bounded append + heartbeat) without exec noise.

echo "==> measuring log ingest ($BENCH_LOG_MIB MiB)" >&2
INGEST_REG="$(curl -sS -X POST "$API_ORIGIN/api/actions/register" \
  -H 'Content-Type: application/json' \
  -d "{\"name\":\"bench-ingest\",\"labels\":[\"bench-ingest\"],\"token\":\"$REG_TOKEN\"}")"
INGEST_TOKEN="$(echo "$INGEST_REG" | jq -r '.runner_token')"
[[ -n "$INGEST_TOKEN" && "$INGEST_TOKEN" != "null" ]] || { echo "ingest runner register failed: $INGEST_REG" >&2; exit 1; }

INGEST_RUN_ID="$(rpc repo.actions.dispatchWorkflow \
  "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"workflow_id\":\".github/workflows/ingest.yml\",\"git_ref\":\"$DEFAULT_REF\"}" \
  | jq -r '.run_id')"
INGEST_JOB=""
for _ in $(seq 1 50); do
  TASK="$(curl -sS -X POST "$API_ORIGIN/api/actions/fetch_task" \
    -H "Authorization: Bearer $INGEST_TOKEN" -H 'Content-Type: application/json' \
    -d '{"wait_ms":0}')"
  INGEST_JOB="$(echo "$TASK" | jq -r '.job_id // empty')"
  [[ -n "$INGEST_JOB" ]] && break
  sleep 0.2
done
[[ -n "$INGEST_JOB" ]] || { echo "ingest claim failed" >&2; exit 1; }

# The chunk is a run of 'x' + newline — JSON-trivial, so the escaped body is
# built once and only log_index changes per request (a 256 KiB --arg would
# blow curl/jq's argv limits).
CHUNK_BYTES=262144
CHUNK_ESC="$(head -c 262143 /dev/zero | tr '\0' 'x')\\n"
BODY_HEAD="{\"run_id\":\"$INGEST_RUN_ID\",\"job_id\":\"$INGEST_JOB\",\"chunk\":\"$CHUNK_ESC\",\"log_index\":"
LOG_INDEX=0
INGEST_T0="$(now_ms)"
SENT=0
TARGET_BYTES=$((BENCH_LOG_MIB * 1024 * 1024))
while [[ $SENT -lt $TARGET_BYTES ]]; do
  ACK="$(printf '%s%d}' "$BODY_HEAD" "$LOG_INDEX" \
    | curl -sS -X POST "$API_ORIGIN/api/actions/update_log" \
      -H "Authorization: Bearer $INGEST_TOKEN" -H 'Content-Type: application/json' \
      --data-binary @-)"
  LOG_INDEX="$(echo "$ACK" | jq -r '.log_index // '"$LOG_INDEX")"
  SENT=$((SENT + CHUNK_BYTES))
done
INGEST_MS=$(( $(now_ms) - INGEST_T0 ))
[[ "$INGEST_MS" -lt 1 ]] && INGEST_MS=1
LOG_MIB_S="$(jq -n --arg b "$SENT" --arg ms "$INGEST_MS" '($b|tonumber) / 1048576.0 * 1000.0 / ($ms|tonumber)')"
# Retire the claimed job so the run doesn't sit in_progress.
curl -sS -X POST "$API_ORIGIN/api/actions/update_task" \
  -H "Authorization: Bearer $INGEST_TOKEN" -H 'Content-Type: application/json' \
  -d "{\"job_id\":\"$INGEST_JOB\",\"state\":\"failure\"}" >/dev/null

# --- Metric 4: listRuns p95 -------------------------------------------------

echo "==> sampling listRuns ($BENCH_LIST_ITERS iters)" >&2
LAT=()
for _ in $(seq 1 "$BENCH_LIST_ITERS"); do
  S="$(now_ms)"
  rpc repo.actions.listRuns "{\"owner\":\"$OWNER\",\"name\":\"$REPO\",\"per_page\":25}" >/dev/null
  LAT+=("$(( $(now_ms) - S ))")
done
P95="$(printf '%s\n' "${LAT[@]}" | sort -n | awk -v n="$BENCH_LIST_ITERS" '{a[NR]=$1} END {i=int((95*n+99)/100); print a[i]}')"

# --- Report -----------------------------------------------------------------

RESULT="$(jq -n \
  --arg ts "$(date -u +%FT%TZ)" \
  --arg sha "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" \
  --argjson jobs "$BENCH_JOBS" \
  --argjson runners "$BENCH_RUNNERS" \
  --argjson d2c "$D2C" \
  --argjson claims "$CLAIMS_PER_S" \
  --argjson ingest "$LOG_MIB_S" \
  --argjson p95 "$P95" \
  '{ts:$ts, commit:$sha, jobs:$jobs, runners:$runners,
    dispatch_to_first_claim_ms:$d2c, claims_per_s:$claims,
    log_ingest_mib_s:$ingest, list_runs_p95_ms:$p95}')"

mkdir -p "$ROOT/tmp"
echo "$RESULT" >> "$ROOT/tmp/bench-actions-history.jsonl"
echo "$RESULT" | jq .
