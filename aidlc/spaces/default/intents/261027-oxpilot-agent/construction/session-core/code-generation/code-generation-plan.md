# Code Generation Plan — session-core (U2)

Unit: `session-core` (US1.5 delete cascade + revoke-hook entrypoint,
US1.8 queue + drain + pending, US5.2 usage counters + refusal;
contributes to US1.1/US1.2 session CRUD backing and US5.5 dialect ACs).
Implements the approved functional design (`functional-design/`
entities + rules + spec), the NFR design (`nfr-design/` — the 21-verb
`SessionService` trait + DTO set, `OnceLock` runtime residence +
`sessions()` accessor, per-constructor `budget_caps`, union dedupe,
per-dialect drain tx, hook-abort delete, `QueuedReceipt` position
ordinals, `snapshot_frame` pending_queue/tool_calls extension), and the
infrastructure spec (`infrastructure-design/` — additive-only 0087 ×3
dialects, unconditional `install_sessions` at boot, no new CI jobs).

Consumes `inception/contract-design/contract-summary.md` C1 (proc
shapes + error vocabulary + `usage_get` payload), C2 (snapshot/tool
frame semantics), C5-AMENDED (21-verb service incl.
`find_message_by_client_msg_id`, `set_message_content`,
`set_message_status`, `list_tool_calls`, `set_tool_call_status`,
`usage_view`; `enqueue -> QueuedReceipt`; `check_budget(user_id, caps)`;
`mark_applied(id, pr_ref)`), C8 (`SessionDeleteHook`).

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

- [x] **Step 1 — Test-runner readiness** (contract runner step): verify
  `cargo nextest` runs the U1 oxpilot binaries green before any code:
  `cargo nextest run -p oxidean-api --test oxpilot`,
  `cargo nextest run -p oxidean-db -E 'binary_id(oxidean-db::dialect_oxpilot)'`,
  `cargo nextest run -p oxidean-api-oxpilot`. (Baseline for every later
  step; commands recorded verbatim in `unit-test-instructions.md`.)
- [x] **Step 2 — Migration `0087_oxpilot_session_core` ×3 dialects**
  (data model — implement): `crates/oxidean-db/migrations/{postgres,
  mysql,sqlite}/0087_oxpilot_session_core.sql` —
  `oxpilot_tool_calls` {id pk, session_id→session CASCADE, message_id,
  call_id, seq, call_index, tool, args_summary, output_summary, status,
  created_at; UNIQUE(session_id, call_id)}, `oxpilot_queued_messages`
  {id pk, session_id→CASCADE, client_msg_id, content, queue_seq,
  created_at; UNIQUE(session_id,client_msg_id), UNIQUE(session_id,
  queue_seq), INDEX(session_id)}, `oxpilot_proposals` {id pk,
  session_id→CASCADE, title, base, diff_body, status pending|applied,
  pr_ref NULL, created_at}, `oxpilot_usage_counters` {id pk, scope,
  scope_key, window, requests, tokens, updated_at; UNIQUE(scope,
  scope_key, window)} — column types mirror 0085's per-dialect
  conventions (US1.5, US1.8, US5.2, US5.5-dialect).
- [x] **Step 3 — Store layer** (repository — implement):
  `crates/oxidean-db/src/oxpilot.rs` gains row structs
  `OxpilotToolCallRow`/`OxpilotQueuedRow`/`OxpilotProposalRow`/
  `OxpilotUsageCounterRow` and NEW fns `delete_session_cascade`,
  `next_queue_seq`, `enqueue_queued`, `find_queued_by_cmid`,
  `list_queued_for_session`, `drain_queue_tx` (pg/sqlite
  `DELETE…RETURNING` + in-tx message inserts minting seq; mysql
  `SELECT…FOR UPDATE ORDER BY` + DELETE-by-ids), `insert_tool_call`,
  `list_tool_calls_for_session`, `set_tool_call_status`,
  `insert_proposal`, `find_proposal`, `mark_proposal_applied`,
  `upsert_usage` (dual-scope pair: `(user, user_id)` +
  `(instance, '__instance__')` in one tx), `read_counters`. REUSED
  untouched: `find_owned_session`, `insert_session`,
  `list_sessions_for_user`, `touch_session_activity`,
  `find_message_by_client_msg_id` (no status filter), `find_message_by_id`,
  `next_message_seq`, `insert_message`, `list_messages` (unfiltered —
  snapshot path needs `streaming`), `set_message_content`,
  `set_message_status`, `sweep_streaming_messages`.
