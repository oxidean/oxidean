# Code Generation Plan — connector-gateway (U6)

Unit: `connector-gateway` (US5.3 pairing/connect surface, US5.5
grant/cancel surface; contributes US1.2 target listing, AC1.5.2
session-delete cascade). Implements the approved functional design
(`functional-design/` — BR1–BR7 + entities + spec), the NFR design
(`nfr-design/` — secret custody, claim-loop ordering, close paths,
audit split, logical components), and the infrastructure spec
(`infrastructure-design/` — no new jobs, dialect tests inside the
shipped `dialect_oxpilot.rs` binary, `oxpilot_*` prefixed tables).

Consumes `inception/contract-design/contract-summary.md` C1 (the 8
`oxpilot.*` procs), C3 (connector HTTP surface: `/pair`, `/claim`,
`/report`, `/heartbeat` + field names `{actions,cancel,retry_after}`),
C7 (`dispatch_action(...) -> Result<Action, Denied>` — `Denied` is an
AppError-domain error with `data:{denied:{reason, grant_id?}}`; the
U5 tool executor depends on this shape verbatim), C8 (hook ordering —
`ConnectorDeleteHook` runs after U3's abort hook).

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

Ordered test-after per the contract; each layer's tests land with it.

1. [x] **Migrations — `crates/oxidean-db/migrations/{postgres,sqlite,mysql}/0088_oxpilot_connector.sql`** (next number after max 0087, additive ×3): tables `oxpilot_pairings`, `oxpilot_targets`, `oxpilot_grants`, `oxpilot_actions` per entities.md; `UNIQUE(code_hash)`; partial-unique `verifier_hash` (NULL tombstones must not collide — filtered index where supported, documented MySQL divergence); FK `ON DELETE CASCADE` on session-scoped grant/action rows (AC1.5.2); index set: `(target_id, state, created_at, action_id)` FIFO dequeue, `(owner_user_id)` owner lists, equality `code_hash`/`verifier_hash`.

2. [x] **DB store — `crates/oxidean-db/src/oxpilot.rs`** extension: the store catalogue from nfr-design logical-components (`pairing` mint/lookup/claim CAS, `target` CRUD + liveness touch + verifier tombstone, `grant` mint/resolve/revoke/cancel + lazy-expiry ops, `action` mint/resolve/dequeue FIFO-50 + cancel ops, session-cascade ops). All dialect code stays in this crate.

3. [x] **Trait port — `crates/oxidean-api-core/src/oxpilot.rs`**: `ConnectorGateway` trait (set-once service) + `ConnectorIdentity`, `Denied` error shape (`Result<Action, Denied>` per C7 — AppError-domain, `data:{denied:{reason, grant_id?}}`), entity view types (`PairingView`, `TargetView`, `GrantView`, `ActionView` — projections exclude hash/secret columns per NFR1.11); `OxpilotRuntime` gains `gateway: OnceLock<Arc<dyn ConnectorGateway>>` beside `sessions` (set-once, survives slot rebuilds).

4. [x] **Dialect tests — `crates/oxidean-db/tests/dialect_oxpilot.rs`** extension (same-change, inside the shipped binary): connector-table CRUD on all three engines; partial-unique `verifier_hash` NULL semantics asserted per real engine behavior; CASCADE removal on session delete; FIFO dequeue ordering; claim CAS single-winner.

5. [x] **Gateway impl — `crates/oxidean-api-oxpilot/src/connector/`**: per-target keyed `Mutex` + `Notify` map (process-local, lazy-built); `dispatch_action` ordered checks (session-ownership → target lifecycle/liveness → caps → live grant → pending dedupe → `Denied{consent_required, grant_id}`); `grant_respond` state machine (pending + BR6.4 deny-on-allowed for `action_session`); claim long-poll (arm `Notify` BEFORE first dequeue, dequeue-before-park, recheck after wake, 60s clamp, `claimed→running` flip on any authenticated touch); report/heartbeat handlers (idempotent terminal close, cancel-set re-announce, `not_found` → kill-local); `ConnectorDeleteHook` (registered after U3's abort hook; hook failure aborts delete; idempotent).

6. [x] **C3 HTTP routes — `/api/oxpilot/connector/{pair,claim,report,heartbeat}`** in api-oxpilot's route registration: enablement via `resolve_enablement_slot(&*state.oxpilot.slot.read().await)` → `{error:"disabled"}` fail-closed; `/pair` unauth + unrate-limited (code IS the credential); verifier auth = IP-limiter-check → SHA-256 → hash lookup → `ConnectorIdentity` sole scoping (body `target_id` mismatch → `forbidden`; foreign proc-side ids → `not_found`); revoked verifier still authenticates for heartbeat/terminal-close; server-side `redact` before persist/audit; `audit::record_with` on these ctx-less paths.

7. [x] **Owner procs — `oxpilot.pairing_create`, `oxpilot.target_list`, `oxpilot.target_get`, `oxpilot.target_disconnect`, `oxpilot.target_revoke`, `oxpilot.grant_list`, `oxpilot.grant_get`, `oxpilot.grant_respond`** registered in PROCEDURES + dispatch (SessionOnly, `oxpilot.disabled`-gated, fail-closed `classify_pat_proc` untouched); pairing mint enforces `body.caps ⊆ {exec_shell, fs_read, fs_write}` + per-user limiter; connect-grant re-mint on owner read of pending_consent target (BR3.2); `audit::record` on proc paths.

8. [x] **Generated client — `make rpc-gen`** and COMMIT the regenerated `packages/api-client` output (unconditional — the `rpc-sync` job fails on drift).

9. [x] **Route + proc integration tests — `crates/oxidean-api-oxpilot/tests/`**: C3 auth matrix (401 absent/unknown/tombstoned, revoked-exception arms, 429 on limiter trip, `{error:"disabled"}` when off, forbidden on body-mismatch); pair→claim→report→heartbeat happy path; claim park/wake semantics (204 + `Retry-After`, dequeue-before-park, lost-wakeup pin test); consent flow → `Denied` shape; disconnect/revoke/session-delete close paths; audit emission spy checks (no secrets in audit payloads).

10. [x] **Docs — same change**: `docs/TESTING.md` job-table row for the connector dialect coverage + exact unit-scoped commands; `docs/CONFIGURATION.md` connector-egress paragraph (outbound-only, what C3 carries). No `.env.example` changes (no new env vars).

11. [x] **Record artifacts**: `unit-test-instructions.md` (exact commands), `code-summary.md`, `traceability.json` for this stage.

## Bounded invariants the implementer must not drift

- `dispatch_action` returns `Result<Action, Denied>` — consent is an
  ERROR channel (`Denied`), never an Ok variant.
- `connector_id == target_id` (v1 same value, both response fields).
- Claim ordering: armed Notify future precedes first dequeue;
  dequeue-before-park; recheck after wake; `claimed→running` flip on
  any authenticated touch (claim OR heartbeat).
- Capability mismatch at dequeue = skip only; dead grant = cancel
  that row + audit (`grant_expired` extension reason).
- Disconnect: verifier tombstoned + pending/allowed grants
  `cancelled` + all non-terminal actions directly `cancelled` —
  never `cancel_requested` (channel dead). Revoke: verifier stays
  valid, grants `revoked`, queued `cancelled`, claimed/running
  `cancel_requested` carried on heartbeat.
- Session delete: hook after U3's abort hook; pending/allowed grants
  + all non-terminal actions `cancelled` directly; connect grants
  (`session_id NULL`) survive; audit survives (no FK).
- Never stored/audited/logged: verifier, verifier hash, pairing
  code, code hash, raw action args, env values, unredacted output.
