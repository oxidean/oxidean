#!/usr/bin/env bash
# Validate vendored stack-presets: catalog ↔ dirs, size budget (issue #18).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PRESETS="$ROOT/crates/oxidean-api-repo/assets/stack-presets"
CATALOG="$PRESETS/catalog.json"

MAX_PACK_BYTES=$((2 * 1024 * 1024))
MAX_TOTAL_BYTES=$((40 * 1024 * 1024))

if [[ ! -f "$CATALOG" ]]; then
  echo "error: missing $CATALOG" >&2
  exit 1
fi

# jq (not python3) — the ci tools image ships jq but no python.
if ! ids_raw="$(jq -r '
    .packs[] | .id as $id
    | (has("source") and has("source_ref") and has("last_synced"))
    | if . then $id else error("pack \($id) missing required catalog keys") end
  ' "$CATALOG")"; then
  exit 1
fi
mapfile -t IDS <<< "$ids_raw"

TOTAL=0
FAIL=0

for id in "${IDS[@]}"; do
  dir="$PRESETS/$id"
  if [[ ! -d "$dir" ]]; then
    echo "error: catalog id '$id' has no directory" >&2
    FAIL=1
    continue
  fi
  # Sum file sizes (exclude directory metadata noise)
  size=$(find "$dir" -type f -print0 | xargs -0 stat -c '%s' 2>/dev/null | awk '{s+=$1} END {print s+0}')
  TOTAL=$((TOTAL + size))
  if [[ "$size" -gt "$MAX_PACK_BYTES" ]]; then
    echo "error: pack '$id' is ${size} bytes (max ${MAX_PACK_BYTES})" >&2
    FAIL=1
  fi
  # Packs ship manifests (package.json, requirements.txt ranges) — never
  # frozen resolutions: locks pin stale versions that dependency-graph
  # scans flag as security-patch noise (issue #186).
  while IFS= read -r -d '' lock; do
    echo "error: pack '$id' vendors lockfile '${lock#$dir/}' — presets ship dependency manifests, not frozen resolutions" >&2
    FAIL=1
  done < <(find "$dir" -type f \( \
      -name 'Cargo.lock' -o -name 'package-lock.json' -o -name 'yarn.lock' \
      -o -name 'pnpm-lock.yaml' -o -name 'bun.lock' -o -name 'bun.lockb' \
      -o -name 'deno.lock' -o -name 'go.sum' -o -name 'Gemfile.lock' \
      -o -name 'composer.lock' -o -name 'mix.lock' -o -name 'pubspec.lock' \
      -o -name 'poetry.lock' -o -name 'Pipfile.lock' -o -name 'uv.lock' \
      -o -name 'requirements.lock' -o -name 'flake.lock' \
      -o -name 'packages.lock.json' -o -name 'project.assets.json' \
      -o -name '.terraform.lock.hcl' -o -name 'gradle.lockfile' \
      -o -name 'Package.resolved' -o -name 'Podfile.lock' \
      -o -name 'Cartfile.resolved' -o -name 'stack.yaml.lock' \
      -o -name 'cabal.project.freeze' -o -name 'rebar.lock' \
      \) -print0)
  count=$(find "$dir" -type f | wc -l)
  if [[ "$count" -lt 1 ]]; then
    echo "error: pack '$id' has no files" >&2
    FAIL=1
  fi
done

# Every directory must be in the catalog
while IFS= read -r -d '' dir; do
  base=$(basename "$dir")
  found=0
  for id in "${IDS[@]}"; do
    if [[ "$id" == "$base" ]]; then
      found=1
      break
    fi
  done
  if [[ "$found" -eq 0 ]]; then
    echo "error: directory '$base' not listed in catalog.json" >&2
    FAIL=1
  fi
done < <(find "$PRESETS" -mindepth 1 -maxdepth 1 -type d -print0)

if [[ "$TOTAL" -gt "$MAX_TOTAL_BYTES" ]]; then
  echo "error: total stack-presets ${TOTAL} bytes exceeds ${MAX_TOTAL_BYTES}" >&2
  FAIL=1
fi

if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi

echo "check-stack-presets: ok (${#IDS[@]} packs, ${TOTAL} bytes)"
