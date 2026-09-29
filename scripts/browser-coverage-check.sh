#!/usr/bin/env bash
# Browser (Chromium component) coverage gate — every high-risk interactive
# apps/web .tsrx (Checkbox / RadioGroup / form.Subscribe) must declare
# browser, stack-browser, or documented skip evidence in
# apps/web/src/test/browser-coverage.manifest.ts
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$ROOT/scripts/browser-coverage-check.ts"

if [[ ! -f "$SCRIPT" ]]; then
  echo "browser-coverage-check: FAIL: missing $SCRIPT" >&2
  exit 1
fi
if ! command -v bun >/dev/null 2>&1; then
  echo "browser-coverage-check: FAIL: bun is required" >&2
  exit 1
fi

exec bun "$SCRIPT"
