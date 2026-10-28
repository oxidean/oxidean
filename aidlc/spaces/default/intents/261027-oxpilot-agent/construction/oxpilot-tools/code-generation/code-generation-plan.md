# Code Generation Plan — oxpilot-tools (U5)

Unit: `oxpilot-tools` (US2.1/US2.2/US2.3 proposals + explicit-confirm
materialization, US3.1/US3.2 Actions read surface + fix proposals,
US4.3 exec/fs dispatch through the gateway, US5.3 audit split,
US5.5 permissions + registration). Implements the approved functional
design (`functional-design/` entities + rules + spec — the 23-entry
closed `CATALOG`, `OxpilotTools` executor, `propose_diff`/`proposal_get`,
ArtifactScope-bound `pr_comment`, the `oxpilot.diff_{list,get,apply}`
user-proc trio, exec/fs via `dispatch_action` only), the NFR design
(`nfr-design/` — the `tools/{mod,registry,validate,read,propose,dispatch,
comment,procs}.rs` module map in `oxidean-api-oxpilot`, zero new deps,
hand-authored schema walker), and the infrastructure spec
(`infrastructure-design/` — additive `0089_oxpilot_proposals_ops.sql` ×3
dialects, `install_tools` beside `install_gateway` at boot, no new CI
jobs).

Consumes `inception/contract-design/contract-summary.md` C1
(`oxpilot.diff_*` payload/error vocabulary), C5 (`save_proposal` /
`proposal` / `mark_applied` service verbs + `get_session`), C6
(`ToolExecutor{catalog,invoke}` + shipped `ToolSpec`/`ToolCtx`/
`ToolOutput`/`ToolError`/`NullTools`), C7 (`dispatch_action`/
`action_status`/`list_targets` + the `data.denied` marker contract).

## Testing Contract

```json
{
  "version": 1,
  "methodology": "test-after",
  "source": "team",
  "ordering": "We implement the change first, then write and run the tests the change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing expectations), and the full CI suite must be green before the PR can merge.",
  "scope": "feature",
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
      "Meet an 80% line-coverage floor.",
      "Run the selected tests in CI before merge."
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
  "input_sha256": "sha256:7ae78f2c2f75e8446714d34b7e23c2369d30af0622bdcafe3bba1f8d40a110e3",
  "contract_sha256": "sha256:2f7938d58ac724d699d8c1731ce63148c2703bdae96beda4ac1aef80f9aaec31"
}
```

## Story-to-code traceability

| Plan step | Stories / ACs served |
|---|---|
| 3-4 (ops_json chain + db verbs) | US2.1 (AC2.1.1), US2.3, AC5.5.1 — persisted proposal shape the apply path executes verbatim |
| 5 (registry + validate) | US5.5 (AC5.5.1/5.5.2/5.5.4), FR7.1-7.5, NFR9 — closed catalog, provider-legal names, schema gate |
| 6 (mod.rs dispatch ladder) | BR1.3/BR1.4/BR2.1/BR2.3/BR2.4 — unknown_tool, invalid_args, session re-resolve |
| 7 (read.rs) | US3.1 (AC3.1.1-3.1.3), US5.5 — repo/issue/pull + six Actions reads via rpc_dispatch |
| 8 (propose.rs) | US2.1/US2.3 (AC2.1.1/2.1.2), AC2.3.1 — pending insert, diff↔ops coherence, diff card, session-pinned get |
| 9 (dispatch.rs) | US4.3 (AC4.3.1-4.3.5), FR5.3 — target resolution, denied-marker mapping, async status |
| 10 (comment.rs) | FR7.5 (AC5.5.2), US5.3 — ArtifactScope + pull.comments.create |
| 11 (procs.rs + wiring) | US2.2 (AC2.2.1-2.2.4), FR3.2/FR3.3 — user-only apply, provenance line, pr_ref |
| 12-13 (tests) | all of the above — per change-type minimums |

## Plan steps (test-after ordering)

- [x] **Step 1 — Module skeleton + wiring points.** Create
  `crates/oxidean-api-oxpilot/src/tools/` with the eight files from the
  NFR module map (`mod.rs`, `registry.rs`, `validate.rs`, `read.rs`,
  `propose.rs`, `dispatch.rs`, `comment.rs`, `procs.rs`) as compilable
  stubs; `pub mod tools` in `lib.rs`; `install_tools(state)` helper
  beside `install_gateway` setting `OxpilotRuntimeSlot.tools =
  Some(Arc::new(OxpilotTools::new(state.clone())))` at shell assembly
  in `crates/oxidean-api/src/main.rs`. No new dependencies (TS-9).
