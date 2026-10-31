# Code Generation Plan — u9-vcs-mcp

Upstream design stages (functional-design / nfr-requirements / nfr-design / infrastructure-design) were workflow-skipped for u7–u10 under the floor-16 forward jump; this plan embeds the design decisions those stages would have produced, sourced from the frozen contract (`inception/contract-design/contract-summary.md` C3 + C1 consumes), `inception/units-generation/unit-of-work.md` §U9, `inception/requirements-analysis/requirements.md` (FR13.1–FR13.3), `inception/user-stories/stories.md` (US8.3: AC8.3.1–AC8.3.3), and `inception/refined-mockups/mockups.md` §12.

## Unit of work

`oxidean-vcs` gains `client::mcp` — `McpSurface` emitting 13 tool descriptors (`vcs_status`, `vcs_log`, `vcs_diff`, `vcs_tree`, `vcs_file_get`, `vcs_blame`, `vcs_path_history`, `vcs_merge_base`, `vcs_changeset_create`, `vcs_merge`, `vcs_rebase`, `vcs_ref_list`, `vcs_changeset_get`) as name/description/procedure/access-tag/input-schema tuples. `oxidean-api::mcp` wraps them: `tool_defs()` appends mapped `ToolDef`s (`Access::RepoRead`/`RepoWrite`), `build_rpc_input` injects `vcs.log`'s required `from` when absent. Deliberate absences stay absent (`vcs_repo_enable`, `vcs_ref_update`, `vcs_git_import`, `vcs_git_export`, `checkout`).

## Design decisions (embedded — the skipped design stages)

### DD-1 — Seam shape and dependency direction (AC8.3.3, FR13.3)

`oxidean-api::mcp::{ToolDef, Access}` are private host types; the dependency direction is host→crate, so the crate must not name them. `oxidean_vcs::client::mcp` therefore exports:

```rust
pub enum McpAccess { RepoRead, RepoWrite }
pub struct McpToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub procedure: &'static str, // "vcs.status" etc.
    pub access: McpAccess,
    pub input_schema: serde_json::Value,
}
pub fn tool_descriptors() -> Vec<McpToolDef>
```

Placement is `client::mcp` (default feature): descriptors are pure wire metadata — zero server deps — and any host surface (`oxidean-api` today) can consume them without pulling `oxidean-db`/`oxidean-git`. `mcp.rs` maps `McpAccess::{RepoRead,RepoWrite}` → `Access::{RepoRead,RepoWrite}` and wraps each descriptor in `ToolDef`.

### DD-2 — Tool set (FR13.1 + contract C3)

