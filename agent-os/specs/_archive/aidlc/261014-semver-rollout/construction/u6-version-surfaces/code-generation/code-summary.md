# U6 Version Surface Wiring — Code Summary

## What landed

- `crates/oxidean-core/src/lib.rs` — `wire_compatible(a, b)` shared caret
  SemVer predicate (0.x: same minor; ≥1.0: same major; unparseable →
  incompatible, BR6.1); `LEGACY_WIRE_VERSION = "0.1.0"` (the contract the
  header ordinal `"1"` aliases); `MCP_PROTOCOL_VERSIONS` canonical pin list
  (BR6.7); `ManifestResponse.wire_version` added (BR6.3);
  `RPC_PROTOCOL_VERSION` documented as the frozen legacy ordinal.
- `crates/oxidean-api/src/rpc.rs` — `check_version_header` dual-accepts `"1"`
  (→ `LEGACY_WIRE_VERSION`) and compatible SemVer via `wire_compatible`
  against `WIRE_VERSION` (BR6.2); the `"1"` alias expires automatically when
  the wire reaches `1.0.0`. `manifest_response()` emits `wire_version` beside
  the frozen `protocol_version`, `server_version`, `min_cli_version`.
- `crates/oxidean-api/src/routes/cli_dist.rs` — `/cli/latest` now serves the
  release-written `latest.json` (`version` + `targets` + `dist_commit`);
  absent/corrupt/foreign-schema manifests answer `{version: null,
  targets: []}` with a warn log — never `env!("CARGO_PKG_VERSION")` (BR6.5,
  FR6.3). `dist_commit` falls back to the `SOURCE_COMMIT` drop-in.
- `crates/oxidean-api/src/rest/spec.rs` + regenerated `docs/openapi.yaml` —
  `info.version` renders `WIRE_VERSION` (`0.1.0`).
- `crates/oxidean-api/src/mcp.rs`, `crates/oxidean-web/src/wellknown.rs`,
  `crates/oxidean-cli/src/mcp.rs`, `apps/web/src/lib/webmcp.ts`,
  `apps/web/src/routes/admin/mcp.tsrx` — all five MCP pin sites now read
  `MCP_PROTOCOL_VERSIONS` from `oxidean-core` (TS side via the generated
  client). `oxidean-web` gained its `oxidean-core` dependency edge.
- `crates/oxidean-cli/src/rpc.rs` — sends `Oxidean-RPC-Version: <WIRE_VERSION>`
  (`0.1.0`) instead of the integer ordinal.
- `crates/oxidean-cli/src/manifest.rs` — `Manifest.wire_version: Option`;
  `compat_notices` caret-compares `wire_version` when present, maps
  `protocol_version: 1` to the legacy 0.1.x contract, and warns-only when
  neither is advertised (BR6.4). `min_cli_version` floor check unchanged.
- `rpc_gen.rs` template → regenerated `packages/api-client/src/index.ts` —
  exports `WIRE_VERSION`, `LEGACY_WIRE_VERSION`, `MCP_PROTOCOL_VERSIONS`,
  `wireCompatible()` (TS twin), `wireCompatWarning()` (advisory, never
  refuses); `system.manifest()` surfaces the warning via `console.warn`;
  `ManifestResponse.wire_version?: string`; header sends `WIRE_VERSION`.
- Dead config removed (BR6.8): `OXIDEAN_RPC_VERSION` deleted from
  `.env.example`, `docker-compose.yml`, `docs/CONFIGURATION.md`.
- Doc truth pass: `docs/API.md`, `docs/ARCHITECTURE.md`, `docs/database.md`
  describe the SemVer header (`0.1.0` today, legacy `1` accepted);
  `docs/CLI.md` `/cli/latest` prose now describes `latest.json` as the source.

## Tests

- core: `wire_compatible` tables (0.x minor, ≥1 major, boundary cross,
  garbage, prerelease/build), manifest round-trip with `wire_version`, MCP
  pin shape.
- api: `check_version_header` dual-accept + reject matrix, legacy-alias
  expiry, `manifest_shape` asserts all six fields; `cli_dist` unit tests for
  absent/corrupt/invalid/valid `latest.json`; integration test serves the
  fixture manifest verbatim.
- cli: `compat_notices` BR6.4 paths (incompatible wire, legacy
  `protocol_version: 1`, no fields → warn-only); `rpc_http` mock asserts the
  SemVer header.
- TS: `index.test.ts` asserts `WIRE_VERSION`, `wireCompatible` table,
  `wireCompatWarning` advisory paths, `MCP_PROTOCOL_VERSIONS` shape, and the
  `0.1.0` header on requests.

## Decisions

- **`latest.json` absent/corrupt → `{version: null, targets: []}`** per
  BR6.5/WF-4 — supersedes C5's earlier `0.0.0-dev` wording; both forbid the
  crate version (the real invariant), and `null` gives `ox self-update` an
  honest "unpublished" failure rather than a phantom `0.0.0-dev` update.
- **`schema_version` is validated leniently** — absent or `1` accepted; other
  values are treated as unreadable (unpublished).
- **`dist_commit` precedence**: `latest.json` field wins, `SOURCE_COMMIT`
  drop-in remains the fallback — self-hosts that stage the file keep working.
- **`/cli/bin/*` is unchanged** — drop-in binaries remain fetchable; only the
  `/cli/latest` advertisement requires a manifest.
- **`RPC_VERSION = 1` stays exported** from the generated client (frozen
  legacy ordinal) so third-party consumers don't break; the wire send uses
  `WIRE_VERSION`.
- **Existing script/test header sends of `"1"` are intentionally kept** —
  they exercise the dual-accept legacy path every run.

## Deviations

- C5 `absent_or_invalid` wording (`0.0.0-dev`) superseded by BR6.5 (`null`) —
  recorded here and in the review.

## Verification

- `cargo check --workspace` clean; `cargo clippy` on touched crates clean.
- `cargo test -p oxidean-core -p oxidean-cli` green; `cargo test -p
  oxidean-api` — all green except a pre-existing `repo_fork` parallel-run
  flake (`GIT_TEMPLATE_DIR` env poisoning races `clone_bare`; passes
  `--test-threads=1` and per-test; unrelated to this unit).
- `bunx vitest run` in `packages/api-client` — 6/6.
- `bun test scripts/versioning` — 115 pass; `make version-check` ok.
- `make web-lint` + `make web-format-check` clean.
- `make openapi-gen` regenerated `docs/openapi.yaml` (`info.version: 0.1.0`).
