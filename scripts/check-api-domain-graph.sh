#!/usr/bin/env bash
# Issue #162 — guard the api domain-crate split. Asserts:
#   1. Dependency direction: domain crates may only depend on
#      `oxidean-api-core` among `oxidean-api*` crates (never on one another);
#      `oxidean-api-core` depends on none of them; the shell `oxidean-api`
#      may compose all of them.
#   2. Procedure ownership is disjoint: every `PROCEDURES` const across the
#      domain crates plus core's residual list contains each procedure name
#      at most once — a duplicated name would shadow silently in dispatch.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$ROOT" <<'PY'
import glob
import os
import re
import sys
import tomllib

root = sys.argv[1]
fail: list[str] = []

api_dep = re.compile(r"(?m)^(oxidean-api[a-z-]*)\s*=")
section = re.compile(r"(?ms)^\[(?:build-|dev-)?dependencies\](.*?)(?=^\[|\Z)")
proc_block = re.compile(r"(?ms)pub const PROCEDURES\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\];")
proc_name = re.compile(r'^\s*"([a-zA-Z][a-zA-Z0-9._]*)",\s*$', re.M)

crates = sorted(glob.glob(os.path.join(root, "crates", "oxidean-api*", "Cargo.toml")))
for manifest in crates:
    data = open(manifest).read()
    name = tomllib.loads(data)["package"]["name"]
    deps: set[str] = set()
    for m in section.finditer(data):
        deps.update(api_dep.findall(m.group(1)))
    if name == "oxidean-api":
        continue  # the composition shell may depend on every api-* crate
    allowed = set() if name == "oxidean-api-core" else {"oxidean-api-core"}
    bad = deps - allowed
    if bad:
        fail.append(
            f"{name} depends on {sorted(bad)} — domain crates may only use "
            f"{sorted(allowed) or 'no api-* crate'}"
        )

seen: dict[str, str] = {}
sources = glob.glob(os.path.join(root, "crates", "oxidean-api-*", "src", "lib.rs"))
sources.append(os.path.join(root, "crates", "oxidean-api-core", "src", "rpc.rs"))
for src_path in sources:
    owner = os.path.basename(os.path.dirname(os.path.dirname(src_path)))
    blocks = proc_block.findall(open(src_path).read())
    if not blocks:
        continue
    for p in proc_name.findall("\n".join(blocks)):
        if p in seen:
            fail.append(f"procedure {p} is owned by both {seen[p]} and {owner}")
        else:
            seen[p] = owner

print(f"api-domain-graph: {len(seen)} procedures across {len(sources)} PROCEDURES lists")
if fail:
    for line in fail:
        print(f"error: {line}", file=sys.stderr)
    sys.exit(1)
print("api-domain-graph: ok")
PY
