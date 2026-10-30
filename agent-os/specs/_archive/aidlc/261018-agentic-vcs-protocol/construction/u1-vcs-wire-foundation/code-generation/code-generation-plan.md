# Code Generation Plan — u1-vcs-wire-foundation

Implements the wire foundation per functional-design/nfr-design artifacts. Test strategy: standard (unit tests per component + boundary assertions). All shapes pinned by contract-summary C1 + entities.md; mechanism pins from nfr-design LC1–LC6.

## Steps

### 1. `crates/oxidean-core/src/vcs_types.rs` (new module — LC1)
- [ ] Wire helpers: `pub enum Nullable<T>` (`Null`/`Value(T)`, custom Deserialize, untagged-style Serialize emitting bare null/value, `is_null()`/`value()`; field-level `#[schemars(with = "Option<T>")]` on CAS fields); `ChangesetId` validated newtype (`cs_<ulid>` shape); `ObjectId` (`sha256:`-prefixed hex). *(AC8.1.x)*
- [ ] `VcsErrorCode`: closed enum, per-variant `#[serde(rename = "vcs.<code>")]` — the 11 codes (disabled, not_found, invalid_input, ref_conflict, conflict, no_common_ancestor, quota_exceeded, too_large, import_state, unauthenticated, forbidden); `as_str()`; `impl From<VcsErrorCode> for String`. *(AC8.1.4)*
- [ ] Shared shapes: `VcsChangesetPublic`, `VcsRefPublic`, `ManifestEntry`, `ErrorData`/typed error payloads, `VcsHeadPublic`, pagination fields (`limit`, `cursor` — per-DTO, `Option` + `skip_serializing_if`). *(AC8.1.1–8.1.3)*
- [ ] 17 `Vcs{Op}Request`/`Vcs{Op}Response` pairs — exact C1 signatures: repo_enable `{owner,name,enabled}→{enabled}`; status `{owner,name}→{enabled,heads,changeset_count,object_count,storage_bytes}`; changeset_create `{owner,name,parents,message,author?,changes:[ManifestEntry]}→{changeset_id,manifest_hash}`; changeset_get `{owner,name,id}→{changeset_id,parents,author,message,committed_at,manifest}`; log `{owner,name,from,limit?,cursor?}→{entries,cursor?}`; tree `{owner,name,changeset_id,path_prefix?,limit?,cursor?}→{entries:[{path,mode,blob}],cursor?}`; diff `{owner,name,from,to,paths?,limit?,cursor?}→{entries:[{path,kind,old_mode?,new_mode?,old_blob?,new_blob?,patch?}],cursor?,totals}`; blame `{owner,name,path,at?,limit?,cursor?}→{entries:[{changeset_id,author,content}],cursor?}`; path_history `{owner,name,path,limit?,cursor?}→{entries:[{changeset_id,kind,path,committed_at}],cursor?}`; merge_base `{owner,name,a,b}→{merge_base}`; file_get `{owner,name,changeset_id,path}→{blob,mode,size,content_b64|null,too_large}`; merge `{owner,name,a,b,message?}→{changeset_id,fast_forward?}`; rebase `{owner,name,changeset_id,new_parent_id,message?}→{head}`; ref_list `{owner,name,limit?,cursor?}→{entries:[VcsRefPublic],cursor?}`; ref_update `{owner,name,ref:serde-rename,expected_old:Nullable,new:Nullable}→{ref,changeset_id|null}`; git_import `{owner,name,from?,cursor?,limit?}→{imported,cursor?,done,heads}`; git_export `{owner,name,head,branch,force?}→{branch,git_oid}`. *(AC8.1.1–8.1.3, AC8.2.1)*
- [ ] Seven `vcs.*` activity-kind constants (`VCS_ACTIVITY_*` or `pub const` strings). *(AC2.6.1 share)*
- [ ] Re-export from `lib.rs` (`pub mod vcs_types; pub use vcs_types::*`).

### 2. `proc_classes.rs` `vcs.*` block (LC2)
- [ ] 17 procedure arms + 2 alias arms as literal `"vcs.x" =>` lines: RepoRead ×10, RepoWrite ×6, RepoAdminWrite (`vcs.repo_enable`), aliases `vcs.object_get`→RepoRead, `vcs.object_put`→RepoWrite. Unlisted stays `SessionOnly` fallthrough. *(AC8.6.1)*

### 3. `oxidean-api` registration surfaces (LC5)
- [ ] `PROCEDURES` gains the 17 names; dispatch gains **17 separate single-name `"vcs.x" =>` arms** each returning `AppError` `vcs.disabled` (parity-scanner shape pinned). *(AC8.1.2)*
- [ ] `rpc_gen.rs` template: 17 client methods + generated `index.ts` types; keep `WIRE_VERSION` literal equal to `oxidean_core::WIRE_VERSION`; run `make rpc-gen`, commit output. *(AC8.2.2)*

### 4. `RepoActivityItem` widening (LC3)
- [ ] `repo_types.rs`: five fields → plain `Option` (NO `skip_serializing_if`); `repo/activity.rs` `row_to_item` wraps `Some(...)`; `apps/web` `repo-activity-row.tsrx` null guards (`?? item.ref_name`, `after != null`, `commits_count ?? 0`). *(AC2.6.1 share)*
- [ ] `wire`-scoped breaking commit for the widening (`feat(wire)!:` shape — wire bumps 0.1→0.2; `globs=[]` so scope drives attribution).

### 5. `crates/oxidean-vcs` skeleton + versioning (LC4)
- [ ] `Cargo.toml` (literal `version = "0.1.0"`, workspace-inherited edition/license, zero deps), `src/lib.rs`, `CHANGELOG.md`; root `Cargo.toml` `members`; `versioning.toml` `[[component]]` (`id="oxidean-vcs"`, `kind="crate"`, `manifest_path`, `tag_prefix="oxidean-vcs-v"`, `changelog_path`, `globs=["crates/oxidean-vcs/**"]`, `scopes=["vcs"]`, `depends_on=[]`); `Cargo.lock` member delta in the same tree. *(AC8.2.2)*

### 6. Tests (LC6 — mandatory)
- [ ] proc_classes coverage: all 17 PROCEDURES-listed vcs.* → non-SessionOnly + dispatch arm; aliases classified & absent from PROCEDURES.
- [ ] Serde round-trips: `Nullable` absent→missing_field / null→Null / value→Value; `VcsErrorCode` dotted strings; `ref` field rename; `Option` absent-not-null; widened `RepoActivityItem` emits explicit null.
- [ ] Generated-client coverage: all 17 names in `packages/api-client/index.ts`.
- [ ] `WIRE_VERSION` sync assertion (template literal == const) — interpolate or string-compare.

## Verification
`cargo nextest` on touched crates, `make rpc-sync-check`, `make version-check`, `make web-lint`/`web-format-check`, `make dead-code-check`, `cargo fmt --check`, rustdoc-clean.

## Story traceability
Steps 1–3 → US8.1/8.2/8.6 (+ shared AC1.1.2 wire envelope); step 4 → AC2.6.1/AC5.1.1 shares; step 5 → AC8.2.2; step 6 → NFR5/NFR9/NFR10.

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
