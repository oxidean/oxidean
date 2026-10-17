#!/usr/bin/env bash
# Validate vendored workflow templates: yml ↔ properties.json pairing, required
# properties fields, referenced icons exist, no stray files, size budget.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATES="$ROOT/crates/oxidean-api/assets/workflow-templates"
CATALOG="$TEMPLATES/catalog.json"

MAX_TOTAL_BYTES=$((1024 * 1024))

if [[ ! -f "$CATALOG" ]]; then
  echo "error: missing $CATALOG" >&2
  exit 1
fi

FAIL=0

python3 - <<'PY' "$TEMPLATES" "$CATALOG"
import json, os, sys

root, catalog_path = sys.argv[1], sys.argv[2]
catalog = json.load(open(catalog_path))
dirs = catalog["include_dirs"]
allowed_top = set(dirs) | {"icons", "catalog.json"}
fail = 0

for key in ("repo", "commit", "synced"):
    if key not in catalog["upstream"]:
        print(f"error: catalog upstream missing {key}", file=sys.stderr); fail = 1
if "excluded" not in catalog:
    print("error: catalog missing excluded map", file=sys.stderr); fail = 1

templates = 0
for d in dirs:
    ddir = os.path.join(root, d)
    if not os.path.isdir(ddir):
        print(f"error: missing dir {ddir}", file=sys.stderr); fail = 1; continue
    for ent in os.listdir(ddir):
        if os.path.isdir(os.path.join(ddir, ent)):
            print(f"error: unexpected subdir {d}/{ent}", file=sys.stderr); fail = 1
            continue
        if ent.endswith((".yml", ".yaml")):
            stem = ent.rsplit(".", 1)[0]
            props = os.path.join(ddir, f"{stem}.properties.json")
            if not os.path.isfile(props):
                print(f"error: {d}/{ent} has no properties.json", file=sys.stderr)
                fail = 1; continue
            p = json.load(open(props))
            for key in ("name", "description", "iconName", "categories"):
                if key not in p:
                    print(f"error: {d}/{stem}.properties.json missing {key}", file=sys.stderr)
                    fail = 1
            icon = p.get("iconName")
            if icon and not os.path.isfile(os.path.join(root, "icons", f"{icon}.svg")):
                print(f"warning: {d}/{stem} icon '{icon}' not vendored (octicon refs are expected)")
            templates += 1
        elif ent.endswith(".properties.json"):
            stem = ent[: -len(".properties.json")]
            if not (os.path.isfile(os.path.join(ddir, f"{stem}.yml"))
                    or os.path.isfile(os.path.join(ddir, f"{stem}.yaml"))):
                print(f"error: {d}/{ent} has no workflow file", file=sys.stderr); fail = 1
        else:
            print(f"error: unexpected file {d}/{ent}", file=sys.stderr); fail = 1

for ent in os.listdir(root):
    if ent not in allowed_top:
        print(f"error: unexpected top-level entry {ent}", file=sys.stderr); fail = 1

for key in catalog.get("templates", {}):
    d, _, stem = key.partition("/")
    if d not in dirs or not (
        os.path.isfile(os.path.join(root, d, f"{stem}.yml"))
        or os.path.isfile(os.path.join(root, d, f"{stem}.yaml"))
    ):
        print(f"error: catalog templates entry '{key}' has no vendored template", file=sys.stderr)
        fail = 1

print(f"check: {templates} templates validated", file=sys.stderr)
sys.exit(fail)
PY

TOTAL=$(find "$TEMPLATES" -type f -print0 | xargs -0 stat -c '%s' 2>/dev/null | awk '{s+=$1} END {print s+0}')
if [[ "$TOTAL" -gt "$MAX_TOTAL_BYTES" ]]; then
  echo "error: workflow-templates ${TOTAL} bytes exceeds ${MAX_TOTAL_BYTES}" >&2
  FAIL=1
fi

[[ "$FAIL" -eq 0 ]] || exit 1
echo "check-workflow-templates: ok (${TOTAL} bytes)"