- [x] **Step 4 — Data-layer tests** (data model + repository — write
  and run after impl): extend
  `crates/oxidean-db/tests/dialect_oxpilot.rs` with 0087 assertions per
  dialect — table presence, cascade-delete removes
  messages/tool_calls/queue/proposals but NOT usage_counters, unique
  violations on `(session_id,client_msg_id)`/`(session_id,queue_seq)`/
  `(session_id,call_id)`/`(scope,scope_key,window)`, `drain_queue_tx`
  atomicity (queue rows gone, transcript rows minted in order),
  `upsert_usage` dual-scope single-tx. Run the registered binary:
  `cargo nextest run -p oxidean-db -E
  'binary_id(oxidean-db::dialect_oxpilot)'`. Update the
  `docs/TESTING.md` row's descriptive text in the same change.
- [x] **Step 5 — api-core trait port + runtime seam** (business-logic
  seam — implement): `crates/oxidean-api-core/src/oxpilot.rs` gains —
  `trait SessionService` (21 verbs, `user_id` first arg,
  `Result<_, AppError>`, oxidean-db row returns + `QueuedReceipt`),
  `trait SessionDeleteHook`, the DTO set (`CreateSession`,
  `AppendMessage{id,status}`, `Enqueue`, `QueuedReceipt{Option,
  Option}`, `ToolCallWrite{seq,call_index}`, `ToolCallStatusUpdate`,
  `ProposalWrite`, `UsageDelta`, `BudgetCaps`, `UsageView`);
  `OxpilotRuntime` gains `sessions: OnceLock<Arc<dyn SessionService>>`
  + `install_sessions` (get_or_init semantics) + `sessions()` accessor
  (unset ⇒ `oxpilot.internal`); `OxpilotRuntimeSlot` gains
  `budget_caps: BudgetCaps` populated at EVERY constructor — boot
  `OxpilotRuntime::new` parses `OXIDEAN_OXPILOT_{USER,INSTANCE}_BUDGET`
  envs, `resolve_slot`/`built_slot`/`carried_slot`/`disabled_slot`
  thread it (`carried_slot` gains `&EffectiveProviderConfig`;
  `disabled_slot` gains `caps: BudgetCaps`, standalone callers pass
  `BudgetCaps::default()`); `snapshot_frame` signature gains
  `pending_queue` + `tool_calls` params and merges tool cards into
  each message object by `message_id` ordered by `call_index`.
- [x] **Step 6 — api-core tests** (business logic — write and run
  after impl): colocated `#[cfg(test)]` in `oxpilot.rs` — `sessions()`
  unset ⇒ `oxpilot.internal`, `install_sessions` set-once/no-op
  second call, `budget_caps` populated on every constructor path
  (boot/disabled/carried/built), `snapshot_frame` merges tool cards in
  `call_index` order and passes pending_queue through. Run
  `cargo nextest run -p oxidean-api-core -E 'test(oxpilot)'`.
