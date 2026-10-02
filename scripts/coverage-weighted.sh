#!/usr/bin/env bash
# Weighted coverage gate (D-QH-02): unit 25% + integration 40% + e2e 35%.
# Floor 0.70 (ratcheted from bootstrap 0.65; baseline ~0.68, main measured ~0.73).
# Raise COVERAGE_WEIGHTED_FLOOR in CI/docs together when suites deepen further.
set -euo pipefail

WEIGHT_UNIT=0.25
WEIGHT_INTEGRATION=0.40
WEIGHT_E2E=0.35
FLOOR="${COVERAGE_WEIGHTED_FLOOR:-0.70}"

usage() {
  cat <<'EOF'
Usage: coverage-weighted.sh [--unit N] [--integration N] [--e2e N]
                            [--unit-json PATH] [--integration-json PATH]
                            [--e2e-checklist]

Layer scores are 0..1. Defaults (env overrides):
  COVERAGE_UNIT, COVERAGE_INTEGRATION, COVERAGE_E2E
  COVERAGE_WEIGHTED_FLOOR (default 0.70)

Vitest json-summary: --unit-json / --integration-json read total.lines.pct / 100.
--e2e-checklist runs scripts/coverage-e2e-checklist.sh for the e2e layer score.

Exit 0 when weighted score >= floor; non-zero otherwise.
EOF
}

unit=""
integration=""
e2e=""
unit_json=""
integration_json=""
use_e2e_checklist=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    --unit) unit="${2:-}"; shift 2 ;;
    --integration) integration="${2:-}"; shift 2 ;;
    --e2e) e2e="${2:-}"; shift 2 ;;
    --unit-json) unit_json="${2:-}"; shift 2 ;;
    --integration-json) integration_json="${2:-}"; shift 2 ;;
    --e2e-checklist) use_e2e_checklist=1; shift ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

pct_from_json_summary() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Missing coverage json-summary: $path" >&2
    return 1
  fi
  bun -e '
    const fs = require("node:fs");
    const p = process.argv[1];
    const j = JSON.parse(fs.readFileSync(p, "utf8"));
    const pct = Number(j?.total?.lines?.pct ?? 0);
    if (!Number.isFinite(pct)) process.exit(2);
    process.stdout.write(String(Math.max(0, Math.min(100, pct)) / 100));
  ' "$path"
}

if [[ -n "$unit_json" ]]; then
  unit="$(pct_from_json_summary "$unit_json")"
fi
if [[ -n "$integration_json" ]]; then
  integration="$(pct_from_json_summary "$integration_json")"
fi
if [[ "$use_e2e_checklist" -eq 1 ]]; then
  root="$(cd "$(dirname "$0")/.." && pwd)"
  e2e="$("$root/scripts/coverage-e2e-checklist.sh" --score-only)"
fi

unit="${unit:-${COVERAGE_UNIT:-}}"
integration="${integration:-${COVERAGE_INTEGRATION:-}}"
e2e="${e2e:-${COVERAGE_E2E:-}}"

missing=0
for name in unit integration e2e; do
  eval "val=\$$name"
  if [[ -z "${val}" ]]; then
    echo "Missing $name layer score (set --$name, env COVERAGE_${name^^}, or json/checklist)" >&2
    missing=1
  fi
done
[[ "$missing" -eq 0 ]] || exit 2

is_score() {
  bun -e '
    const n = Number(process.argv[1]);
    if (!Number.isFinite(n) || n < 0 || n > 1) process.exit(1);
  ' "$1"
}

for name in unit integration e2e; do
  eval "val=\$$name"
  if ! is_score "$val"; then
    echo "Invalid $name score '$val' (need 0..1)" >&2
    exit 2
  fi
done

weighted="$(bun -e '
  const u = Number(process.argv[1]);
  const i = Number(process.argv[2]);
  const e = Number(process.argv[3]);
  const w = 0.25 * u + 0.40 * i + 0.35 * e;
  process.stdout.write(w.toFixed(6));
' "$unit" "$integration" "$e2e")"

echo "coverage-weighted: unit=$unit (25%) integration=$integration (40%) e2e=$e2e (35%)"
echo "coverage-weighted: score=$weighted floor=$FLOOR"

pass="$(bun -e '
  const score = Number(process.argv[1]);
  const floor = Number(process.argv[2]);
  process.exit(score + 1e-9 >= floor ? 0 : 1);
' "$weighted" "$FLOOR" && echo 1 || echo 0)"

if [[ "$pass" -eq 1 ]]; then
  echo "coverage-weighted: PASS"
  exit 0
fi

echo "coverage-weighted: FAIL (below floor $FLOOR)" >&2
exit 1
