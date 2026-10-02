#!/usr/bin/env bash
# Contract: change-aware UI coverage rejects skip-only for touched surfaces.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "ui-coverage-change.contract: FAIL: $*" >&2; exit 1; }
pass() { echo "ui-coverage-change.contract: ok — $*"; }

# Inventory-only (no base) must still pass.
unset UI_COVERAGE_BASE BROWSER_COVERAGE_BASE UI_COVERAGE_TOUCHED
./scripts/browser-coverage-check.sh >/tmp/bcc-inventory.txt \
  || fail "inventory browser-coverage-check should PASS"
grep -q 'mode=inventory-only' /tmp/bcc-inventory.txt \
  || fail "expected inventory-only mode in browser-coverage-check"
./scripts/route-coverage-check.sh >/tmp/rcc-inventory.txt \
  || fail "inventory route-coverage-check should PASS"
grep -q 'mode=inventory-only' /tmp/rcc-inventory.txt \
  || fail "expected inventory-only mode in route-coverage-check"
pass "inventory-only passes"

# Touching a known skip-only high-risk surface must fail browser-coverage.
if UI_COVERAGE_TOUCHED='components/settings/gpg-key-add-form.tsrx' \
  ./scripts/browser-coverage-check.sh >/tmp/bcc-touch.txt 2>/tmp/bcc-touch.err; then
  fail "expected browser-coverage-check to FAIL for touched skip-only gpg-key-add-form"
fi
grep -Eq 'skip-only|still skip-only' /tmp/bcc-touch.err \
  || fail "expected skip-only failure message for gpg-key-add-form; got: $(cat /tmp/bcc-touch.err)"
pass "touched skip-only high-risk UI fails browser-coverage"

# Touching a covered high-risk surface must still pass.
UI_COVERAGE_TOUCHED='components/settings/pat-classic-form.tsrx' \
  ./scripts/browser-coverage-check.sh >/tmp/bcc-ok.txt \
  || fail "touched covered PAT form should PASS"
grep -q 'change-aware' /tmp/bcc-ok.txt \
  || fail "expected change-aware mode for UI_COVERAGE_TOUCHED"
pass "touched covered high-risk UI passes"

# Touching a skip-only route must fail route-coverage (if any skip-only route exists).
SKIP_ROUTE="$(
  bun -e '
    import { routeCoverageManifest } from "./apps/web/src/test/route-coverage.manifest.ts";
    const hit = routeCoverageManifest.find((e) =>
      !e.layoutOnly &&
      Array.isArray(e.coverage) &&
      e.coverage.length > 0 &&
      e.coverage.every((c) => c.kind === "skip")
    );
    if (!hit) process.exit(2);
    process.stdout.write("routes/" + hit.route);
  ' 2>/dev/null || true
)"
if [[ -n "${SKIP_ROUTE}" ]]; then
  if UI_COVERAGE_TOUCHED="${SKIP_ROUTE}" \
    ./scripts/route-coverage-check.sh >/tmp/rcc-touch.txt 2>/tmp/rcc-touch.err; then
    fail "expected route-coverage-check to FAIL for touched skip-only ${SKIP_ROUTE}"
  fi
  grep -q 'skip-only' /tmp/rcc-touch.err \
    || fail "expected skip-only failure for ${SKIP_ROUTE}; got: $(cat /tmp/rcc-touch.err)"
  pass "touched skip-only route fails route-coverage (${SKIP_ROUTE})"
else
  pass "no skip-only routes to probe (acceptable)"
fi

echo "ui-coverage-change.contract: PASS"