- [x] **Step 7 — `OxpilotSessionService` impl** (business logic —
  implement): `crates/oxidean-api-oxpilot/src/services.rs` — fields
  `db: oxidean_db::Database` + `hooks: Vec<Arc<dyn SessionDeleteHook>>`
  (ships `vec![]` + `cfg(test)` `ProbeHook`); `impl
  From<&EffectiveProviderConfig> for BudgetCaps`; every session-scoped
  verb runs `require_owned_session` first (id-keyed proposal verbs
  fetch-then-check via the row's `session_id`); `redact_in_place` on
  exactly BR3.1's field set at service top; `append_message` mints seq
  via `next_message_seq`; union dedupe transcript-first-then-queue;
  `enqueue` returns `QueuedReceipt{Some,Some}` / `{null,null}` on
  transcript-hit; `drain_queue` wraps `drain_queue_tx`; `delete_session`
  = ordered hook loop (hook error ⇒ merge `hook_index` into
  `AppError.data`) → `delete_session_cascade`; `check_budget` ⇒
  `read_counters` + compare both axes vs caller-passed caps (user
  breach reported first) ⇒ `budget_exceeded` + `data:{scope}`;
  `usage_view` fills all five C1 fields and never refuses.
- [x] **Step 8 — Service tests** (business logic — write and run after
  impl): colocated `#[cfg(test)]` in `services.rs` against a real
  sqlite `Database` — dedupe arm matrix (transcript-hit null/null,
  queued-hit same receipt, collision⇒idempotent), `queue_seq` collision
  bounded retry (8), `redact_in_place` field coverage incl. id
  carve-outs, hook ordering + abort-before-cascade (`ProbeHook`),
  `budget_exceeded` scope ordering, `usage_view` never-refuses,
  `position` ordinal vs `queue_seq` after drains, proposal fetch-then-
  check `not_found` on foreign row.
- [x] **Step 9 — Procs + route rewiring** (API/endpoint — implement):
  `oxidean-api-oxpilot` `dispatch.rs`/`routes.rs` — `OXPILOT_PROCEDURES`
  +5 (`session_get`, `session_delete`, `message_queue`, `queue_list`,
  `usage_get`), SessionOnly + `require_enabled` at handler top, thin
  parse→`ctx.oxpilot` unwrap (settings.rs:591 idiom)→`.sessions()?`→
  service→typed-error map; `session_delete` handler writes the
  `audit::record` row on Ok AND on hook_failed/not_found Err
  (`detail={"result":..,"hook_index"?}`); routes.rs rewires its session
  writes through the service — dedupe read →
  `service.find_message_by_client_msg_id`, inserts →
  `append_message{id: turn.message_id/ fresh, status}`,
  `set_message_content`/`set_message_status`/`append_tool_call` via
  service (caller-supplied seq/call_index), `snapshot_frame` call sites
  pass `list_queue` + `list_tool_calls` reads; `main.rs` calls
  `install_sessions` unconditionally after `AppState::new`.
- [x] **Step 10 — API tests** (API/endpoint — write and run after
  impl): `crates/oxidean-api/tests/oxpilot.rs` additions — five procs'
  auth matrix (anon `unauthenticated`, PAT `pat_scope`, flag-off
  `oxpilot.disabled`), `message_queue` receipt shapes incl.
  transcript-hit `{null,null}` and live-turn `409`-mapped
  `session_active_turn_absent`, `queue_list` ordering, `usage_get`
  C1 payload (null caps = unbounded, values after `record_usage`),
  `session_delete` audit row + `hook_failed` detail, PROCEDURES closed-
  set test +5. Run `cargo nextest run -p oxidean-api --test oxpilot`
  and `-p oxidean-api-oxpilot`.
- [x] **Step 11 — RPC types + docs** (environment/config): `make
  rpc-gen` regenerates `packages/api-client` for the five new procs;
  `make rpc-sync-check` idempotent before commit.
  `docs/TESTING.md` `dialect_oxpilot` row text updated for 0087.
- [x] **Step 12 — Verification**: unit-scoped nextest legs all green;
  `make test` full workspace green; `rpc-sync-check` clean.
- [x] **Step 13 — Record artifacts**: `source-manifest.json` (every
  created/modified source path), `code-summary.md`,
  `traceability.json` under this stage's record dir.

Story → step map: US1.5 → steps 2 (cascade FKs), 7 (hook loop +
cascade), 9/10 (session_delete proc + audit); US1.8 → steps 2
(queued_messages), 3 (drain tx + queue fns), 7 (enqueue/dedupe/
position), 9/10 (message_queue + queue_list procs); US5.2 → steps 2
(usage_counters), 7 (record_usage/check_budget/usage_view), 9/10
(usage_get proc); US1.1/US1.2 backing → steps 5/7/9/10 (session CRUD
verbs + session_get/list surfaces); US5.5-dialect → steps 2/4.
