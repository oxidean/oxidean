#!/usr/bin/env bash
# bench-web-edge.sh — edge-lane benchmark for issue #209.
#
# Compares the two web-tier topologies on the same machine, sequentially:
#   baseline — stock docker-compose.yml: traefik + web (oxidean-web) + api + postgres
#   watt     — docker-compose.watt.yml overlay: watt (gateway+web workers) + api + postgres
#
# Each lane is seeded identically (one env-seeded admin + one public repo),
# then loaded with oha across the endpoint classes from the issue:
#   static   GET /_astro/*                   — raw static throughput
#   shells   GET /, /login, /{o}/{r}         — dispatch + injection + anon gate path
#   rpc      POST /api/rpc                   — edge → api proxy path
#   git      GET /{o}/{r}.git/info/refs      — edge pattern routing (not prefix)
#
# Per lane it records: p50/p95/p99 latency, max req/s, error rate (medians of
# N runs per endpoint), edge container RSS/CPU sampled via docker stats during
# load, time-to-healthy from compose up, and image sizes. Results print as
# JSON and append to tmp/bench-web-edge-history.jsonl.
#
# Usage: ./scripts/bench-web-edge.sh
# Env:
#   OXIDEAN_BENCH_LANE        baseline | watt | both (default both)
#   OXIDEAN_BENCH_DURATION    oha -z duration per run (default 15s)
#   OXIDEAN_BENCH_CONN        oha -c connections (default 50)
#   OXIDEAN_BENCH_RUNS        runs per endpoint, median taken (default 3)
#   OXIDEAN_BENCH_PROJECT     compose project name (default oxidean-bench)
#   OXIDEAN_ADMIN_EMAIL/PASSWORD  seeded admin creds (defaults match overlay)
# Requires: docker (compose v2), git, curl, jq, oha.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LANE="${OXIDEAN_BENCH_LANE:-both}"
DUR="${OXIDEAN_BENCH_DURATION:-15s}"
CONN="${OXIDEAN_BENCH_CONN:-50}"
RUNS="${OXIDEAN_BENCH_RUNS:-3}"
PROJECT="${OXIDEAN_BENCH_PROJECT:-oxidean-bench}"
# Traefik routers all match Host(`localhost`) — the origin must send that
# Host header or every baseline request 404s. Watt's gateway is host-agnostic.
ORIGIN="http://localhost"
ADMIN_EMAIL="${OXIDEAN_ADMIN_EMAIL:-bench-admin@oxidean.local}"
ADMIN_PASSWORD="${OXIDEAN_ADMIN_PASSWORD:-password1}"
COMPOSE=(docker compose -f docker-compose.yml -f docker-compose.watt.yml -p "$PROJECT")
WORK="$(mktemp -d)"

cleanup() {
  "${COMPOSE[@]}" down -v --remove-orphans >/dev/null 2>&1 || true
  rm -rf "$WORK"
}
trap cleanup EXIT

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "error: required command not found: $1" >&2; exit 1; }
}
need_cmd docker; need_cmd git; need_cmd curl; need_cmd jq; need_cmd oha

now_ms() { date +%s%3N; }

wait_origin() {
  local url="$1" name="$2" tries="${3:-120}"
  for ((i = 1; i <= tries; i++)); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      echo "==> $name ready ($url)" >&2
      return 0
    fi
    sleep 1
  done
  echo "error: timed out waiting for $name at $url" >&2
  return 1
}

# --- Seed (identical per lane) ----------------------------------------------

JAR="$WORK/cookies"
rpc() {
  local proc="$1" input="$2" res
  res="$(curl -sS -b "$JAR" -c "$JAR" \
    -H 'Content-Type: application/json' -H 'Oxidean-RPC-Version: 1' \
    -d "{\"procedure\":\"$proc\",\"input\":$input}" \
    "$ORIGIN/api/rpc")"
  if [ "$(echo "$res" | jq -r '.ok // false')" != "true" ]; then
    echo "rpc $proc failed: $(echo "$res" | jq -c '.error // .')" >&2
    return 1
  fi
  echo "$res" | jq -c '.data'
}

