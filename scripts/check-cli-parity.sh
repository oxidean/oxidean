#!/usr/bin/env bash
# Drift guard for docs/CLI-PARITY.md.
#
# 1. Every `ox` command a matrix row marks **implemented** must parse —
#    `ox <cmd> --help` exits 0 (or reaches the parser's own error, which is
#    still a parse-level answer, not "unknown command").
# 2. Rows marked **implemented** on the ox side must not be empty.
# 3. The pinned gh version is echoed; when `gh` is installed and differs,
#    a warning is printed (not fatal — the matrix is reconciled manually).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MATRIX=docs/CLI-PARITY.md
OX=${OX_BIN:-target/debug/ox}
if [ ! -x "$OX" ]; then
  cargo build -q -p oxidean-cli
fi

fail=0

# ox command groups claimed implemented. For each matrix row whose status
# cell contains `implemented`, take the group word — from the ox column, or
# from the gh column when the ox cell says `same`/is a bare dash. The UX
# table is skipped (its ox column holds prose, not commands).
while IFS= read -r group; do
  [ -z "$group" ] && continue
  out=$("$OX" "$group" --help 2>&1 || true)
  if printf '%s' "$out" | grep -q "unknown command"; then
    echo "cli-parity-check: matrix claims implemented but does not parse: ox $group"
    fail=1
  fi
done < <(
  awk -F'|' '
    /^## / { in_ux = ($0 ~ /Terminal/) }
    !in_ux && $4 ~ /implemented/ {
      cell = ($3 ~ /`/) ? $3 : $2
      if (match(cell, /`[a-z-]+/)) print substr(cell, RSTART + 1, RLENGTH - 1)
    }
  ' "$MATRIX" | sort -u
)

pin=$(grep -oE 'gh [0-9]+\.[0-9]+\.[0-9]+' "$MATRIX" | head -1 | awk '{print $2}')
if [ -z "${pin:-}" ]; then
  echo "cli-parity-check: no gh version pin found in $MATRIX"
  fail=1
elif command -v gh >/dev/null 2>&1; then
  have=$(gh --version | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
  if [ "$have" != "$pin" ]; then
    echo "cli-parity-check: warning — matrix pinned to gh $pin, local gh is $have (reconcile manually)"
  fi
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "cli-parity-check: ok (gh ${pin:-?} pin, $(grep -c 'implemented' "$MATRIX") implemented rows verified)"
