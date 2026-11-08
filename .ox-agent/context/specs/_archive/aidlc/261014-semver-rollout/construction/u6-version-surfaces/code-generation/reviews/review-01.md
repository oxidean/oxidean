**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

First review of the version-surfaces unit on final bytes. Scope reviewed:
`crates/oxidean-core` (wire constants, `wire_compatible`, `ManifestResponse.wire_version`,
`MCP_PROTOCOL_VERSIONS`), `crates/oxidean-api` (RPC header gate, manifest, OpenAPI
`info.version`, `/cli/latest` `latest.json` reader, MCP pin), `crates/oxidean-cli`
(header send, manifest compat fallback, MCP pin), `crates/oxidean-web` (well-known pin),
`rpc_gen.rs` + regenerated `packages/api-client`, `apps/web` MCP consumers, docs,
`.env.example`, `docker-compose.yml`.

Key semantics verified against BR6.x:

1. `wire_compatible` implements same-minor for `0.x`, same-major for `>=1`; prerelease
   and build metadata handled (build metadata ignored per SemVer §10, prerelease is
   identity-bearing). Tests cover both.
2. Header `"1"` is dual-accepted only while `wire_compatible(LEGACY_WIRE_VERSION,
   WIRE_VERSION)` — it ceases to be accepted the moment wire leaves `0.1.x`, which is
   the honest reading (a `"1"` client claims the `0.1.x` contract).
3. `ManifestResponse` carries both `protocol_version` (frozen ordinal 1) and
   `wire_version`; clients absent `wire_version` with `protocol_version: 1` are
   treated as legacy `0.1.x`, otherwise advisory-only (BR6.4). CLI never blocks on
   compat — warnings only.
4. `/cli/latest` reads release-owned `latest.json` from `OXIDEAN_CLI_DIST_DIR`;
   absent/corrupt/incomplete manifests yield `version: null, targets: []` (BR6.5,
   WF-4) instead of the C5 draft's `"0.0.0-dev"` — noted as spec wording drift,
   unit rules win. Live target enumeration removed; `dist_commit` falls back to
   `SOURCE_COMMIT`.
5. MCP protocol revisions stay external date strings, canonicalised to
   `oxidean_core::MCP_PROTOCOL_VERSIONS` and consumed by all five sites
   (`api/mcp.rs`, `cli/mcp.rs`, `web/wellknown.rs`, `webmcp.ts`, `mcp.tsrx` — the
   last two via the generated client's re-export).
6. `OXIDEAN_RPC_VERSION` env removed from config, `.env.example`, compose, and
   docs; live docs now say `Oxidean-RPC-Version: 0.1.0` with legacy `1` noted.

### Findings

| ID | Severity | Location | Finding | Resolution |
|---|---|---|---|---|
| U6-1 | Low | crates/oxidean-cli/src/mcp.rs | `unwrap_or_default()` would emit empty `protocolVersion` if the list were ever empty | Changed to `.expect("MCP_PROTOCOL_VERSIONS is non-empty")` — the list is statically non-empty |
| U6-2 | Low | apps/web/src/routes/admin/mcp.tsrx | `?? "2025-06-18"` fallback re-introduced a literal pin | Dropped; `.at(-1)` returning `undefined` simply omits the key (server picks newest) |
| U6-3 | Low | crates/oxidean-api/src/routes/cli_dist.rs | `schema_version` parsed but never checked (dead-code warning) | Now validated — non-1 schema marks manifest invalid → unpublished path |
| U6-4 | Note | versioning C5 contract | C5 draft said missing `latest.json` → `"0.0.0-dev"`; BR6.5/WF-4 say `null`/`[]` | Followed BR6.5/WF-4; flag for contract wording cleanup |

### Verification

- `cargo test -p oxidean-core -p oxidean-api -p oxidean-cli` targeted suites — pass (incl. 8 manifest tests, dual-accept header tests, cli_dist absent/corrupt/valid cases).
- `cargo test --test cli_dist` — 6 pass / 0 fail.
- `bun test packages/api-client` — 6 pass; `wireCompatWarning` + `MCP_PROTOCOL_VERSIONS` re-export verified.
- `make web-lint` — 0 errors; DOM-race, internal-anchor, route-sync (75↔76) all pass.
- `cargo clippy` on touched crates — clean.
- `make rpc-gen` — client regenerated; `rpc-sync-check` shape intact.
- `versioning: ok — 10 components registered (1 warning)` (pending baseline, expected).
- Traceability sensor: `missing_from_table` now empty; residual `missing_from_upstream_ids` lists only requirements owned by other units (expected per-unit scope).

### Summary

All Oxidean-owned wire surfaces now report the shared `0.1.0` SemVer, the legacy
ordinal `"1"` remains dual-accepted during the 0.x window, `/cli/latest` is honest
about unpublished distributions, and external MCP revision identifiers have one
canonical source. READY on current bytes.
