# Code Generation Plan — agent-runtime (U3)

Unit: `agent-runtime` (US1.4 agent turn loop, US1.6 cancel, US1.7
stream attach; contributes US1.1/1.2 chat UX, US5.x chain/budget
triggers). Implements the approved functional design
(`functional-design/` — BR1–BR7 + entities + WF1–WF6), the NFR design
(`nfr-design/` — admission-lock pair accessor, `emit_terminal` CAS,
wrapper-owned release, `turn_parts()` triple, C6 trait surface), and
the infrastructure spec (`infrastructure-design/` — no new jobs,
existing lanes absorb tests).

Consumes `inception/contract-design/contract-summary.md` C1
(`oxpilot.turn_cancel` `{session_id}→{aborted:bool}`,
`no_active_turn`), C2 (SSE frame vocabulary + attach order), C4
(provider trait + `ChatMsg` tool-result amendment), C5 (SessionService
surface), C6 (`ToolExecutor`/`ToolCtx`/`ToolOutput`/`ToolSpec` —
PROVIDED by U3, consumed by U5), C8 (abort-hook ordering).

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
      "text": "- **Methodology**: test-after\n- **Ordering**: We implement the change first, then write and run the\n  tests the change-type minimum requires (`docs/CODE_PRACTICES.md`\n  § Testing expectations), and the full CI suite must be green before\n  the PR can merge.\n- **TDD option**: an in-repo `tdd` skill (`.agents/skills/tdd/`) exists\n  and may be invoked deliberately for a unit, but red-first is not the\n  team's mandated posture.\n- **Coverage**: CI enforces a **weighted composite gate at floor 0.70**\n  (`coverage-weighted` job, `COVERAGE_WEIGHTED_FLOOR`, composed by\n  `scripts/coverage-weighted.sh` — itself contract-tested by\n  `coverage-weighted.contract.sh` via `make coverage-contract`):\n  web unit 0.25 + web integration 0.40 (v8 line coverage) + e2e 0.35 —\n  where the e2e third is a **frozen 7-path presence checklist**\n  (`scripts/coverage-e2e-checklist.sh` checks `[[ -f ]]`: 2 stack-browser\n  + 2 stack + 3 protocol-smoke paths), not executed coverage; e2e\n  correctness is enforced by the separate `e2e-stack` / `actions-e2e` /\n  `smoke-protocol` job gates. Adding a checklist item grows a hard path\n  dependency for every future PR — an `/oxpilot` stack test lands in\n  job-gates unless we deliberately grow the denominator. The composite\n  **consumes web numbers only**: Rust coverage is collected\n  (`cargo-llvm-cov`, artifacts uploaded) but **not gated** — affirmed for\n  this intent (Q3): `oxidean-api-oxpilot` lands **collect-only** with no\n  new numeric floor; the change-type minimums + green CI carry its\n  testing, and we record that decision consciously rather than\n  normalizing crate-level coverage blindness. Cross-dialect execution in\n  CI is the `db-dialects` job, which runs **three `dialect_*` binaries**\n  per dialect — `dialect_probe`, `dialect_vcs`, `dialect_actions` — plus a\n  `workflow_conformance` leg replayed on real Postgres **and** MySQL with\n  per-test provisioned `oxt_<uuid>` databases via\n  `OXIDEAN_TEST_DATABASE_URL` (unset falls back to sqlite pass-through).\n  The parity story is narrower than folklore says: of the **23**\n  `dialect_*` files in `crates/oxidean-db/tests/`, only **4** gate on\n  `DATABASE_URL` (`dialect_probe`, `dialect_auth`, `dialect_vcs`,\n  `dialect_actions`); the other ~19 are sqlite-only migration/schema\n  assertions that execute for real in the `api-rust` shards.\n  **`dialect_auth` is dead coverage**: it gates on `DATABASE_URL` but is\n  absent from the `db-dialects` `-E` filter — it has never run against\n  Postgres/MySQL in CI (housekeeping: join the filter or document the\n  exclusion). If this intent adds `oxidean-db` tables, a\n  `dialect_oxpilot.rs` must join the `-E` filter **and** the TESTING.md\n  job table in the same change or it is dead-on-arrival (same job — no\n  ruleset change needed). No performance-test infrastructure exists (no\n  benches, no load harness) — recorded debt for NFR work; note SSE\n  streams, LLM round-trips, and computer-use sessions are\n  latency/long-lived surfaces the suite cannot measure, so NFR planning\n  should at least define smoke-level expectations (stream first-byte,\n  session teardown) rather than invent a harness.\n- **UI coverage is change-aware**: route- and browser-coverage manifests\n  mean every new/changed `apps/web` route needs test evidence (the\n  route-coverage manifest carries **82** hand-maintained rows), and\n  new/changed high-risk interactive UI (Checkbox / RadioGroup / Select /\n  Switch / portals / `form.Subscribe`) needs Chromium\n  `*.browser.test.tsx` proof — fresh skip-only rows fail CI, and\n  `kind: \"browser\"` entries are source-verified (mount, interact, assert\n  markers). Manifest `skip` rows are the only sanctioned skip: zero\n  `#[ignore]` and zero `it.skip`/`test.skip`/`describe.skip` in the tree\n  (re-verified by tree scan). For `/oxpilot` specifically the same-change\n  registration set is bigger than one manifest row: a `route-coverage`\n  row with real happy-dom mount or stack-browser evidence (skip-only\n  fails for added routes; file existence is enforced, a mount is\n  doc-convention), the `check-route-sync` triangle (`oxidean-web`\n  `shells.rs` `SHELLS` + an Astro page emitting the dist shell +\n  `route-params.ts` `RESERVED_TOP_SEGMENTS` — a new top-level segment\n  omitted there misbinds as `/{owner}`), a `browser-coverage` row if the\n  UI uses high-risk controls (a model picker or dialog almost certainly\n  does), and the generated `routeTree` sync. The `settings/ai.tsrx`\n  route — manifest row + happy-dom test — is the precedent shape for\n  oxpilot provider settings.\n- **Tooling**: cargo-nextest for the Rust workspace (`.config/nextest.toml`,\n  `ci` profile: no fail-fast, 90s slow-timeout, **`retries = 1`** — a\n  bounded per-test retry covering lanes with no shell retry such as\n  `coverage-weighted` and `db-dialects`; one shared nextest archive from\n  `rust-build` — built `--workspace --features server` so the\n  feature-gated vcs suite exists in it — feeds the 5-way `api-rust`\n  shards, `db-dialects`, `vcs-server`, and archive consumers, one compile\n  per run; `api-rust` shards additionally re-run whole on failure via a\n  shell-level retry — coarse flake tolerance). Vitest 5 for TS with five\n  projects in `apps/web` (`unit` on node, `integration` on happy-dom with\n  `fileParallelism: false`, `browser` on Playwright Chromium,\n  `e2e-stack` and `e2e-stack-browser` gated on `E2E_STACK=1`) plus\n  `@oxidean/api-client`'s own node-env suite; the `web-browser` job runs\n  the browser project as **5 internal shards** with a retry keyed\n  strictly on `VITEST_BROWSER_CONNECTION_CLOSED` (Chromium\n  session-lifetime bound). Playwright drives real-Chromium component and\n  full-stack e2e; wiremock supplies outbound-HTTP fixtures — connector\n  tests belong in the crate's `tests/` against mocked HTTP, never live\n  providers; Rust integration tests open real SQLite tempdir DBs (or\n  fresh provisioned per-test DBs when `OXIDEAN_TEST_DATABASE_URL` points\n  at a throwaway Postgres/MySQL); protocol smokes run real `git`, `ssh`,\n  and `git-lfs` client binaries against a live Compose stack (fail-closed\n  in CI under `SMOKE_REQUIRE_STACK=1`). **SSE streaming has no proven\n  test-type precedent**: happy-dom cannot exercise real SSE. The assigned\n  layers are an HTTP-level `*.stack.test.ts` (node env,\n  `OXIDEAN_E2E_API_ORIGIN`) asserting event-stream frames against the\n  live API for the stream contract, happy-dom for the client\n  append/fallback state, and optional stack-browser click-through for the\n  user-visible flow.\n- **Local/CI asymmetry**: `make test` runs nextest + web\n  unit/integration (plus the `oxidean-vcs --features server` leg);\n  browser, stack e2e, and smokes are separate targets — a green local\n  `make test` proves far less than a green CI run, and smoke scripts\n  skip-pass locally without Docker but fail-closed under `CI`.\n- **Test types / minimums**: per change type — pure helper → unit;\n  Query/session UI → happy-dom integration; high-risk UI → Chromium\n  browser test + manifest row; auth/RPC contract → Rust integration\n  and/or stack e2e; dialect-touching DB code → `dialect_*` test (noting\n  the CI-execution gap — and that a new gated binary only counts once it\n  joins the `db-dialects` `-E` filter). Suites: colocated Rust units plus\n  `crates/*/tests/` integration (**130** API test files — and\n  `procedures_list_matches_dispatch` is a *per-crate* convention, so\n  `oxidean-api-oxpilot` ships its own copy), web\n  unit/integration/component tests, stack e2e with Mailpit/OIDC/\n  Resend-WorkOS stubs (`make test-e2e-stack`), forge protocol smokes,\n  compose bring-up smoke per dialect, a VCS-server leg, and an Actions\n  pipeline e2e."
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
  "input_sha256": "sha256:a1e651323ca96675e9aa5d407638485a7fba191156059a5736d94d05df00fd98",
  "contract_sha256": "sha256:b277a854378ebfff5982c1e31d9804929cb0c6958d88f26c8ed8f5fcd3af7c1d"
}
```

## Steps

- [x] **Step 1 — Test-runner readiness** (contract runner step): baseline
  `cargo nextest run -p oxidean-api --test oxpilot`,
  `cargo nextest run -p oxidean-api-oxpilot`,
  `cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)'`
  (sqlite scratch DB under `tmp/` per U2's pinned invocation) —
  commands recorded verbatim in `unit-test-instructions.md`.

- [x] **Step 2 — C6 trait surface in api-core** (`crates/oxidean-api-core/src/oxpilot.rs`):
  `ToolSpec{name,description,parameters_json}`, `ToolCall{call_id,name,args_json}`,
  `ToolCtx{session_id,user_id,repo_id,proposal_id:Option<String>}`,
  `ToolOutput{content,output_summary}`, `ToolError{code,message}`,
  `trait ToolExecutor { catalog() -> Vec<ToolSpec>; async invoke(call,ctx) -> Result<ToolOutput,ToolError> }`,
  `NullTools` (empty catalog; invoke ⇒ `no_tools`). Per pinned fd
  entities + C6 amendment.

- [x] **Step 3 — ActiveTurn/ActiveTurns extension** (api-core `oxpilot.rs`):
  `ActiveTurn` gains `admission_lock: Arc<tokio::sync::Mutex<()>>`,
  `terminal_sent: AtomicBool`, `generation: u32`; `ActiveTurns` gains
  `admission_lock(session_id) -> Option<(Arc<ActiveTurn>, Arc<Mutex<()>>)>`
  (pair-return for stale-guard re-verify — R-13c); keep shipped
  `claim`/`get`/`release(session_id, turn_id)`.

- [x] **Step 4 — `turn_parts()` + slot `tools:`** (api-core `oxpilot.rs`):
  `OxpilotRuntimeSlot.tools: Option<Arc<dyn ToolExecutor>>`; widen
  `provider_for_turn()` to `turn_parts() -> TurnParts{provider:Option<Arc<dyn LlmProvider>>, model:Option<String>, executor:Arc<dyn ToolExecutor>}`
  under ONE slot-lock read; `None` maps to `NullTools`. Update
  `disabled_slot`/`new`/`from_slot`/rebuild constructors + the U4
  call sites.

- [x] **Step 5 — Runtime helpers** (`crates/oxidean-api-oxpilot/src/routes.rs`
  + new `runtime.rs` if the file needs the seam): `emit_terminal`
  (pub(crate), async, CAS on `terminal_sent` inside `emitted_deltas`
  lock, fanout push) + `emit_frame` + `assemble_prompt`
  (PROMPT_TRANSCRIPT_MAX_BYTES 128 KiB newest-first window via
  `rendered_len` incl. ≤200B/call summary lines, BR2.3) +
  `run_turn` → `run_round` (iterative; Delta→frame+accumulate;
  ToolCall→effective call_id mint → persist card `running` → emit →
  `executor.invoke` under `TOOL_CALL_TIMEOUT=60s` racing `cancel` →
  card `done`/`error` → tool `ChatMsg` ≤ `TOOL_OUTPUT_MAX` 32 KiB
  redacted; `Done`→terminal done; `Error(c)`→terminal `{error,c}`;
  `MAX_TOOL_ROUNDS=8` ⇒ `tool_rounds_exceeded`; wall `TURN_MAX_DURATION=10min`
  ⇒ `turn_timeout`; `model:None` ⇒ `no_model`; provider-absent ⇒
  `oxpilot.disabled`; card persist fail ⇒ `card_persist` ToolError
  feedback).

- [x] **Step 6 — `spawn_turn_task` wrapper + `drain_and_chain`**:
  outer spawn awaits inner `JoinHandle` — `Err(JoinError)` ⇒ warn +
  `emit_terminal{internal}` (CAS no-ops if already sent); then
  `drain_and_chain` under `admission_lock`: `release(session_id,turn_id)`
  → `svc.drain_queue` → non-empty ∧ `generation<16` ⇒ `claim(gen+1)`
  + respawn (claim-loss ⇒ quiet exit); `Err` ⇒ warn + exit.
  `run_turn` never releases — wrapper owns.

- [x] **Step 7 — `chat_post` pinned admission order** (routes.rs):
  verify/reorder to BR1.1's exact sequence — session auth →
  `require_enabled` → owned session → body validate → union dedupe →
  `check_budget` → `claim` → post-claim dedupe recheck
  (release-on-hit) → residual drain (BR4.2) → user-row insert →
  spawn. Post-claim exits release.

- [x] **Step 8 — `message_queue` locked enqueue** (session.rs):
  `check_budget` stays pre-hold; under `admission_lock` the proc
  re-verifies `(turn_id,liveness)` via the pair then commits
  `enqueue`; `None`/stale ⇒ `session_active_turn_absent`; post-commit
  queued-card emission unchanged (queued_id guard).

- [x] **Step 9 — `oxpilot.turn_cancel` proc + C8 hook**:
  dispatch arm + PROCEDURES + generated client (via `make rpc-gen`
  step); SessionOnly+`require_enabled`+owned; live ⇒
  `emit_terminal{aborted}`+`cancel.cancel()` ⇒ `{aborted:true}`;
  CAS-loss ⇒ `{aborted:false}`; absent ⇒ `no_active_turn`.
  `OxpilotTurnAbortHook` registered on the service at shell
  construction — delete path: `emit_terminal{aborted}` + cancel, then
  C8 chain continues (proposals.set_session_status + cascade); the
  doomed-session recheck exits quietly via Err/claim-loss arms.

- [x] **Step 10 — SSE attach hardening** (routes.rs `chat_stream`):
  verify shipped order holds — `fanout.subscribe()` BEFORE
  `emitted_deltas.lock().await.clone()` (R-06 pin); snapshot via
  `svc.list_messages` (streaming rows incl.); late-attach post-terminal
  resolves snapshot-only + ends.

- [x] **Step 11 — Tests**: unit tests in api-oxpilot for
  `assemble_prompt` windowing (byte bound + newest-first + reverse),
  `emit_terminal` first-wins (two callers ⇒ one frame), effective
  call_id mint, `run_round` bounds (timeout/rounds via fake provider
  + NullTools + `tokio::time::pause`), drain_and_chain arms
  (empty/nonempty/Err/claim-loss/depth-cap); proc integration tests
  for `turn_cancel` (live/absent/PAT/foreign); SSE attach smoke
  (snapshot→replay→live→terminal); PROCEDURES-set pin updated.

- [x] **Step 12 — `make rpc-gen`** for `turn_cancel` types; commit
  generated output (Rust is source of truth).

- [x] **Step 13 — Record**: `code-summary.md`, `unit-test-instructions.md`,
  `traceability.json` (per-step file targets); run the pinned test
  commands + `make test` for the workspace.
