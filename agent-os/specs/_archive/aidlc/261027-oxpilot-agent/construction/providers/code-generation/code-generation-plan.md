# Code Generation Plan — providers (U4)

Unit: `providers` (US5.1, US5.4). Implements the approved functional
design (`functional-design/` entities + rules BR1.1–BR5.6 + spec WF1–WF5),
the NFR design (`nfr-design/` — `OxpilotRuntime` slot model, resolve →
validate → build → persist → swap → audit critical section), and the
infrastructure spec (`infrastructure-design/` — env docs, railway.ts
preserve declarations, wiremock-only CI).

Consumes `inception/contract-design/contract-summary.md` C1 (proc shapes +
error vocabulary) + C4 (full `LlmProvider` shape: `tools` field,
`ToolCall`, `Done{usage}`).

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

Methodology: **test-after** — implement each applicable layer, then write
and run that layer's tests. Frontend behavior layer is inapplicable (U4
has no UI surface — U8 owns it).

## Plan steps

- [x] **Step 1 — Crate skeleton + deps.** Add `reqwest` (rustls+json+stream features), `async-trait`, `tokio-util` (already present), `futures-util` (already present) to `crates/oxidean-api-oxpilot/Cargo.toml` as per-crate pins (NOT `workspace = true` — root workspace deps don't carry them); `wiremock` under `[dev-dependencies]`. Create `src/providers/{mod,registry,openai,anthropic,ollama,stream,key,egress}.rs` + `src/settings.rs` module shells. (US5.1)

- [x] **Step 2 — Verify test runner + record unit-scoped command.** `cargo nextest run -p oxidean-api-oxpilot` runs the crate's tests; `cargo nextest run --test dialect_oxpilot` runs dialect assertions; `cargo test -p oxidean-api-oxpilot --test oxpilot_providers` for wiremock tests. Record in `unit-test-instructions.md`.

- [x] **Step 3 — Data model: `oxpilot_settings` migrations.** `crates/oxidean-db/migrations/{sqlite,postgres,mysql}/XXXX_oxpilot_settings.sql` — singleton table `id` (PK, must be 1), `enabled_override BOOL NULL`, `provider_id TEXT NULL`, `model TEXT NULL`, `base_url TEXT NULL`, `key_ciphertext TEXT NULL`, `key_last4 TEXT NULL`, `user_budget BIGINT NULL`, `instance_budget BIGINT NULL`, `updated_at` timestamp. Tri-dialect correct types (U1's `0085_oxpilot.sql` is the shape precedent). (US5.1, BR5.3)

- [x] **Step 4 — Data model tests.** Extend `crates/oxidean-db/tests/dialect_oxpilot.rs` with `oxpilot_settings` assertions (columns, nullability, singleton constraint id=1) per dialect. Runs in `db-dialects` CI job via the already-registered `-E` filter entry. (US5.5)

- [x] **Step 5 — Repository: `oxidean-db` oxpilot_settings store.** `crates/oxidean-db/src/oxpilot_settings.rs` — `get_row() -> Option<OxpilotSettingsRow>`, `upsert_row(&CandidateRow)`, `clear_row()` tri-dialect; key pair (ciphertext+last4) written/cleared in ONE statement; `updated_at` server-set. Mirror `mcp_settings.rs` precedent. (BR5.3, NFR1.5)

- [x] **Step 6 — Repository tests.** Store unit/integration tests: round-trip write→read, sparse-clear semantics, key-pair atomicity, singleton id=1. (Standard: 5-8 tests)

- [x] **Step 7 — Business logic part A: api-core trait delta + runtime types.** `crates/oxidean-api-core/src/oxpilot.rs`: add `ToolSpec`, `Usage`, extend `ChatRequest.tools: Vec<ToolSpec>`, `ChatStreamItem::Done{usage: Option<Usage>}` (C4 shape); `OxpilotRuntime { slot: RwLock<OxpilotRuntimeSlot>, rebuild_lock: Mutex<()> }`, `OxpilotRuntimeSlot { provider: Option<Arc<dyn LlmProvider>>, model: Option<String>, disabled_reason: Option<String>, turns: Arc<ActiveTurns>, built_from: Option<ProviderIdentity> }`, `ProviderIdentity { provider_id, model, base_url, key_source }` (inert types only); `OxpilotRuntime::new(env_slot)` sync boot construction (U1 parity); `resolve_oxpilot`/`require_enabled` become async slot reads; `AppState.oxpilot: Arc<OxpilotRuntime>`, `RpcCtx.oxpilot: Option<Arc<OxpilotRuntime>>`. (BR5.5, WF1)

- [x] **Step 8 — Business logic part B: providers tree.** `src/providers/`: `registry.rs` (3-entry `REGISTRY` + 4-member `RESOLUTION_MAP` incl. stub; `build(cfg) -> Result<Arc<dyn LlmProvider>, ProviderError>`); `key.rs` (`pub(crate) encrypt_key/key_last4`, `pub(in crate::providers) decrypt_key`, KEY_ENVS chain `OXIDEAN_OXPILOT_SECRETS_KEY → OXIDEAN_SESSION_SECRET`, `crypto.unavailable` on missing source); `egress.rs` (`validate_base_url(Option<&str>, &str)` — http/https, literal-IP all forms, `lookup_host` all-public, ollama + None-provider exemptions, redirect-free clients); `stream.rs` (`SseDecoder` `\n\n` framing + `NdjsonDecoder` `\n` over `bytes_stream()`/`futures_util`); `openai.rs`/`anthropic.rs`/`ollama.rs` adapters (chat-completions SSE, messages SSE, `/api/chat` NDJSON → `ChatStreamItem::{Delta,ToolCall,Done,Error}` + per-adapter `ProviderError.code` tables incl. 401/403/429/context-length/5xx; connect 10s / total 300s / ≤1 `is_connect` retry / redirect none). (BR1.1–BR1.5, NFR1.4–1.11, NFR3.3–3.5, NFR4.7–4.10)

- [x] **Step 9 — Business logic part C: settings module.** `src/settings.rs`: `EffectiveProviderConfig` + `resolve_over_env`; `apply(row, payload)` sparse semantics (absent=keep, explicit null=clear, `clear_overrides` wipes row; `api_key` vs `clear_key` conflict ⇒ `rpc.bad_input`); `validate` (async — resolved-config base_url check, model vs advertised models, key-requirement check); `build_slot`/`resolve_slot`/`disabled_slot` per the approved `built_from`-carry design (flag-off retains provider when identity unchanged; enabled+`provider:None` always carries a reason); `rebuild(&state)` shared boot/write path; `audit_oxpilot_settings` fire-and-log names-only. Whole section under `rebuild_lock`: resolve → validate → build → persist → swap → audit; build BEFORE persist; flag/budget-only writes skip build; build arm requires `resolved.enabled`. (BR2.1–BR5.2, WF2–WF4, NFR4.1–4.6)

- [x] **Step 10 — Business logic tests.** Unit tests (colocated): resolve_over_env merge matrix (row-over-env per field, null=follow-env); apply() sparse semantics incl. api_key/clear_key conflict; resolve_slot all arms (flag-off carry/drop, no_provider, misconfigured-on-uncarryable); validate cases incl. provider-less; egress literal forms (dotted/hex/octal/int/IPv6/v4-mapped) + private/loopback/link-local/metadata rejects + ollama/None exemptions; custody round-trip (encrypt→decrypt, last4, missing-source ⇒ crypto.unavailable); decoders (delta ordering, done+usage, tool calls, error frames) per fixture; classify tables (401/403/429/context-length/5xx); retry boundary (is_connect once, sent-never). Wiremock tests: per-adapter golden fixture + error-status set; no live calls. (5-8/component, NFR8.1)

- [x] **Step 11 — API/endpoint layer.** `admin.oxpilot.settings_get` / `settings_set` / `providers_list` procs in `settings.rs`; register in `procedures.rs` closed set + `dispatch.rs` arms; `require_admin` session gate (PAT refused `auth.pat_scope` pre-dispatch); gate-exempt from `oxpilot.disabled`; `settings_get` returns effective + `*_overridden` flags + `key_set`/`key_last4` (never plaintext/ciphertext); `providers_list` = REGISTRY + one row read for `configured`/`requires_key`; `procedures_list_matches_dispatch` pinning test (per-crate convention). Run `make rpc-gen` and commit generated client output. (BR5.1–BR5.5, C1, NFR8.2)

- [x] **Step 12 — API/endpoint tests.** `tests/oxpilot_settings.rs` integration: session-only rejection matrix (bearer/PAT/anon/non-admin ⇒ shipped codes), settings round-trip (get→set→get with overrides), invalid provider/model/`base_url` ⇒ `invalid_provider` + nothing persisted + old slot intact, provider-less writes succeed (budget-only/flag-only/clear_overrides), flag-off→flag-on Arc carry, build-failure ⇒ `invalid_provider`, key write→`key_set`+`key_last4` and no material in output, concurrent-writes serialization, `stub` refused in release-shaped write path. (Standard: integration boundary)

- [x] **Step 13 — U1 seam migration.** `routes.rs`/`status.rs`/`session.rs` read `slot` via `resolve_oxpilot` (async); `main.rs` unconditional `settings::rebuild(&state)` after `AppState::new` (outside AUTO_MIGRATE branch; lookup_host bounded so it can't stall listen); `tests/oxpilot.rs` switches `build_oxpilot_slot` → `OxpilotRuntime::new`+`rebuild`; `with_oxpilot` test hook updated. (BR5.5, NFR4.11–4.12)

- [x] **Step 14 — Environment/build configuration.** `.env.example` + `docs/CONFIGURATION.md`: `OXIDEAN_OXPILOT_API_KEY`, `_BASE_URL`, `_USER_BUDGET`, `_INSTANCE_BUDGET`, `_SECRETS_KEY` documented (secrets marked, custody + rotation caveats, Ollama private-target disclosure, DNS TOCTOU caveat). `.railway/railway.ts`: `preserve()` declarations for all five + `OXIDEAN_SESSION_SECRET` on the api service. `docs/TESTING.md`: oxpilot_settings row note if the dialect row format needs it. `Cargo.toml` workspace unchanged (deps are per-crate). (BR5.6, NFR2.1)

- [x] **Step 15 — Documentation and traceability.** `code-summary.md` (files, decisions, deviations), `source-manifest.json` (strict schema), `traceability.json` (BR/AC/NFR → files). Verify: `cargo nextest run -p oxidean-api-oxpilot`, `cargo nextest run -p oxidean-db -E 'binary_id(dialect_oxpilot)'`, `cargo nextest run --test oxpilot_settings`, `make rpc-gen` idempotent, `make test` green.

## Story traceability

- US5.1 (flag/provider config): Steps 1,3,5,7,8,9,11,13,14 — settings procs + runtime slot + custody.
- US5.4 (Ollama self-host): Steps 8 (ollama adapter + egress exemption), 12.
- US5.5 clauses (dialect/registration/docs): Steps 4,11,14.
- BR3.x write semantics: Steps 9,12; BR4.x rebuild mechanics: Steps 7,9,13; BR5.x surface/docs: Steps 11,14.
