# U6 Version Surface Wiring — Unit Test Instructions

Test-after methodology (testing strategy: standard). Write the tests below
after implementation, run them green, then run the full workspace check.

## Rust unit tests

### `crates/oxidean-core/src/lib.rs` — `wire_compatible`

- `wire_compatible("0.1.0", "0.1.5")` → true (same 0.x minor, patch differs)
- `wire_compatible("0.1.0", "0.2.0")` → false (0.x minor differs)
- `wire_compatible("1.0.0", "1.4.2")` → true (same major ≥1.0)
- `wire_compatible("1.9.0", "2.0.0")` → false
- `wire_compatible("0.9.9", "1.0.0")` → false (crossing the 1.0 boundary)
- `wire_compatible("1.0.0", "0.9.9")` → false (symmetric)
- `wire_compatible("garbage", "0.1.0")` / `wire_compatible("0.1.0", "x")` → false
- `wire_compatible("0.1.0-beta.1", "0.1.0")` → true (prerelease in same minor)
- `ManifestResponse` round-trips JSON with `wire_version`, `protocol_version`,
  `server_version`, `min_cli_version`, `procedures`, `capabilities`.

### `crates/oxidean-api/src/rpc.rs` — `check_version_header`

Table over header value → expected accept/reject while `WIRE_VERSION = 0.1.0`:

- missing header → reject (mismatch error)
- `"1"` → accept (legacy alias for 0.1.x, BR6.2)
- `"0.1.0"`, `"0.1.9"` → accept (same 0.1 minor)
- `"0.2.0"` → reject
- `"1.0.0"` → reject
- `"garbage"` → reject
- `manifest_response()` JSON contains `wire_version == WIRE_VERSION` and
  `protocol_version == 1`.

### `crates/oxidean-api/src/routes/cli_dist.rs` — `latest()`

- `latest.json` present + valid → 200 with its `version` and `targets`.
- `latest.json` absent → 200 with `version: null`, `targets: []`.
- `latest.json` corrupt JSON → same unpublished response (no panic/500).
- Response never equals `env!("CARGO_PKG_VERSION")` when manifest absent.

### `crates/oxidean-cli/src/manifest.rs` — BR6.4 fallback paths

- Manifest with `wire_version` incompatible → warning notice emitted.
- Manifest with `wire_version` compatible → no wire warning.
- Manifest without `wire_version`, `protocol_version == 1` → treated as legacy
  0.1.x: compatible during the 0.x window, advisory only.
- Manifest without either field → warn-and-continue, no hard failure.
- `min_cli_version` floor still enforced independently.

## TypeScript tests

### `packages/api-client` surface (bun test or static assertions)

- Generated client exports `WIRE_VERSION === "0.1.0"`, `MCP_PROTOCOL_VERSIONS`
  ending in `"2025-06-18"`, `wireCompatible()`, `RPC_VERSION_HEADER`.
- Generated `ManifestResponse` type includes `wire_version` (type-level check
  via a satisfied assignment in test or `// @ts-expect-error` absence).
- `apps/web` MCP pin sites import the generated constant (grep-shape test or
  rely on type-check failure if removed).

## Regeneration checks

- `make rpc-gen` → `packages/api-client` contains the new exports.
- `make openapi-gen` → `docs/openapi.yaml` `info.version` is `0.1.0`.

## Full verification

`cargo test -p oxidean-core -p oxidean-api -p oxidean-cli`,
`cargo check --workspace`, `bun test scripts/versioning`, `make version-check`,
plus the app's type gate if `apps/web` files changed (`make web-lint`).
