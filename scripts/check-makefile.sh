#!/usr/bin/env bash
# Parse the root Makefile (GNU Make) and run checkmake.
# Fail closed: syntax errors, undefined-variable warnings, and checkmake violations.
#
# Usage: make makefile-lint
#        ./scripts/check-makefile.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CHECKMAKE_VERSION="${CHECKMAKE_VERSION:-0.3.2}"
MAKEFILE="${MAKEFILE:-Makefile}"
CONFIG="${CHECKMAKE_CONFIG:-checkmake.ini}"

die() {
  echo "error: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "'$1' is required"
}

need_cmd make
need_cmd curl
need_cmd uname
need_cmd mkdir
need_cmd mktemp

[[ -f "$MAKEFILE" ]] || die "Makefile not found: $MAKEFILE"
[[ -f "$CONFIG" ]] || die "checkmake config not found: $CONFIG"

# --- 1) GNU Make parse + undefined-variable warnings (dry-run help) ---
warn_log="$(mktemp)"
trap 'rm -f "$warn_log"' EXIT

# Expand the help recipes; discard recipe echo, keep make diagnostics on stderr.
if ! make -n --warn-undefined-variables -f "$MAKEFILE" help >/dev/null 2>"$warn_log"; then
  echo "==> GNU Make failed to parse or dry-run '$MAKEFILE'" >&2
  cat "$warn_log" >&2
  exit 1
fi

if grep -F 'warning: undefined variable' "$warn_log" >/dev/null 2>&1; then
  echo "==> undefined Make variables while expanding '$MAKEFILE' (help):" >&2
  grep -F 'warning: undefined variable' "$warn_log" >&2 || true
  exit 1
fi
echo "makefile-lint: GNU Make parse ok"

# --- 1.5) Target wiring: .PHONY coverage, runnable recipes, script paths ---
# This Makefile defines no file-producing targets: every target is a command,
# so a recipe-bearing target missing from .PHONY silently stops running the
# moment a file or directory with its name exists. Check both directions and
# that every scripts/ path a recipe calls exists (and is executable for .sh).

makefile_targets="$(awk '
  /^[A-Za-z0-9][A-Za-z0-9_.-]*[ \t]*:/ {
    rest = $0
    sub(/^[A-Za-z0-9][A-Za-z0-9_.-]*[ \t]*:/, "", rest)
    if (rest ~ /^:?=/) next
    name = $0
    sub(/[ \t]*:.*/, "", name)
    print name
  }' "$MAKEFILE" | sort -u)"

phony_targets="$(awk '
  /^\.PHONY:[ \t]*/ {
    line = $0
    sub(/^\.PHONY:[ \t]*/, "", line)
    n = split(line, a, /[ \t]+/)
    for (i = 1; i <= n; i++) if (a[i] != "") print a[i]
  }' "$MAKEFILE" | sort -u)"

wiring_bad=0
while IFS= read -r t; do
  [[ -n "$t" ]] || continue
  if ! printf '%s\n' "$phony_targets" | grep -qxF "$t"; then
    echo "error: target '$t' has a recipe but is not declared .PHONY" >&2
    wiring_bad=1
  fi
done <<< "$makefile_targets"

while IFS= read -r t; do
  [[ -n "$t" ]] || continue
  if ! printf '%s\n' "$makefile_targets" | grep -qxF "$t"; then
    echo "error: .PHONY declares '$t' but no such target exists in $MAKEFILE" >&2
    wiring_bad=1
    continue
  fi
  if ! out="$(make -n -f "$MAKEFILE" "$t" 2>/dev/null)" || [[ -z "${out//[[:space:]]/}" ]]; then
    echo "error: .PHONY target '$t' has no runnable recipe (make -n produced no commands)" >&2
    wiring_bad=1
  fi
done <<< "$phony_targets"

while IFS= read -r s; do
  [[ -n "$s" ]] || continue
  if [[ ! -f "$s" ]]; then
    echo "error: recipe references missing script: $s" >&2
    wiring_bad=1
  elif [[ "$s" == *.sh && ! -x "$s" ]]; then
    echo "error: recipe references non-executable script: $s" >&2
    wiring_bad=1
  fi
done < <(grep -oE '(\./)?scripts/[A-Za-z0-9_/.-]+\.(sh|mjs|ts)' "$MAKEFILE" | sed 's|^\./||' | sort -u)

[[ "$wiring_bad" -eq 0 ]] || exit 1
echo "makefile-lint: target wiring ok"

# --- 2) checkmake (pinned binary under tmp/) ---
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$arch" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) die "unsupported arch for checkmake binary: $arch" ;;
esac
case "$os" in
  linux | darwin) ;;
  *) die "unsupported OS for checkmake binary: $os" ;;
esac

bin_dir="$ROOT/tmp/tools"
bin="$bin_dir/checkmake-v${CHECKMAKE_VERSION}.${os}.${arch}"
url="https://github.com/checkmake/checkmake/releases/download/v${CHECKMAKE_VERSION}/checkmake-v${CHECKMAKE_VERSION}.${os}.${arch}"

if [[ ! -x "$bin" ]]; then
  mkdir -p "$bin_dir"
  echo "==> fetching checkmake v${CHECKMAKE_VERSION} ($os/$arch)"
  curl -fsSL -o "$bin" "$url"
  chmod +x "$bin"
fi

"$bin" --config "$CONFIG" "$MAKEFILE"
echo "makefile-lint: checkmake ok"
