# Code Generation Plan — connector-client (U7)

Unit: `connector-client` (US4.3 connector run verbs; contributes
US4.6 custody/disconnect UX). Implements the approved functional
design (`functional-design/` — BR1–BR9 + entities + spec), the NFR
design (`nfr-design/` — two-backend custody, nested exec select!,
classify→Disposition dispatch, jail chain, scrub pipeline), and the
infrastructure spec (`infrastructure-design/` — zero deployables,
workspace-archive test lanes, parity-check wiring, docs surfaces).

Consumes `inception/contract-design/contract-summary.md` C3 (the
consumed wire surface: `/pair`, `/claim`, `/report`, `/heartbeat`
with flat `{"error":"<code>"}` envelopes, `{actions,cancel,
retry_after}` claim shape, `args` as a JSON-encoded STRING), and
the shipped U6 route semantics in
`crates/oxidean-api-oxpilot/src/connector/` (10-code vocabulary,
`Retry-After` on 204 + `retry_after` body field, `OUTPUT_MAX` 256
KiB, wait max 60s, heartbeat 15s, pairing TTL 600s).

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

1. [x] **Module skeleton — `crates/oxidean-cli/src/pilot/`**: `mod.rs`
   (Command enum variants `PilotPair`/`PilotRun`/`PilotDisconnect`/
   `PilotStatus` + HELP usage-registry entries + dispatch arms, all
   hand-rolled per args.rs's `Command`/`Flags`/`GROUPS`/`completion::
   COMMAND_TREE` — every match site is compile/test-enforced, and the
   exhaustive `required_procedures` match must return `vec![]` for the
   non-RPC pilot arms); help text carries the exit-code contract, jail
   semantics, and cap table per observability-design. `openapi.yaml`
   checked in as the consumed-C3 reference doc.

2. [x] **`client.rs`**: typed C3 calls (pair/claim/report/heartbeat)
   over the crate's reqwest conventions; `C3Error` enum decode
   (`{"error":"<code>"}` parsed FIRST — unparseable body →
   `Transport`, never a guessed code); `RateLimited{retry_after}`
   reads the `Retry-After` RESPONSE HEADER on 429; `wait_seconds`
   client clamp [1,60]; timeouts 30s / wait+15s; Bearer-only auth
   (never `resolve_token`/PAT/cookie); claim response `{actions,
   cancel, retry_after}` with `ActionClaim.args` double-parse (outer
   envelope, then inner args JSON string); `--verbose` request log
   with the enumerated redact set.

3. [x] **`custody.rs`**: `write_verifier` (keyring
   `pilot/<instance>/<connector_id>` first → on Err the
   `pilot-secrets.json` 0600 tmp+fsync+rename map — both-fail → abort
   WITHOUT a row), `read_verifier` → `Result<Option<(Backend,
   SecretString)>>` (backend rides the return for status's live
   probe), `delete_verifier` best-effort BOTH; `pilot-targets.json`
   0600 atomic map I/O keyed by `target_id` (verifier NEVER in-file);
   create-path ordering verifier-then-row with `delete_verifier`
   compensation on upsert failure; corrupt-file = named parse error
   (not zombie).

4. [x] **`exec.rs`**: `exec_shell` spawn `sh -c`/`cmd /C`,
   cwd=canonical_root, own pgid (`process_group`), env strip
   (OXIDEAN_*TOKEN*/KEY*/SECRET*/PASS* + bare `*PAIRING*`, no verifier
   in env ever); stdout/stderr pump threads into a shared ≤64 KiB
   tail buffer (UTF-8 char-boundary trim + `…<n> bytes truncated`);
   `EXEC_TIMEOUT` 300s deadline → group kill → `failed` "timeout";
   `killpg`/`taskkill /T` mid-flight kill; `fs_read`/`fs_write`
   jail-resolve chain (launch: canonical root is_dir + BOTH
   `is_within` directions vs canonical config_dir refuse; per-op:
   lexical normalize → escape refusal → canonicalize → root+cfg
   boundary checks → `symlink_metadata` non-regular refusal);
   `FS_IO_CAP` 128 KiB raw before read/write (`failed:too_large` with
   byte count); `fs_write` atomic tmp+rename; enumerated-value ≥8-char
   scrub set (verifier + unprefixed *TOKEN*/KEY*/SECRET*/PASS* env
   values + config.json instance tokens + pilot-secrets.json values)
   applied to `output` before report; universal `output` <
   `OUTPUT_MAX` assertion.

5. [x] **`state.rs` + run loop**: `RunState` {running map (≤1,
   insert-before-spawn), settled set (insert-after-report, evict when
   server stops re-announcing), backoff {fail_streak, delay
   min(60, 2^n × (1±0.2))}, canonical root, secret set}; outer
   `select!` {heartbeat.tick 15s | claim future | signal}; exec-phase
   nested `select!` {child | EXEC_TIMEOUT | heartbeat.tick | signal}
   — heartbeat-delivered cancels reach `running` mid-flight AND keep
   the 90s liveness window on >90s jobs; `apply_cancels` dedupe +
   `cancelled:pre-start`/`cancelled:mid-flight` close-out;
   `settled.contains` checks on exec-entry (skip cancelled batch
   members) and child-completion (suppress second report);
   `classify(C3Error, route) -> Disposition` TOTAL dispatch incl.
   pair-route rows (disabled/internal/transport/bad_input → verbatim
   reason Exit(1); rate_limited → honor header retry ONCE; loop-only
   codes on pair → Internal/Unknown arm Exit(1); pair-only codes on
   loop routes → Internal/Unknown backoff); `terminate(reason)` sole
   exit funnel (kill-first → report-or-not per cred liveness →
   custody deletes → pinned code); signal arm shares the kill
   machinery, exits 0, no cred deletes.

6. [x] **Verb implementations**: `pair` (--label/--caps/--verbose —
   NO `--root`; custody-first write then row; verbatim pair-error
   text); `run` (--root required --target-or-default
   --exec-timeout --wait-seconds --verbose; load row+verifier →
   canonical root → RunState → loop); `disconnect` (--target-or-
   default; local-forget ONLY — verifier deletes both backends +
   row delete; server-side sever is the owner's UI revoke — help
   text says so); `status` (--json rides Global flag; ALL rows
   always; metadata-only fields + live-probe `verifier:
   present|missing` + custody backend).

7. [x] **Deps**: `secrecy` (or `zeroize` newtype) + `base64` (pin to
   the workspace's existing usage) + the group-signal path
   (`rustix`/`libc` killpg or `kill` shellout — prefer the smallest
   dep that lands it); committed `Cargo.lock`.

8. [x] **Tests — `tests/pilot_connector.rs`** (wiremock, mirrors
   `tests/rpc_http.rs`): all four routes × wire shapes; every error
   code → classifier arm asserts; `retry_after`/`Retry-After`
   honored; cancel-set applied; pair error vocabulary. **Unit tests
   colocated in `src/pilot/`**: jail chains (escape/symlink/
   non-regular/config-dir/refuse-both-directions), env filter,
   tail-buffer bound + char-boundary trim, scrub set coverage,
   settled dedupe + exec-entry/completion suppression, custody
   backends on `tempfile` dirs (file fallback write/read/delete,
   corrupt-file named error, zombie detection), dispatch-table
   totality incl. pair-route rows.

9. [x] **Docs + registrations — same change**: `docs/CLI-PARITY.md`
   pilot rows `implemented` (NOT `ox-only` — the awk probes only
   `implemented`); `docs/CLI.md` `## Commands` gains the pilot
   group; `docs/CONNECTOR.md` runbook (pair→run, jail semantics,
   cap table, exit codes, custody model, disconnect-local-forget);
   `docs/TESTING.md` job-table row if the suite shape warrants;
   `.github/workflows/ci.yml` `rpc-sync` job gains ONE step:
   `OX_BIN=target/debug/ox ./scripts/check-cli-parity.sh` (the job
   already downloads `rust-bins` and sets `OXIDEAN_PREBUILT_BINS=1`
   — no new jobs, no ruleset edit). `crates/oxidean-cli/skill/
   SKILL.md` deliberately untouched (operator-run lifecycle, not
   agent-driven — recorded decision).

10. [x] **Record artifacts**: `unit-test-instructions.md` (exact
    commands), `code-summary.md`, `traceability.json`.

## Bounded invariants the implementer must not drift

- Wire constants: `OUTPUT_MAX` 256 KiB, `FS_IO_CAP` 128 KiB RAW
  pre-encode, exec tail 64 KiB, `EXEC_TIMEOUT` 300s, timeouts
  30s / wait+15s, `wait_seconds` [1,60], heartbeat 15s, backoff
  1s→×2→60s ±20% jitter BEFORE clamp, cancel-only/204 floor 1s.
- Flat `{"error":"<code>"}` decode FIRST, all 10 codes, route-scoped
  classification — never HTTP-status-only.
- `args` is a JSON-encoded STRING — double-parse outer then inner.
- Verifier: keyring `pilot/<instance>/<connector_id>` + file
  fallback; NEVER in `pilot-targets.json`/`config.json`/logs/
  `--json`/child env; `status` backend via live probe only.
- `settled` inserts AFTER report; exec-entry + completion consults;
  cancel close-out exactly once per id (claimed or not).
- One `select!` per loop level — nested exec select keeps heartbeat/
  cancel/signal alive mid-flight; `spawn_blocking` for fs_* ops.
- `terminate(reason)` is the ONLY exit path; delete order
  verifier-then-row; corrupt files are named errors, never zombies.
- `disconnect` = local forget only; docs/help say so verbatim.
- No clap, no new crates, no server-side changes, no new env vars;
  SKILL.md untouched; `required_procedures` returns `vec![]` for
  pilot arms (an `oxpilot.*` literal fails skill-sync-check).
