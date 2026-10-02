#!/usr/bin/env bash
# Contract test for scripts/coverage-weighted.sh (11.1-02)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$ROOT/scripts/coverage-weighted.sh"
fail() { echo "FAIL: $*" >&2; exit 1; }

[[ -x "$SCRIPT" ]] || fail "scripts/coverage-weighted.sh missing or not executable"

set +e
COVERAGE_UNIT=0.5 COVERAGE_INTEGRATION=0.5 COVERAGE_E2E=0.5 "$SCRIPT" >/dev/null 2>&1
rc_low=$?
set -e
[[ "$rc_low" -ne 0 ]] || fail "expected non-zero exit when weighted score under floor"

set +e
COVERAGE_UNIT=0.9 COVERAGE_INTEGRATION=0.9 COVERAGE_E2E=0.9 "$SCRIPT" >/dev/null 2>&1
rc_high=$?
set -e
[[ "$rc_high" -eq 0 ]] || fail "expected zero exit when weighted score above floor (got $rc_high)"

# Exact boundary: floor 0.70 should pass
set +e
COVERAGE_UNIT=0.70 COVERAGE_INTEGRATION=0.70 COVERAGE_E2E=0.70 "$SCRIPT" >/dev/null 2>&1
rc_eq=$?
set -e
[[ "$rc_eq" -eq 0 ]] || fail "expected zero exit at exact floor 0.70"

grep -q '0\.25' "$SCRIPT" || fail "unit weight 0.25 missing"
grep -q '0\.40' "$SCRIPT" || fail "integration weight 0.40 missing"
grep -q '0\.35' "$SCRIPT" || fail "e2e weight 0.35 missing"
grep -q '0\.70' "$SCRIPT" || fail "floor 0.70 missing"

echo "PASS: coverage-weighted aggregator contract"
