**Collaborator:** aidlc-quality-agent

## Contribution

### Verified against the draft

All of the draft's Testing Posture claims hold up against the evidence:

- **test-after** is the real methodology. No red-first/TDD mandate exists in
  docs, hooks, or CI; `docs/CODE_PRACTICES.md` § "Testing expectations" and the
  PR-template test plan both presume implementation exists first. An in-repo
  `tdd` skill (`.agents/skills/tdd/SKILL.md`) exists as opt-in reference —
  it mandates seams-agreed red→green when invoked, but nothing invokes it
  by policy.
- **Weighted coverage gate**: `scripts/coverage-weighted.sh` composes
  unit 0.25 / integration 0.40 / e2e-checklist 0.35 against
  `COVERAGE_WEIGHTED_FLOOR=0.70`, set in `ci.yml` `coverage-weighted` job
  (line ~462). Contract self-test: `scripts/coverage-weighted.contract.sh`.
- **Change-aware manifests**: `route-coverage` and `browser-coverage` jobs
  resolve `UI_COVERAGE_BASE` to the PR base (push → `origin/main`) via
  `scripts/ui-coverage-diff.ts`; touched skip-only rows fail, and newly
  added high-risk `.tsrx` absent from the manifest fail outright.
- **Evidence is stronger than "has a test"**: `kind: "browser"` entries in
  `browser-coverage.manifest.ts` are source-verified — the test must call
  `mountComponent`/`mountWithQueryClient`, interact
  (click/pickSelectOptionByTestId), assert via
  `expectNoOctaneOverlayInDocument`/`expectNoDomRaces`/`trackDomErrors`,
  contain the `subject` marker, and reference the surface basename.
- **Rust lane**: one `cargo nextest archive` from `rust-build` feeds the
  5-way `api-rust` shards, `db-dialects`, and archive consumers — one
  compile per run. `ci` profile: `fail-fast=false`, 90s slow-timeout,
  `num-cpus` threads. Shards re-run the whole shard once on failure
  (coarse flake tolerance — no `--retries` scoping).
- **Vitest 5** with five projects in `apps/web/vitest.config.ts`
  (unit=node, integration=happy-dom `fileParallelism:false`, browser=
  Playwright Chromium, e2e-stack + e2e-stack-browser gated on
  `E2E_STACK=1`); `@oxidean/api-client` has its own node-env Vitest config
  with wire-version/fetch-header tests (`src/index.test.ts`).
- **Real dependencies over mocks where it matters**: Rust API tests open
  real SQLite tempdir DBs (`Database::connect` + `migrate`) and use
  wiremock only for outbound HTTP (webhooks, resend); protocol smokes run
  real `git`/`ssh`/`git-lfs` binaries against a live Compose stack, and
  are fail-closed in CI (`SMOKE_REQUIRE_STACK=1`).
- **Skip discipline is real**: zero `#[ignore]` and zero
  `it.skip`/`test.skip` in the tree; manifest `skip` rows are the only
  sanctioned skip, and they're policed change-aware.

### Corrections and gaps the draft understates

1. **Dialect parity is overstated.** "~30 dialect-parity files run across
   Postgres/MySQL/SQLite" is not what CI does. Every `dialect_*` file is
   `DATABASE_URL`-gated and **skip-passes green** when unset — which is
   the case in the `api-rust` shards. The `db-dialects` job runs only
   `-E 'binary_id(oxidean-db::dialect_probe)'` per dialect (3 tests).
   Real cross-dialect execution in CI is the probe alone; the rest of the
   parity suite exercises pg/mysql only when a developer exports
   `DATABASE_URL` locally. A new VCS crate with schema changes inherits a
   parity gate that mostly doesn't execute.
2. **The e2e third of the weighted gate is a presence checklist, not a
   coverage signal.** `scripts/coverage-e2e-checklist.sh` scores
   files-present/total over a hard-coded 7-path list — 1.0 today. E2E
   *correctness* is enforced by the separate `e2e-stack` / `actions-e2e` /
   `smoke-protocol` job gates, not by this score. If the intent adds new
   protocol surfaces, the checklist must be hand-extended or the 0.35
   weight ratifies nothing new.
3. **The weighted gate consumes web numbers only.** Unit/integration
   scores come from `apps/web` Vitest json-summaries; `cargo-llvm-cov`
   runs collection-only and uploads artifacts. A Rust-heavy intent (the
   new VCS crate/protocol path) ships code with **no enforced coverage
   signal at all** — the sharpest gap for this work.
