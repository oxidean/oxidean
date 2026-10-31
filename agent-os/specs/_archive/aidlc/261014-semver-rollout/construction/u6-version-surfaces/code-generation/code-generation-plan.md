# U6 Version Surface Wiring — Code Generation Plan

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "org",
  "ordering": "implement each applicable testable layer, then write and run that layer's tests.",
  "scope": "semver-release-automation",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14)"
    }
  ],
  "obligations": {
    "strategy": "standard",
    "strategy_volume": [
      "Five to eight tests per component.",
      "Unit tests plus integration tests for key boundaries.",
      "Add E2E, performance, or security tests when requirements demand them."
    ],
    "scope_floor": [
      "Keep the existing test suite green.",
      "This scope adds no extra new-test floor beyond the selected test strategy."
    ],
    "combination_rule": "Apply every selected-strategy obligation and every scope-floor obligation; neither replaces the other, and a targeted scope regression may add the narrowest necessary test type beyond the strategy default."
  },
  "plan_profile": {
    "methodology": "test-after",
    "runner_step": "Verify the existing test runner/configuration and record the exact unit-scoped command.",
    "runner_ready_before_first_test": true,
    "testable_layers": [
      "Data model / database behavior",
      "Repository / data access",
      "Business logic",
      "API / endpoint",
      "Frontend behavior"
    ],
    "steps": [
      "Project structure and production configuration skeleton.",
      "Verify the existing test runner/configuration and record the exact unit-scoped command.",
      "Data model / database behavior - implement.",
      "Data model / database behavior - write and run its tests after implementation.",
      "Repository / data access - implement.",
      "Repository / data access - write and run its tests after implementation.",
      "Business logic - implement.",
      "Business logic - write and run its tests after implementation.",
      "API / endpoint - implement.",
      "API / endpoint - write and run its tests after implementation.",
      "Frontend behavior - implement.",
      "Frontend behavior - write and run its tests after implementation.",
      "Environment/build configuration.",
      "Documentation and traceability."
    ]
  },
  "input_sha256": "sha256:269b5fe6c215f1c174eee59ab39199d705e1364320e780e6695b357b2faffa68",
  "contract_sha256": "sha256:3a38dc4eaaa9c318909bab63ee5df202dbcf2a67191fe3789eb17b5f2bd2ae2e"
}
```

## Scope

Make every external version surface tell the truth: the `wire` SemVer
constant + shared compat predicate, the dual-accept server header gate, the
four-field `system.manifest`, `/cli/latest` reading `latest.json` (never
the API crate version — FR6.3's latent bug), OpenAPI `info.version`, the
deduplicated MCP pin constant, and removal of the dead `OXIDEAN_RPC_VERSION`
config.

## Steps

1. `crates/oxidean-core/src/lib.rs` —
   - `MCP_PROTOCOL_VERSIONS: &[&str]` canonical constant (BR6.7).
   - `wire_compatible(a: &str, b: &str) -> bool` — caret SemVer: both <1.0 →
     same `0.MINOR`; both ≥1.0 → same MAJOR; mixed/unparseable → false
     (BR6.1). Owns its unit tests.
   - `ManifestResponse` gains `wire_version: String` (BR6.3).
2. `crates/oxidean-api/src/rpc.rs` —
   - `check_version_header`: `"1"` → legacy wire 0.1.x (accepted while
     `WIRE_VERSION` is 0.1.x — BR6.2 transition); else parse + caret-compare
     against `WIRE_VERSION`; missing → unchanged mismatch error.
   - `manifest_response` emits `wire_version` + frozen `protocol_version`.
3. `crates/oxidean-api/src/mcp.rs` — `PROTOCOL_VERSIONS` →
   `oxidean_core::MCP_PROTOCOL_VERSIONS`.
4. `crates/oxidean-web/src/wellknown.rs` — same collapse to the core const.
5. `crates/oxidean-api/src/rest/spec.rs` — `info.version` renders
   `WIRE_VERSION`; `make openapi-gen` refreshes `docs/openapi.yaml`.
6. `crates/oxidean-api/src/routes/cli_dist.rs` — `latest()` reads
   `latest.json` from the dist dir: emits its `version` field; absent or
   corrupt → `{version: null, targets: []}` + warn log (BR6.5). Never reads
   `CARGO_PKG_VERSION`. Response envelope shape unchanged.
7. `crates/oxidean-cli/src/rpc.rs` — sends `WIRE_VERSION` on the header.
8. `crates/oxidean-cli/src/manifest.rs` — `Manifest` gains
   `wire_version: Option<String>`; notices caret-compare `wire_version`
   when present, else `protocol_version==1` → wire 0.1.x (BR6.4 advisory);
   `min_cli_version` floor unchanged.
9. `crates/oxidean-cli/src/mcp.rs` — hardcoded `"2025-06-18"` →
   `MCP_PROTOCOL_VERSIONS.last()` from core.
10. `rpc_gen.rs` template — emit `WIRE_VERSION`, `MCP_PROTOCOL_VERSIONS`,
    a TS `wireCompatible()` twin, `wire_version` on `ManifestResponse`,
    and header send via `WIRE_VERSION`; regenerate `packages/api-client`.
11. `apps/web/src/lib/webmcp.ts` + `apps/web/src/routes/admin/mcp.tsrx` —
    import the emitted `MCP_PROTOCOL_VERSIONS` / use its last entry.
12. Delete `OXIDEAN_RPC_VERSION` from `.env.example` and
    `docs/CONFIGURATION.md` (BR6.8); update stale doc lines in
    `docs/CLI.md` about `/cli/latest` version source.
13. Tests — `wire_compatible` table (core), `check_version_header` dual-accept
    matrix (api), `latest()` missing/corrupt/present, client manifest
    BR6.4 paths (cli), generated-client surface assertions (TS test), MCP pin
    dedup test, openapi-gen refresh.

## Notes

- `cli.ts` untouched — commands self-register.
- `/cli/latest` absent/corrupt default is `{version: null, targets: []}`
  per BR6.5/WF-4 (the unit's binding rules); C5's earlier "0.0.0-dev" wording
  is superseded — both forbid the crate version, which is the contract's real
  invariant.
- `self_update.rs` already treats a null/missing `version` as a transport
  failure — an honest "unpublished" signal, matching BR6.5.
- u1/u3/u5 manifest overlap: edits to `Cargo.toml` lockfile, `Makefile`
  (none planned), and docs will re-stale earlier checkpoints — handle via the
  established re-review cycle.
