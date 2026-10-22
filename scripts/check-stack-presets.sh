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

mapfile -t IDS < <(python3 - <<'PY' "$CATALOG"
import json, sys
path = sys.argv[1]
data = json.load(open(path))
for p in data["packs"]:
    print(p["id"])
    for key in ("source", "source_ref", "last_synced"):
        if key not in p:
            raise SystemExit(f"pack {p['id']} missing {key}")
PY
)

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