4. **No test-placement gate exists.** There is no
   `scripts/check-test-placement.sh`; placement is enforced implicitly —
   vitest `include` globs silently ignore misnamed files, and knip only
   catches them as unused files. A misnamed `*.spec.ts`/`*.check.ts` never
   runs and fails only as dead code.
5. **No performance-test infrastructure.** No criterion benches, no
   k6/artillery, no load/soak harness anywhere in the repo. If the new
   protocol path carries NFRs (clone/fetch latency, throughput), there is
   no tooling precedent — the interview must decide whether that debt is
   accepted or the intent must bootstrap it.
6. **Local/CI asymmetry to keep honest.** `make test` runs only nextest +
   web unit/integration; browser, stack e2e, and smokes are separate
   targets — a green local `make test` proves far less than a green CI
   run. Smoke scripts skip-pass locally without Docker but fail-closed
   under `CI`.
7. **Org default vs repo reality conflict is sharper than flagged.** Org
   memory says `classic` adds an **80% line-coverage floor**; the repo's
   actual enforced gate is the 0.70 *weighted composite* on web only.
   These are different units (line % vs weighted score). The interview
   should affirm the repo's gate as the floor rather than inherit an 80%
   line floor nothing measures.

### Suggested `## Testing Posture` wording for team-practices.md

- **Methodology**: test-after. The `tdd` skill ships in-repo and may be
  invoked deliberately (e.g. for the protocol engine), but red-first is
  not the team's default mandate.
- **Ordering**: implement the change, then write/run the tests the
  change-type minimum requires (`docs/CODE_PRACTICES.md` § Testing
  expectations); the full CI suite must be green before merge.
- **Coverage**: `coverage-weighted` gates a weighted composite at floor
  **0.70** — web unit 25% + web integration 40% (v8 line coverage) + e2e
  35% (presence checklist). UI coverage is additionally **change-aware**:
  every new/changed `apps/web` route needs manifest evidence, and
  new/changed high-risk interactive UI needs real-Chromium
  `*.browser.test.tsx` proof — touched skip-only rows fail. Rust coverage
  is collected (`cargo-llvm-cov`, artifacts uploaded) but **not gated**;
  cross-dialect execution in CI is limited to `dialect_probe` — both are
  recorded debt this intent may need to close.
- **Tooling**: cargo-nextest (shared archive, 5-way shard, `ci` profile);
  Vitest 5 (unit/integration happy-dom, Chromium component tier, gated
  stack tiers); Playwright for real-browser e2e; wiremock for outbound
  HTTP; real git/ssh/git-lfs clients in protocol smokes; real SQLite
  tempdir DBs (not mocks) in Rust integration tests.
- **Test types / minimums**: per change type — pure helper → unit;
  Query/session UI → happy-dom integration; high-risk UI → Chromium
  browser test + manifest row; auth/RPC contract → Rust integration
  and/or stack e2e; dialect-touching DB code → `dialect_*` test (noting
  the CI-execution gap above).

## Positions

- AGREE: Methodology `test-after` — matches all evidence; no red-first
  mandate exists, and the shipped `tdd` skill is opt-in.
- AGREE: Weighted floor 0.70 and weights 0.25/0.40/0.35 — verbatim in
  `coverage-weighted.sh` and `ci.yml`.
- AGREE: Manifests make UI coverage change-aware with real teeth — the
  checkers verify mount/interaction/assertion markers, not just file
  existence.
- AGREE: Rust coverage collect-only is correctly labeled known debt.
- OBJECT: "~30 dialect-parity files run across Postgres/MySQL/SQLite" —
  overstated; `dialect_*` files skip-pass without `DATABASE_URL` and CI
  runs only `dialect_probe` per dialect. Reword to name the probe as the
  real gate and the rest as local-capable.
- OBJECT: Coverage bullet should say the e2e layer is a 7-path *presence*
  checklist and that the composite consumes web numbers only — otherwise
  readers assume executed e2e coverage and Rust coverage feed the score.
- OBJECT: The section omits that the intent's new Rust crate lands with no
  enforced coverage floor — flag it explicitly in the interview questions
  (it does appear in `evidence.md` open question 1; it deserves a sentence
  in team-practices.md's coverage bullet too).