seed_lane() {
  echo "==> login + seed bench repo" >&2
  rm -f "$JAR"
  rpc auth.login "{\"identifier\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\",\"remember_me\":false}" >/dev/null
  local me owner pat token seed_dir repo
  me="$(rpc auth.me '{}')"
  owner="$(echo "$me" | jq -r '.username')"
  repo="edge-bench"
  rpc repo.create "{\"name\":\"$repo\",\"visibility\":\"public\"}" >/dev/null 2>&1 || true
  pat="$(rpc pat.createClassic "{\"name\":\"bench-$(date +%s)\",\"scopes\":[\"repo\"]}")"
  token="$(echo "$pat" | jq -r '.token')"

  seed_dir="$WORK/seed-$owner-$repo"
  mkdir -p "$seed_dir"
  printf '# edge-bench\n' > "$seed_dir/README.md"
  git -C "$seed_dir" init -q .
  git -C "$seed_dir" add -A
  git -C "$seed_dir" -c user.email=bench@oxidean -c user.name=bench commit -qm seed
  git -C "$seed_dir" remote add origin "http://oauth2:${token}@localhost/${owner}/${repo}.git"
  git -C "$seed_dir" push -qf origin HEAD:main 2>/dev/null \
    || git -C "$seed_dir" push -qf --force origin HEAD:main 2>/dev/null \
    || git -C "$seed_dir" push -qf origin HEAD:master
  echo "$owner $repo"
}

# --- docker stats sampler ----------------------------------------------------
# Samples `docker stats --no-stream --format json` for the project's edge
# containers every ~2s while a run is in flight; aggregates max RSS and
# avg CPU% afterwards.

STATS_FILE=""
stats_begin() {
  STATS_FILE="$WORK/stats-$1.jsonl"
  : > "$STATS_FILE"
  (
    round=0
    while :; do
      ids=$("${COMPOSE[@]}" ps -q 2>/dev/null | tr '\n' ' ')
      if [[ -n "${ids// /}" ]]; then
        # r tags the sampling round so per-container readings from one
        # `docker stats` call can be summed into an interval total below.
        docker stats --no-stream \
          --format "{\"r\":$round,\"n\":\"{{.Name}}\",\"m\":\"{{.MemUsage}}\",\"c\":\"{{.CPUPerc}}\"}" \
          $ids >> "$STATS_FILE" 2>/dev/null || true
      fi
      round=$((round + 1))
      sleep 2
    done
  ) &
  STATS_PID=$!
}
stats_end() {
  kill "$STATS_PID" 2>/dev/null || true
  wait "$STATS_PID" 2>/dev/null || true
}