- [x] **Step 2 — Verify test runner.** Confirm the unit-scoped commands
  in `unit-test-instructions.md` run green on the unmodified tree
  (`cargo nextest run -p oxidean-api-oxpilot`,
  `cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)' -j 1`
  with a file `DATABASE_URL`, `cargo nextest run -p oxidean-api --test oxpilot`).
- [x] **Step 3 — Data model: `ops_json` additive chain (implements
  US2.1 persistence).** New migration `0089_oxpilot_proposals_ops.sql`
  in all three dialects (`sqlite`, `postgres`, `mysql`) —
  `ALTER TABLE oxpilot_proposals ADD COLUMN ops_json ... NOT NULL
  DEFAULT ...` per dialect convention (0087 is never edited); then the
  db layer: `NewOxpilotProposal.ops_json`, `OxpilotProposalRow.ops_json`,
  `insert_oxpilot_proposal` column+bind list, `PROPOSAL_SELECT_{PG,
  MYSQL,SQLITE}` column lists, `map_proposal!`; new read verb
  `find_oxpilot_proposals_by_session(session_id)` ×3 dialects (newest-
  first); core `ProposalWrite.ops_json` + `save_proposal` signature/
  impl pass-through in `services.rs` (redaction of `title`/`diff_body`
  stays inside the service).
- [x] **Step 4 — Data model tests.** Extend
  `crates/oxidean-db/tests/dialect_oxpilot.rs`: ops_json round-trip on
  insert/select for all three dialect shapes, `find_oxpilot_proposals_
  by_session` ordering + session scoping, backfill default on a row
  inserted pre-column shape if the dialect allows. 5-8 tests.
- [x] **Step 5 — Business logic A: `registry.rs` + `validate.rs`.**
  `CatalogEntry{spec, class, handler}` + `EntryKind` + static
  `CATALOG: &[CatalogEntry]` — the 23 pinned names in fixed order
  (BR1.1/BR1.2, write class = {propose_diff, pr_comment, exec_shell,
  fs_read, fs_write}); per-tool `parameters_json` schemas closed
  (`additionalProperties: false`) with BR3.3/BR4.4/BR7.2/BR8.3 bounds.
  `validate.rs`: the hand-authored schema walker (`type`, `required`,
  `properties` closed, `enum`, `minLength`/`maxLength`, `minimum`/
  `maximum`, `pattern` via the closed matcher table, `items`) — no
  `jsonschema`/`regex` deps; violations yield `FieldErr` naming the
  field.
- [x] **Step 6 — Business logic B: `mod.rs` dispatch ladder.**
  `OxpilotTools{state: Arc<AppState>}`; `impl ToolExecutor`: `catalog()`
  returns spec halves verbatim in registry order; `invoke(call, ctx)`:
  registry lookup ⇒ `unknown_tool` → `validate` ⇒ `invalid_args`
  naming field → `svc.get_session(ctx.user_id, ctx.session_id)` ⇒
  `not_found` → handler by class. Shared helpers: `resolve`,
  `emit_card` (turns().get → `emit_frame`), `audit`
  (`record_with` + `find_user_by_id` actor, swallowed failure),
  `bounded` (TOOL_OUTPUT_MAX/OUTPUT_SUMMARY_MAX + `truncated` marker +
  `oxidean_core::redact`), `dispatch_proc` (synthesize RpcCtx via
  `build_rpc_ctx_with_session` + `ClientMeta::default()`, verbatim
  AppError→ToolError code map, internal ⇒ `internal`).
- [x] **Step 7 — Business logic C: `read.rs` (15 read handlers).**
  `repo_tree`/`repo_file_read`/`repo_log`/`repo_commit_get`/`repo_refs`,
  `issue_list`/`issue_get`, `pull_list`/`pull_get`, six Actions reads —
  each an `EntryKind::Proc` literal binding through `dispatch_proc`
  with `{owner, name}` injected from the RESOLVED session's repo
  binding (never args), `ref`/`branch` narrowing only inside the repo
  defaulting to the session branch; `actions_get_job_log` carries
  `{content(tail-bounded), next_offset, done}` and never sends
  `wait_ms`. No `repo_get` catalog entry (BR3.2's `repo.get` is
  allowlisted-but-undispatched in v1).
- [x] **Step 8 — Business logic D: `propose.rs`.** `propose_diff`:
  bounds check (title ≤200, files ≤50, diff_body ≤256 KiB, summed
  decoded content_b64 ≤256 KiB), per-op rules (rename⇒new_path,
  create/update⇒content_b64), diff↔ops path-set coherence ⇒
  `invalid_args` naming disagreeing paths; `svc.save_proposal` insert
  (status pending, pr_ref null — unconditional); `emit_card` diff card
  post-commit; `oxpilot.proposal_created` audit; ToolOutput
  "proposal <id> created — pending review". `proposal_get`:
  `svc.proposal` + post-fetch `row.session_id == ctx.session_id` pin ⇒
  `not_found`; `files[]` derived from `diff --git` headers.
