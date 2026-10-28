# Code Generation Plan — u1-skeleton-slice

Walking skeleton: `/oxpilot` page → SessionOnly `oxpilot.*` RPC →
StubProvider → SSE-streamed reply, with session persistence on
`oxidean-db` ×3 dialects.

Consumes: functional-design/{entities,functional-spec,rules},
nfr-requirements/*, nfr-design/*, infrastructure-design/*,
contract-summary.md (C1–C4, C8), unit-of-work.md §U1.

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

## Plan steps (test-after ordering per contract)

1. **Project structure skeleton** — scaffold `crates/oxidean-api-oxpilot`
   (Cargo.toml, lib.rs skeleton, CHANGELOG.md); add workspace member in
   root `Cargo.toml`; add `[[component]]` + `oxidean-api.depends_on`
   edge in `versioning.toml`. Stories: US1.1/US1.2/US1.3 backbone.
2. **Runner readiness** — verify unit-scoped commands:
   `cargo nextest run -p oxidean-api-oxpilot` (crate tests),
   `cargo test --manifest-path crates/oxidean-db/Cargo.toml --test
   dialect_oxpilot` (dialect), `bun vitest run <path>` for web files.
   Record in `unit-test-instructions.md`.
3. **Data model — migrations ×3** — `oxpilot_sessions` +
   `oxpilot_messages` tables in
   `crates/oxidean-db/migrations/{sqlite,postgres,mysql}/`: TEXT ids
   (CHAR(36) mysql), `(user_id,last_activity_at)` + `(session_id,seq)`
   indexes, `client_msg_id` per-session unique, cascade FK
   `sessions.user_id`, `messages.session_id` ON DELETE CASCADE.
   Stories: US1.2 (persistence).
4. **Data model — dialect test** — `crates/oxidean-db/tests/dialect_
   oxpilot.rs` asserting both tables + indexes + FK exist on all three
   dialects; add `binary_id(oxidean-db::dialect_oxpilot)` to the
   `db-dialects -E` filter in ci.yml run step + `docs/TESTING.md` job
   table row.
5. **Repository/data access — `oxpilot-db` module in the crate** —
   `SessionStore`/`MessageStore`: `create_session`, `list_sessions`
   (owned, `ORDER BY last_activity_at DESC`), `find_owned_session`
   (anti-enumerating), `insert_user_message` (redacted, before turn
   claim), `insert_assistant_row`, `append_delta`/`finalize_row`,
   `snapshot` (`ORDER BY seq`), `sweep_orphans` (boot). Stories:
   US1.2, US1.3.
6. **Repository tests** — crate `tests/` sqlite-backed: insert/list/
   ownership-filter/snapshot ordering/client_msg_id dedupe/orphan
   sweep semantics (5–8 tests).
7. **Business logic — `redact()`** — new pattern-table detector in
   `oxidean-core` (bearer/JWT/PEM/key idioms); unit tests colocated
   (each pattern family + non-secret pass-through, ~6 tests).
8. **Business logic — `EnablementGuard`** —
   `resolve_enablement(ctx) -> EnablementState` (env flag →
   `oxpilot_settings` probe-read (absent ⇒ env fallback) → provider
   slot resolution `stub|unset|other` → reason precedence
   `flag_off > misconfigured > no_provider`) and
   `require_enabled(ctx) -> Result<_, AppError>` (`oxpilot.disabled`
   typed). `StubProvider` + `LlmProvider` trait slot on `RpcCtx`
   (`Option<ProviderHandle>`); constructor-only failure-injection flag
   on StubProvider. `ActiveTurn` map slot on `RpcCtx`: insert-if-absent
   claim, `turn_id`/`message_id`/`started_seq` minted at claim,
   broadcast(256), per-message `emitted_deltas` buffer (all deltas of
   the in-flight message), cancellation token, drop-guard/JJoinHandle
   finalization (panic ⇒ entry removal + `status=error`). Unit tests:
   enablement matrix ×6, claim/dup-claim/409, buffer replay, panic
   finalization.
9. **API/endpoint — RPC procs** — `oxpilot.status` (gate-exempt,
   `resolve_enablement` → `{enabled, provider{id,model}|null, reason}`),
   `oxpilot.session_create` (`require_enabled` → repo resolve →
   insert → fire-and-forget audit `{session_id, repo_owner, repo_name,
   branch}`), `oxpilot.session_list` (`require_enabled` → owned list).
   `PROCEDURES` + dispatch arm + `extend_from_slice` in the api shell;
   `make rpc-gen` for the generated client. Stories: US1.1, US1.2.
10. **API tests** — `tests/` in the crate: `procedures_list_matches_
    dispatch` copy (per-crate convention), SessionOnly closed-set test
    (all three procs refuse PAT-class tokens), status disabled matrix,
    session create/list round-trip + cross-user isolation
    (5–8 tests, sqlite-backed RpcCtx).
11. **API/endpoint — SSE routes** — `/api/` handlers:
    `POST .../chat` (auth → `require_enabled` → owned session → 64 KiB
    body cap ⇒ `422 invalid_body` → redacted user row → claim or
    `409 turn_active`/`client_msg_id` join or post-terminal resend →
    spawn turn task (detach-to-complete + panic guard) → SSE stream:
    buffered replay + live broadcast + `terminal{done|error}` +
    `Lagged`→buffer replay resync); `GET .../chat/stream` (auth → gate →
    ownership → snapshot{messages, active_turn, pending_queue:[]} +
    live attach). Keep-alive 30 s. Transport close detaches only.
    `started_seq`/`seq` wire fields per C2.
12. **SSE/stack tests** — Rust integration: frame ordering
    (appends→single terminal), 409 race, dedupe resend,
    `emitted_deltas` replay on mid-turn attach, `Lagged` resync,
    error-terminal via failure-injection (5–8 tests). Plus
    `apps/web/e2e/stack/oxpilot.stack.test.ts` — POST → frames →
    terminal + first-frame <250 ms / snapshot <250 ms / RPC <150 ms
    smoke ceilings; `run-stack-e2e.sh` exports
    `OXIDEAN_OXPILOT_ENABLED=1` + `OXIDEAN_OXPILOT_PROVIDER=stub`.
13. **Frontend — route registration** — `SHELLS` + Astro shell +
    `RESERVED_TOP_SEGMENTS += "oxpilot"` + regenerated routeTree;
    `routes/oxpilot.tsrx` (index: session list + new-session form) +
    `routes/oxpilot.$session.tsrx` (workspace: transcript + composer);
    `lib/oxpilot-queries.ts` queryOptions factory; `usePageRedirect`
    auth gate; `@tanstack/ai-octane` `useChat`-family composer;
    fetch-ReadableStream SSE client (POST) + snapshot dedupe by
    `(message_id, seq)`; flag-off explanatory state naming
    `OXIDEAN_OXPILOT_ENABLED` + provider envs + `docs/CONFIGURATION.md`;
    baseline semantics (main landmark, headings, labels). Stories:
    US1.1, US1.3 (+ NFR7.1/NFR8.1).
14. **Frontend tests** — happy-dom integration tests (mount, flag-off
    render, composer state append/fallback); Chromium `*.browser.test.
    tsx` for the session-form high-risk control (Select/portal);
    route-coverage manifest rows ×2 + browser-coverage manifest row.
15. **Configuration artifacts** — `.env.example` entries ×3;
    `docs/CONFIGURATION.md` oxpilot section (env table + plaintext
    transcript disclosure + redact-on-write + per-user isolation +
    pattern-table recall limit + single-replica ActiveTurn caveat +
    flag-off reason surface); `.railway/railway.ts` `preserve()` on the
    three vars; `crates/oxidean-api-oxpilot/CHANGELOG.md`;
    `make rpc-gen` client regeneration.
16. **Documentation & traceability** — code-summary.md,
    source-manifest.json, traceability.json; this plan's checkboxes
    ticked.

## Story → step traceability

| story | steps |
|-------|-------|
| US1.1 (enable page + chat) | 1, 9, 11, 13, 14 |
| US1.2 (session create/list persist) | 3, 4, 5, 6, 9, 10 |
| US1.3 (streamed reply + resume) | 8, 11, 12, 13, 14 |
| Flag-off experience (WF5) | 8, 9, 10, 13, 14, 15 |
| Anti-enumeration/ownership (NFR1.4) | 5, 6, 9, 10 |
| Anti-abuse (NFR1.10–1.12) | 8, 11, 12 |

## Source manifest (planned)

`source-manifest.json` written pre-review covering every created/
modified path: new crate (≈8 files), migrations ×3, dialect test,
`oxidean-core/src/redact.rs` + mod wiring, `RpcCtx` slots
(oxidean-api-core), SSE route registrations (oxidean-api-rest or
router shell), api shell PROCEDURES/dispatch/extend_from_slice,
versioning.toml, `.env.example`, `docs/CONFIGURATION.md`,
`.railway/railway.ts`, `scripts/dev-auth/run-stack-e2e.sh`,
`ci.yml -E` filter, `docs/TESTING.md`, web routes + queries + client +
manifests + tests, generated `packages/api-client` + routeTree +
shells.rs outputs.