# $1 = lane (baseline|watt) → edge container name filter (traefik+web vs watt)
# Aggregates the lane's edge containers per sampling round first (the watt
# lane is one container, the baseline two) so the metrics compare like for
# like: peak total RSS and mean total CPU% across the edge tier.
edge_stats_json() {
  local lane="$1" names
  if [[ "$lane" == "watt" ]]; then
    names='[-_]watt[-_][0-9]+$'
  else
    names='[-_](traefik|web)[-_][0-9]+$'
  fi
  jq -Rn --arg names "$names" '
    def mib: (if test("GiB") then (sub("GiB.*";"")|tonumber)*1073741824
              elif test("MiB") then (sub("MiB.*";"")|tonumber)*1048576
              elif test("KiB") then (sub("KiB.*";"")|tonumber)*1024
              else (sub("B.*";"")|tonumber) end);
    [inputs | fromjson? | select(.n | test($names))] as $s
    | if ($s|length) == 0 then {containers:0} else
      ($s | group_by(.r)) as $rounds
      | { containers: ($s | map(.n) | unique | length),
          edge_rss_bytes_max: ($rounds | map(map(.m | split(" / ")[0] | mib) | add) | max),
          edge_cpu_pct_avg:   ($rounds | map(map(.c | rtrimstr("%") | tonumber? // 0) | add) | add / length) }
      end' < "$STATS_FILE"
}

# --- oha runner --------------------------------------------------------------
# $1 label, $2 url, extra oha args on $3+; prints one raw oha JSON doc.
# oha takes a single positional URL — endpoint classes with several URLs run
# one bench per URL (see bench_lane).

oha_run() {
  local label="$1" url="$2" out
  shift 2
  out="$(oha --no-tui --output-format json -z "$DUR" -c "$CONN" "$@" "$url" \
        2>>"$WORK/oha-$label.log" || true)"
  if [[ -z "$out" ]] || ! echo "$out" | jq -e . >/dev/null 2>&1; then
    echo "warn: oha $label produced no JSON (see $WORK/oha-$label.log)" >&2
    echo '{}'
    return 0
  fi
  echo "$out"
}

# Extract the fields we report from an oha JSON doc.
oha_fields() {
  jq -c '{
    p50_ms:   ((.latencyPercentiles.p50  // .latency_percentiles.p50  // .p50)  * 1000),
    p95_ms:   ((.latencyPercentiles.p95  // .latency_percentiles.p95  // .p95)  * 1000),
    p99_ms:   ((.latencyPercentiles.p99  // .latency_percentiles.p99  // .p99)  * 1000),
    req_per_s: (.summary.requestsPerSec // .summary.requests_per_sec // .requestsPerSec),
    error_rate: (1 - (.summary.successRate // .summary.success_rate // 1)),
    errors: (.errorDistribution // .error_distribution // {})
  }'
}

median() { # reads a JSON array of numbers on stdin
  jq 'sort | if length == 0 then null else .[((length - 1) / 2 | floor)] end'
}

# Run one endpoint RUNS times; emit the median aggregate + run details.
# $1 label, $2 url, $3 lane, extra oha args on $4+
bench_endpoint() {
  local label="$1" url="$2" lane="$3"
  shift 3
  local -a p50s=() p95s=() p99s=() rpss=() errs=()
  local -a details=()
  for run in $(seq 1 "$RUNS"); do
    echo "==> $lane/$label run $run/$RUNS (${DUR}, c=$CONN)" >&2
    stats_begin "$lane-$label-$run"
    local j
    j="$(oha_run "$lane-$label-$run" "$url" "$@")"
    stats_end
    local f
    f="$(echo "$j" | oha_fields)"
    p50s+=("$(echo "$f" | jq '.p50_ms')")
    p95s+=("$(echo "$f" | jq '.p95_ms')")
    p99s+=("$(echo "$f" | jq '.p99_ms')")
    rpss+=("$(echo "$f" | jq '.req_per_s')")
    errs+=("$(echo "$f" | jq '.error_rate')")
    details+=("$(jq -nc --argjson f "$f" --argjson s "$(edge_stats_json "$lane")" \
      '{run:'"$run"', metrics:$f, stats:$s}')")
  done
  jq -nc --arg url "$url" \
    --argjson p50 "$(printf '%s\n' "${p50s[@]}" | jq -s . | median)" \
    --argjson p95 "$(printf '%s\n' "${p95s[@]}" | jq -s . | median)" \
    --argjson p99 "$(printf '%s\n' "${p99s[@]}" | jq -s . | median)" \
    --argjson rps "$(printf '%s\n' "${rpss[@]}" | jq -s . | median)" \
    --argjson err "$(printf '%s\n' "${errs[@]}" | jq -s . | median)" \
    --argjson runs "[$(IFS=,; echo "${details[*]}")]" \
    '{url:$url, p50_ms:$p50, p95_ms:$p95, p99_ms:$p99, req_per_s:$rps, error_rate:$err, runs:$runs}'
}

# --- Lane driver --------------------------------------------------------------

bench_lane() {
  local lane="$1"
  local services edge_services
  if [[ "$lane" == "watt" ]]; then
    services="postgres api watt"
    edge_services="watt"
  else
    services="postgres api web traefik"
    edge_services="traefik web"
  fi

  echo "==> lane $lane: compose build ($services)" >&2
  local bsvc bimg
  for bsvc in $services; do
    bimg="$("${COMPOSE[@]}" config --format json 2>/dev/null | jq -r ".services.\"$bsvc\".image // empty")"
    if [[ -z "$bimg" ]] || ! docker image inspect "$bimg" >/dev/null 2>&1; then
      "${COMPOSE[@]}" build "$bsvc" >&2
    else
      echo "    $bsvc: image $bimg present, skipping build" >&2
    fi
  done

  echo "==> lane $lane: compose up ($services)" >&2
  local t0 healthy_ms
  t0="$(now_ms)"
  "${COMPOSE[@]}" up -d $services >&2
  wait_origin "$ORIGIN/" "$lane public origin" 180
  healthy_ms=$(( $(now_ms) - t0 ))

  local seed owner repo
  seed="$(seed_lane)"
  owner="$(echo "$seed" | cut -d' ' -f1)"
  repo="$(echo "$seed" | cut -d' ' -f2)"

  # A real hashed asset for the static class — read it off a served shell so
  # the URL is guaranteed to exist in this build's dist.
  local asset
  asset="$(curl -fsS "$ORIGIN/login" | grep -oE '/_astro/[^"]+\.(js|css)' | head -1 || true)"
  [[ -n "$asset" ]] || asset="/favicon.ico"

  local res_static res_index res_login res_repo res_rpc res_git
  res_static="$(bench_endpoint static "$ORIGIN$asset" "$lane")"
  res_index="$(bench_endpoint index "$ORIGIN/" "$lane")"
  res_login="$(bench_endpoint login "$ORIGIN/login" "$lane")"
  res_repo="$(bench_endpoint repo "$ORIGIN/$owner/$repo" "$lane")"
  res_rpc="$(bench_endpoint rpc "$ORIGIN/api/rpc" "$lane" \
    -m POST -H 'Content-Type: application/json' -H 'Oxidean-RPC-Version: 1' \
    -d '{"procedure":"auth.bootstrap_status","input":{}}')"
  res_git="$(bench_endpoint git "$ORIGIN/$owner/$repo.git/info/refs?service=git-upload-pack" "$lane")"

  # Image sizes for the edge services of this lane.
  local sizes="{}"
  for svc in $edge_services; do
    local img size
    img="$("${COMPOSE[@]}" images "$svc" 2>/dev/null | awk 'NR>1 {print $2":"$3}' | head -1)"
    size="$(docker image inspect "$img" --format '{{.Size}}' 2>/dev/null || echo null)"
    sizes="$(echo "$sizes" | jq --arg s "$svc" --argjson z "${size:-null}" '. + {($s):$z}')"
  done

  jq -nc \
    --arg ts "$(date -u +%FT%TZ)" \
    --arg sha "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)" \
    --arg lane "$lane" \
    --argjson healthy_ms "$healthy_ms" \
    --arg dur "$DUR" \
    --argjson conn "$CONN" \
    --argjson runs "$RUNS" \
    --argjson static "$res_static" \
    --argjson index "$res_index" \
    --argjson login "$res_login" \
    --argjson repo "$res_repo" \
    --argjson rpc "$res_rpc" \
    --argjson git "$res_git" \
    --argjson image_bytes "$sizes" \
    '{ts:$ts, commit:$sha, lane:$lane, time_to_healthy_ms:$healthy_ms,
      oha:{duration:$dur, connections:$conn, runs_per_endpoint:$runs},
      classes:{static:$static, index:$index, login:$login, repo:$repo, rpc:$rpc, git:$git},
      image_bytes:$image_bytes}'

  "${COMPOSE[@]}" down -v --remove-orphans >&2
}

mkdir -p "$ROOT/tmp"
OUT_ALL='[]'
for lane in baseline watt; do
  [[ "$LANE" == "both" || "$LANE" == "$lane" ]] || continue
  res="$(bench_lane "$lane")"
  echo "$res" >> "$ROOT/tmp/bench-web-edge-history.jsonl"
  echo "$res" | jq . > "$ROOT/tmp/bench-web-edge-$lane.json"
  OUT_ALL="$(echo "$OUT_ALL" | jq --argjson r "$res" '. + [$r]')"
done
echo "$OUT_ALL" | jq .