The FR13.1 dozen plus `vcs_changeset_get` (C3's resolved parked question — agents need point reads for lazy-import debugging). Deliberate absences per C3: `vcs_repo_enable` (admin), `vcs_ref_update` (CAS/ambiguous-write surface), `vcs_git_import`/`vcs_git_export` (operator flows), `checkout` (client-side disk op, not an RPC — agents materialize content through the read tools).

### DD-3 — Input schemas mirror the C1 wire DTOs

Every schema is `{type:object, properties:{owner,name,…}, required:[…]}` with separate `owner`/`name` — the exact fields `fg_repo_access_check` extracts (mockups.md §12, refined-mockups blocking note). Non-repo props mirror the wire DTO field names and `required` parity:

| tool | extra props (required in bold) |
|---|---|
| vcs_status | — |
| vcs_log | `from` (optional; schema-optional, inject `"main"` — DD-4), `limit`, `cursor` |
| vcs_diff | **`from`**, **`to`**, `paths` (string[]), `limit`, `cursor` |
| vcs_tree | **`changeset_id`**, `limit`, `cursor` |
| vcs_file_get | **`changeset_id`**, **`path`** |
| vcs_blame | **`path`**, `at`, `limit`, `cursor` |
| vcs_path_history | **`path`**, `limit`, `cursor` |
| vcs_merge_base | **`a`**, **`b`** |
| vcs_changeset_create | **`parents`** (string[]), **`message`**, **`changes`** (ManifestEntry[]: `{path, kind, blob, mode, size, content_b64?}` shape per ManifestEntry), `author` `{name,email}` |
| vcs_merge | **`a`**, **`b`**, `message` |
| vcs_rebase | **`changeset_id`**, **`new_parent_id`**, `message` |
| vcs_ref_list | `limit`, `cursor` |
| vcs_changeset_get | **`id`** |

### DD-4 — `build_rpc_input` defaulting (the `repo_tree` convention)

Vcs tools pass through verbatim via the default arm — args arrive wire-shaped. One exception mirroring `repo_tree`'s `""`-injection pattern: `vcs.log` requires `from` (`String`, non-Option on the wire) while the tool schema keeps it optional for agent ergonomics, so a `vcs_log` arm injects `"from": "main"` when absent — the same default the CLI sends.

### DD-5 — Access classes and error mapping (AC8.3.2, FR13.2)

Access classes mirror C1's `proc_classes` tiers: read tier (status, changeset_get, log, diff, tree, file_get, blame, path_history, merge_base, ref_list) → `RepoRead`; write tier (changeset_create, merge, rebase) → `RepoWrite`. `enforce_pat_scope`, `OXIDEAN_MCP_ENABLED`/`admin.mcp.*` overrides, and the `tool_error`/`isError` domain-denial mapping are already generic over `ToolDef` — **no host logic changes**; tests assert the classes land.

## Steps (plan_profile ordering)

1. **Skeleton + runner**: `crates/oxidean-vcs/src/client/mcp.rs` module; verify `cargo nextest run -p oxidean-vcs` runs its tests (client feature default).
2. **`McpSurface`** — `client/mcp.rs` descriptors + crate-side tests (13 tools, name↔procedure mapping, owner/name required, C1 required-field parity, access-tier split, deliberate absences).
3. **`oxidean-api` wiring** — `mcp.rs` `tool_defs()` extension + `build_rpc_input` `vcs_log` arm.
4. **Host tests** — `tool_defs()` includes the 13; per-tool `Access` assertions; `build_rpc_input` passthrough + `from` injection; `tool_public` shape.
5. **Docs/traceability** — `code-summary.md`, `traceability.json`, `source-manifest.json`.

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "classic",
  "test_strategy": "standard",
  "project_type": "brownfield",
  "applicable_notes": [
    {
      "layer": "org",
      "text": "We treat tests as a first-class deliverable in every Bolt. The specific\nmethodology (TDD, BDD, ATDD, or classic test-after) is affirmed at\npractices-discovery and recorded in `team.md` under this heading with explicit\n`Methodology` and `Ordering` fields; Code Generation resolves those fields\nindependently from coverage, tooling, and scope notes.\n\nWhen no posture has been affirmed, our default per scope is:\n- **Methodology**: test-after\n- **Ordering**: implement each applicable testable layer, then write and run\n  that layer's tests.\n- `mvp`, `enterprise`, `feature`, `infra`, `classic` add an 80% line-coverage\n  floor and CI execution before merge.\n- `bugfix`, `security-patch` add a targeted regression for the specific\n  bug/vulnerability and require the existing suite to remain green.\n- `express` uses the Minimal strategy: requirement-driven unit tests (one per\n  requirement, with a happy-path floor per component); existing tests remain\n  green.\n- `poc`, `refactor`, `workshop` add no extra new-test floor and require the\n  existing suite to remain green.\n\nThe active `Test Strategy` still applies in every scope and determines test\nvolume/types. Scope floors are additive; they never reduce or replace the\nselected strategy.\n\nBuild and Test verifies defined coverage floors and affirmed quality targets;\nthey may not be weakened to make a step pass.\n\nAffirm a stricter posture in `team.md` if the team commits to one."
    },
    {
      "layer": "team",
      "text": "- **Methodology**: test-after\n- **Ordering**: We implement the change first, then write and run the tests\n  the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing\n  expectations), and the full CI suite must be green before the PR can merge.\n- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists and\n  may be invoked deliberately for a unit, but red-first is not the team's\n  mandated posture.\n- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**\n  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by\n  `scripts/coverage-weighted.sh`): web unit 0.25 + web integration 0.40 (v8\n  line coverage) + e2e 0.35 — where the e2e third is a **7-path presence\n  checklist** (`scripts/coverage-e2e-checklist.sh`), not executed coverage;\n  e2e correctness is enforced by the separate `e2e-stack` / `actions-e2e` /\n  `smoke-protocol` job gates. The composite **consumes web numbers only**:\n  Rust coverage is collected (`cargo-llvm-cov`, artifacts uploaded) but\n  **not gated**, so this intent's new VCS crate lands with no enforced\n  coverage signal — collect-only Rust coverage is recorded debt, and we do\n  not invent a new numeric floor for it (the change-type minimums + green CI\n  carry its testing). Cross-dialect execution in CI is limited to\n  `dialect_probe` per dialect (`db-dialects` job): the ~30 `dialect_*` parity\n  files are `DATABASE_URL`-gated and **skip-pass green** in the `api-rust`\n  shards — full parity is local-capable only, also recorded debt. No\n  performance-test infrastructure exists (no benches, no load harness) —\n  recorded debt for NFR work, not a blocker.\n- **UI coverage is change-aware**: route- and browser-coverage manifests mean\n  every new/changed `apps/web` route needs test evidence, and new/changed\n  high-risk interactive UI (Checkbox / RadioGroup / Select / Switch /\n  portals / `form.Subscribe`) needs Chromium `*.browser.test.tsx` proof —\n  fresh skip-only rows fail CI, and `kind: \"browser\"` entries are\n  source-verified (mount, interact, assert markers). Manifest `skip` rows are\n  the only sanctioned skip: zero `#[ignore]` and zero `it.skip`/`test.skip`\n  in the tree.\n- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,\n  `ci` profile: no fail-fast, 90s slow-timeout; one shared nextest archive\n  from `rust-build` feeds the 5-way `api-rust` shards, `db-dialects`, and\n  archive consumers — one compile per run; shards re-run whole on failure —\n  coarse flake tolerance). Vitest 5 for TS with five projects in `apps/web`\n  (`unit` on node, `integration` on happy-dom with `fileParallelism: false`,\n  `browser` on Playwright Chromium, `e2e-stack` and `e2e-stack-browser` gated\n  on `E2E_STACK=1`) plus `@oxidean/api-client`'s own node-env suite.\n  Playwright drives real-Chromium component and full-stack e2e; wiremock\n  supplies outbound-HTTP fixtures; Rust integration tests open real SQLite\n  tempdir DBs; protocol smokes run real `git`, `ssh`, and `git-lfs` client\n  binaries against a live Compose stack (fail-closed in CI under\n  `SMOKE_REQUIRE_STACK=1`).\n- **Local/CI asymmetry**: `make test` runs only nextest + web\n  unit/integration; browser, stack e2e, and smokes are separate targets — a\n  green local `make test` proves far less than a green CI run, and smoke\n  scripts skip-pass locally without Docker but fail-closed under `CI`.\n- **Test types / minimums**: per change type — pure helper → unit;\n  Query/session UI → happy-dom integration; high-risk UI → Chromium browser\n  test + manifest row; auth/RPC contract → Rust integration and/or stack e2e;\n  dialect-touching DB code → `dialect_*` test (noting the CI-execution gap —\n  the probe is the only enforced cross-dialect run). Suites: colocated Rust\n  units plus `crates/*/tests/` integration (107 API test files), web\n  unit/integration/component tests, stack e2e with Mailpit/OIDC/Resend-WorkOS\n  stubs (`make test-e2e-stack`), forge protocol smokes, compose bring-up\n  smoke per dialect, and an Actions pipeline e2e."
    },
    {
      "layer": "project",
      "text": "- CI/Docker-related changes are verified locally with docker + act before shipping; deadlines are ASAP-but-quality-gated — solid tests are required, not optional (learned 2026-10-14) \n\n- Prefer Docker for debugging and testing (docker compose stack, make up/smoke, act for CI lanes) over bare-metal equivalents (learned 2026-10-18)"
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
  "input_sha256": "sha256:304bf27698e8b5f47dff4e16702de3e3c6f286e25e63e22facb420316bac75bc",
  "contract_sha256": "sha256:d7627c3e92082e54b594f33a1ba945e0e072964270fa08e0adcb8fe891ef16c6"
}
```

## Test matrix (see unit-test-instructions.md for commands)

| Row | Component | Coverage |
|-----|-----------|----------|
| mcp_surface | `tool_descriptors` | 13 tools exactly; `vcs_*` name ↔ `vcs.*` procedure mapping; every schema requires `owner`+`name`; required-field parity with the C1 wire DTOs; deliberate absences (repo_enable/ref_update/git_import/git_export/checkout) not present; access split — write tier exactly {changeset_create, merge, rebase} |
| host_wiring | `tool_defs`/`build_rpc_input` | 13 vcs tools in `tool_defs()` with mapped `Access`; passthrough input for a read + a write tool; `vcs_log` `from` default injection; `tool_public` shape (name/description/inputSchema) |
| dependency | structural | descriptors carry no `oxidean-api` types (`McpAccess` crate-local); `client::mcp` compiles client-only |

`traceability.json` maps deliverables to C3, FR13.1–FR13.3, AC8.3.1–AC8.3.3, and C1 `proc_classes` tier mirroring.
