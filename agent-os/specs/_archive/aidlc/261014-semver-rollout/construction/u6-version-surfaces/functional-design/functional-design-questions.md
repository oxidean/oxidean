# Functional Design — U6 Version Surface Wiring — Questions

Unit: `u6-version-surfaces` (kind: library). Scope: WIRE_VERSION + compat predicate, `system.manifest` fields, OpenAPI `info.version`, `/cli/latest` from `latest.json`, MCP pin dedup, dead `OXIDEAN_RPC_VERSION` removal. Contract: C6. FRs: FR6, FR6.1–FR6.6.

## Q1. `system.manifest` — does `protocol_version` stay?

- A) Co-emit `protocol_version` (integer alias of WIRE_VERSION's compatible form) for old clients, plus new `wire_version`/`server_version`/`min_cli_version` fields; mark deprecated in docs.
- B) Remove `protocol_version` outright.

[Answer]: A — co-emit as a compatibility alias during the transition. Old clients may read it; removing silently breaks them. The alias emits `1` while wire is `0.x` (pre-1.0 semantics: wire 0.x <-> protocol 1 — the deployed integer the ecosystem already speaks) — decided: alias emits the integer the OLD contract used, i.e. `1`, until wire reaches `1.x`.

## Q2. Old-server manifests lack `wire_version` — client behaviour?

- A) Client treats absent `wire_version` as wire `"1"` (integer era) → compatible iff its own wire is 0.x-compatible with `"1"`; pre-request warning stays advisory-only.
- B) Treat absent as incompatible and refuse.

[Answer]: A — absent `wire_version` + integer `protocol_version=1` means the old contract; the generated client maps it to wire `"0.1"` semantics. Hard-refusal would break every deployed CLI against pre-versioning servers.

## Q3. Server-side `Oxidean-RPC-Version` gate during transition?

- A) Accept BOTH `"1"` (integer era, from deployed CLIs) and caret-compatible wire strings; reject everything else. The integer `1` maps to the pre-versioning contract = wire `0.1.x` semantics.
- B) Strict wire strings only.

[Answer]: A — dual-accept during the 0.x window. `"1"` is treated as the legacy contract identifier (wire 0.1.x); once wire reaches 1.x the integer path is dropped (documented removal condition). Compat predicate: `<1.0.0` same `0.MINOR` any patch; `>=1.0.0` same major.

## Q4. `/cli/latest` when `latest.json` is absent?

- A) Documented default response: `{"version": null, "targets": []}` shape per C5's missing-file value — honest "no dist manifest" signal, never the API crate version.
- B) 404.

[Answer]: A — the C5-specified missing-file value; clients learn there is no published dist rather than guessing. Never fall back to `CARGO_PKG_VERSION` (that's the bug being fixed).

## Q5. MCP pin dedup — where's the canonical list?

- A) `MCP_PROTOCOL_VERSIONS` const in `oxidean-core`, propagated to TS via `rpc-gen` into `@oxidean/api-client`.
- B) Keep separate pins.

[Answer]: A — single source in oxidean-core; TS side is generated, never hand-maintained. Dead `OXIDEAN_RPC_VERSION` env/config is removed outright (it's already inert — removal is the cleanup, not a deprecation).

---

## Consolidated Summary Confirmation

- `system.manifest` co-emits `protocol_version` (legacy `1` alias) + `wire_version`, `server_version`, `min_cli_version`.
- Absent `wire_version` on old servers → legacy contract `1` → wire `0.1.x` semantics; advisory warning preserved.
- Server gate dual-accepts `"1"` and caret-compatible wire strings during the 0.x window.
- `/cli/latest` serves the C5 missing-file default when `latest.json` is absent; never CARGO_PKG_VERSION.
- One `MCP_PROTOCOL_VERSIONS` constant in oxidean-core → rpc-gen → api-client; `OXIDEAN_RPC_VERSION` deleted.

Does this all look correct before I generate the functional-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
