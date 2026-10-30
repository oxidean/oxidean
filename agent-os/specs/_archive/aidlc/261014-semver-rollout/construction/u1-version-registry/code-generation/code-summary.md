# U1 Version Registry — Code Summary

## What landed

- `versioning.toml` (repo root) — schema v1 registry: `[baseline]`,
  `[grammar]`, 10 `[[component]]` entries (7 crates, 2 Bun packages, 1 wire
  contract). `baseline.commit = "pending"` per BR1.6.
- `scripts/versioning/` — Bun TypeScript CLI:
  - `lib/registry.ts` — types, strict TOML parse, all schema/semantic rules
    (BR1.1 xor carriers, BR1.3 unknown keys, BR1.4 DAG, BR1.7 uniqueness),
    line-accurate violations, `loadRegistry()` (never partial).
  - `lib/globs.ts` — conservative segment-wise `globsMayOverlap` + `matchesAny`.
  - `lib/workspace.ts` — `cargo metadata` + Bun workspace enumeration,
    literal-version readers (Cargo.toml, package.json, `file#NAME` constant),
    non-component path rejection (BR1.5).
  - `commands/check.ts` — `check` orchestration, `file:line` text output /
    `--json`, exit 0/1/2, pending-baseline warning (BR1.6/BR1.8).
  - `cli.ts` — argv dispatch, `--root` flag, usage → exit 2.
- Manifest migration: all 7 crate manifests carry literal `version = "0.1.0"`;
  `version` removed from root `[workspace.package]`; bun manifests already
  literal. `pub const WIRE_VERSION: &str = "0.1.0"` added in
  `crates/oxidean-core/src/lib.rs` beside `RPC_PROTOCOL_VERSION` (kept — still
  the on-wire ordinal until U6).
- `make version-check` target (+ `.PHONY`).

## Deviation from functional spec

- `wire.constant_path` points at `crates/oxidean-core/src/lib.rs` — the spec's
  draft said `src/rpc.rs`, which does not exist; `lib.rs` is where
  `RPC_PROTOCOL_VERSION` lives. Same carrier intent.
- Crate set is the real workspace (`oxidean-db`, `oxidean-git`), replacing the
  spec's stale names (`oxidean-logging`, `oxidean-memory`).
- Component ids shortened where meaningful: `ox` (CLI crate), `api-client`,
  `web` — matching their shipped artifact names and tag prefixes.

## Tests

30 tests, all passing: `bun test scripts/versioning` — registry schema
parsing, strict-key violations, kind/carrier rules, uniqueness, glob-overlap
unit cases, DAG edge cases, baseline states, fixture-workspace check matrix
(BR1.1/1.2/1.5/1.6), `--json` shape, exit codes, and a real-repo integration
check. `cargo check -p oxidean-core` clean; Cargo.lock unchanged.
