#!/bin/sh
# ox binary smoke — runs the artifact a CI matrix leg produced.
# usage: ox-smoke.sh <path-to-ox>
set -eu

OX="$1"
[ -x "$OX" ] || { echo "not executable: $OX"; exit 1; }

out=$("$OX" --version)
echo "$out" | grep -qE '^ox [0-9]+\.[0-9]+\.[0-9]+' \
    || { echo "bad --version output: $out"; exit 1; }

"$OX" --help | grep -q 'ox <command>'
"$OX" | grep -q 'ox <command>'

if "$OX" frobnicate >/dev/null 2>&1; then
    echo "unknown command exited 0"; exit 1
fi

# Per-command help resolves without a config or network.
"$OX" issue --help | grep -q 'ox issue'
"$OX" issue create --help | grep -q 'ox issue create'
"$OX" repo deploy-key --help | grep -q 'deploy-key'

# Unknown / missing subcommands exit 2 (usage).
set +e
"$OX" frobnicate >/dev/null 2>&1; [ "$?" -eq 2 ] || { echo "unknown: expected 2"; exit 1; }
"$OX" repo deploy-key >/dev/null 2>&1; [ "$?" -eq 2 ] || { echo "nested missing sub: expected 2"; exit 1; }
"$OX" repo view badspec >/dev/null 2>&1; [ "$?" -eq 2 ] || { echo "bad repo spec: expected 2"; exit 1; }
set -e

# Scrubbed env: empty config dir + no instance/token env vars. `auth status`
# must fail as a usage error (2, "no instance configured") without touching
# the network — which also proves the config directory resolved (on Windows
# that is the %APPDATA% fallback; a NoConfigDir crash exits 1 instead).
SCRUB=$(mktemp -d)
set +e
err=$(env -u OXIDEAN_INSTANCE -u OXIDEAN_TOKEN \
    OXIDEAN_CONFIG_DIR="$SCRUB" "$OX" auth status 2>&1)
code=$?
set -e
[ "$code" -eq 2 ] || { echo "auth status: expected exit 2, got $code: $err"; exit 1; }
echo "$err" | grep -qi 'instance'

echo "smoke OK: $out"