- [x] **Step 9 — Business logic E: `dispatch.rs`.**
  `exec_shell`/`fs_read`/`fs_write`: schema-gated args (≤2 KiB
  serialized), target resolution (arg ⇒ verbatim; absent ⇒
  `list_targets` filter `lifecycle == "connected"`: 0⇒`no_target`,
  >1⇒`ambiguous_target` with guidance in message), `dispatch_action(
  &ClientMeta::default(), user_id, session_id, target_id, kind=
  entry.spec.name, args)`; outcome map — success ⇒ `{action_id, state:
  queued}` ToolOutput; `oxpilot.action_denied`+`data.denied`:
  `consent_required` ⇒ consent card + success-shaped ToolOutput
  ("do not re-issue" guidance), `not_connected|offline|capability` ⇒
  `ToolError{action_denied}` carrying the reason in message, missing/
  malformed marker ⇒ `ToolError{internal}` fail-closed; other AppError
  ⇒ verbatim code. `targets_list`, `action_status` read-class over the
  same client-first gateway verbs. No raw-arg retention (BR7.7).
- [x] **Step 10 — Business logic F: `comment.rs`.** `pr_comment`:
  ArtifactScope rebuild via `find_oxpilot_proposals_by_session` →
  applied rows' `pr_ref` set; `owner/name#<number>` ∉ scope ⇒
  `ToolError{forbidden}`; `body` non-empty ≤4 KiB + `redact` pre-
  dispatch; `pull.comments.create{owner, name, number, body}` through
  `dispatch_proc` unanchored; `oxpilot.artifact_comment` audit post-
  commit.
- [x] **Step 11 — API layer: `procs.rs` + registration.** USER-only
  `oxpilot.diff_list` (owned session → `find_oxpilot_proposals_by_
  session` → `{diffs:[ProposedDiff]}` newest-first), `oxpilot.diff_get`
  (`svc.proposal` → wire shape + `pr_ref`), `oxpilot.diff_apply`
  (require_enabled → require_verified → `svc.proposal` → pending check
  ⇒ `diff_not_pending` → CALLER `ctx.rpc_dispatch` chain:
  `repo.branchCreate{branch: "oxpilot/proposal-<id8>", start: base}` ⇒
  `apply_failed:branch_exists` on exists → ordered ops via
  `repo.file.upload` batched create/update (≤50/call) +
  `repo.file.delete`/`rename` per op ⇒ `apply_failed:<proc>` →
  `pull.create{base_ref, head_ref, title, body: summary + provenance
  line}` → `svc.mark_applied` → `oxpilot.diff_applied` audit →
  `{proposal_id, pr_ref}`; never push/merge/default-branch). Register
  the three names in `PROCEDURES` + three dispatch arms in the crate
  `dispatch.rs` (SessionOnly, `require_enabled`, `require_verified` on
  the apply write — the U6 `oxpilot.connector.*` precedent); `make
  rpc-gen` + verify `make rpc-sync-check`.
- [x] **Step 12 — API/business tests (5-8/component).** Registry/
  validate pin tests (catalog closed+ordered, name charset, schema
  rejection per bound, `additionalProperties` on identity fields);
  invoke ladder (unknown_tool/invalid_args/not_found ordering);
  read-handler allowlist + injected owner/name + verbatim error map;
  propose (coherence reject naming paths, bounds, pending-only insert,
  card emit, audit row, session-pin get); dispatch (target resolution
  arms, denied-marker matrix, malformed marker ⇒ internal); comment
  (scope reject, redact applied); diff procs (list ordering, get
  shape, apply ordering/fail-stop/double-apply ⇒ diff_not_pending).
  Colocated `#[cfg(test)]` + proc integration in
  `crates/oxidean-api/tests/oxpilot.rs` + `procedures_list_matches_
  dispatch` extension to the three new names; dialect tests from
  Step 4.
- [x] **Step 13 — Env/build config.** Cargo.toml unchanged (zero new
  deps — serde_json/oxidean_core/audit already reachable); confirm
  `cargo check -p oxidean-api-oxpilot --tests` clean; `cargo fmt`.
- [x] **Step 14 — Documentation + traceability.** `code-summary.md`,
  `source-manifest.json` (every touched path incl. migrations, PROCEDURES,
  main.rs, generated api-client), `traceability.json` (all AC/NFR/BR
  ids → implementation or test file). Frontend layer omitted — U8 owns
  all `/oxpilot` UI; this unit ships no web surface.

## Notes

- Application code lands in `crates/` at workspace root — never under
  this record dir.
- Measurable targets (NFR1/NFR2/NFR5/NFR9, coverage floor, U3/U6 pins)
  are inputs, not suggestions; surface gaps rather than weakening them.
