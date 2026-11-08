# U6 Version Surface Wiring — Functional Spec

## Purpose

Makes every external version surface tell the truth: wire-contract SemVer (`WIRE_VERSION` replacing integer `RPC_PROTOCOL_VERSION`), `system.manifest` fields, OpenAPI `info.version`, `/cli/latest` from the dist manifest, deduplicated MCP pins, and removal of the dead `OXIDEAN_RPC_VERSION` config.

## Workflows

### WF-1 Wire constant + shared compat predicate

1. `WIRE_VERSION: &str` lives in `oxidean-core` (the `wire` registry entry's `constant_path`); replaces `RPC_PROTOCOL_VERSION` (integer) everywhere it was read.
2. One `wire_compatible(a, b) -> bool` predicate implements BR6.1; server gate and generated client share it via codegen.

### WF-2 Server header gate (BR6.2)

`Oxidean-RPC-Version` header: `"1"` → legacy contract (wire 0.1.x semantics); else parse + BR6.1; mismatch → wire-mismatch error. Dual-accept is the deliberate transition window; the `"1"` path drops at wire ≥1.0.0 (documented condition, not a forever-alias). Consequence: the first wire bump to 0.2.x hard-rejects all legacy `"1"` CLIs — intended SemVer behaviour, recorded in `docs/WIRE-CHANGELOG.md` when it happens.

### WF-3 `system.manifest` (BR6.3)

Emits `server_version`, `wire_version`, `min_cli_version`, `protocol_version` (=1 while wire <1.0.0). Old clients keep working; new clients prefer `wire_version`.

### WF-4 `/cli/latest` (BR6.5)

Reads `latest.json` from `OXIDEAN_CLI_DIST_DIR` (merge/drop-in per C5): emits `{version, targets}`; missing file → `{version: null, targets: []}`; corrupt → same default + warn log. Never reads `CARGO_PKG_VERSION`.

### WF-5 OpenAPI + MCP pins + cleanup

- `rest/spec.rs` `info.version = WIRE_VERSION`; `make openapi-gen` refreshes `docs/openapi.yaml` (BR6.6).
- `MCP_PROTOCOL_VERSIONS` canonical in oxidean-core; five pin sites collapse; TS copy emitted by rpc-gen (BR6.7).
- `OXIDEAN_RPC_VERSION` env/config deleted — code, docs, `.env.example` (BR6.8).

## States

- Wire compatibility is total: every (client-wire, server-wire) pair is compatible or not per BR6.1/BR6.2 — no partial states.
- `/cli/latest`: `published` (manifest present+valid) or `unpublished` (absent/corrupt → null response). No third state.

## Interfaces

- Consumes: C5 `latest.json` (written by U4's stable release flow), registry `wire` entry.
- Emits: shared compat predicate (server + rpc-gen client), manifest fields (C6), OpenAPI version, MCP constant.
- Removal boundary: `RPC_PROTOCOL_VERSION` integer and `OXIDEAN_RPC_VERSION` die here — the changelog records it under `wire`'s changelog + the api changelog.
