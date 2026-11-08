# Code Generation Plan — u3-vcs-object-store

## Deliverables

1. [x] **`crates/oxidean-vcs/Cargo.toml`** — `[features] default = ["client"]`, `client = []`, `server = [dep:oxidean-db, dep:sha2, dep:tokio, dep:futures-util, dep:bytes, dep:tracing, dep:chrono]`; dev-dep `tempfile` (per-crate pins TD3.2).
2. [x] **`crates/oxidean-vcs/src/lib.rs`** — doc header + `#[cfg(feature = "server")] pub mod object_store;` (never named unconditionally — client builds stay dep-free).
3. [x] **`crates/oxidean-vcs/src/object_store/paths.rs`** — `validate_hash` (len==64 && lowercase-hex before ANY join), `shard_path` = `{dir}/{h[0:2]}/{h[2:4]}/{h}`, tmp naming `{pid}-{counter}-{nanos}.tmp`.
4. [x] **`crates/oxidean-vcs/src/object_store/config.rs`** — `ObjectStoreConfig{objects_dir, max_object_bytes, repo_quota_bytes, gc_grace_secs, gc_interval_secs}` + `from_env` (u64 parse → Config on VarError; grace `[1, i64::MAX/1000]` chrono-ms bound; interval `[0, i64::MAX]`; byte caps ≤ i64::MAX; sibling-dir equality refusal vs resolved `OXIDEAN_LFS_DIR`/`OXIDEAN_PACKAGES_DIR`).
5. [x] **`crates/oxidean-vcs/src/object_store/mod.rs`** — `ObjectStore::{new, put, get, verify}`, `PutOutcome{bare hash, Stored|Dedup}`, `ObjectStoreError` closed enum, `VcsGcStats{orphans_removed, rowless_removed, tmp_removed, bytes_reclaimed, skipped, errors}`.
6. [x] **`crates/oxidean-vcs/src/object_store/gc.rs`** — `gc_once`, `clear_stale_lock`, `LockGuard` (sync remove_file on Drop — panic/abort-safe), age-stale lock + `set_modified` heartbeat (no truncate).
7. [x] **`crates/oxidean-db/src/vcs.rs`** — the two sanctioned additions ONLY: `vcs_get_object_link(repo_id, hash) -> Result<bool>` (`SELECT 1 AS ok FROM vcs_object_links WHERE repo_id = ? AND hash = ?` + fetch_optional + row.is_some()) and `vcs_delete_object_if_unlinked(hash) -> Result<bool>` (conditional DELETE + NOT EXISTS on `vcs_object_links` AND `vcs_file_changes` → rows_affected>0). Zero migrations.
8. [x] **Unit tests** — `#[cfg(all(test, feature = "server"))]` in-module; real sqlite `Database` + `tempfile` dirs (see unit-test-instructions.md matrix).
9. [x] **CI (`.github/workflows/ci.yml`)** — `:207` archive gains `--features server`; new `vcs-server` job (ubuntu-latest, alias checkout + toolchain + `nextest@0.9.146` + download `nextest-archive` → `-E 'package(oxidean-vcs)'`); `vcs-server` added to `rust-artifacts-cleanup` `needs` (:893 — it consumes the archive); `dead-code` job gains the clippy `--all-features` + rustdoc `--features server` lines. `docs/TESTING.md:258-259` table gains the `vcs-server` row.
10. [x] **`Makefile`** — `test` nextest arm + `cargo test` fallback arm, `dead-code-check`, `docs-check` lines exactly as cicd-pipeline.md pins.

## Pinned corrections (review findings — MUST implement)

- **`new()` ordering** (SD3.8): existing-regular-file ⇒ `Config` → non-empty (exempting only `.oxidean-vcs-objects` + `lost+found`) && no marker ⇒ `Config` → `create_dir_all` → write marker → `create_dir_all(tmp)`. Marker BEFORE tmp; zero writes before refusal.
- **`gc_once` ordering** (INF-era pin, NFR-review minor): absent dir ⇒ zero stats FIRST → marker gate → THEN `.gc-lock` acquire (lock never precedes the gates) → sweep.
- **PUT ordering** (SD3.3): stream→hash+count (count>max ⇒ tmp removed + `QuotaExceeded{Object}`) → finalize → rename atomic → `insert_object_if_absent` → `get_object_link` probe → linked ⇒ done → else `count_for_repo` quota → on `QuotaExceeded{Repo}`+Stored eager cleanup: `delete_object_if_unlinked` unlink file ONLY when true → else `insert_object_link_if_absent`. Bare hash return (no `sha256:` prefix — corrections register).
- **Fail-close** (SD3.7): `db.is_skipped()` ⇒ `DbNotConfigured` at put/get/gc entry. `From<io::Error>`: NotFound⇒NotFound else `Io`. `From<String>`⇒`Db` ONLY for facade results; stream errors construct `Stream(String)` explicitly. `From<VarError>`⇒`Config`.
- **GC reclaim** (SD3.5): conditional row delete BEFORE file unlink; `false` = protected (racing link / file_changes FK) silent-skip; DB probe `Err` in row-less sweep ⇒ keep file + `errors+=1` (an unconfirmable probe is never rowless); all remove_file `io::ErrorKind::NotFound`-tolerant; per-object logs `%hash`+`%error` only — no paths/env/content.
- **Lock** (SD3.4): `create_new(.gc-lock)`; AlreadyExists ⇒ mtime check `< gc_grace_secs` ⇒ `skipped=true` stats; stale ⇒ remove+retry once. Heartbeat = `OpenOptions::write` + `set_modified` — never `File::create` (no truncate).
- **Quota casts** (SD3.6): u64 config / i64 rows — checked `u64::try_from(i64)`; `size:u64→i64` bind safe only under `max_object_bytes ≤ i64::MAX`; `repo_quota_bytes=0` admits size-0 objects (pinned carve-out).
- **Permissions** (SD3.8): `0o600` renamed blobs, `0o700` tmp/+shard dirs — unix `#[cfg]` `from_mode` idiom.
- **Row-less sweep predicate** (SD3.2): `{2hex}/{2hex}/{64hex}` name-shape filter BEFORE stat; canonicalize + `starts_with(canonical_root)`; symlinks skipped no-follow; marker/lock/tmp/lost+found/dotfiles can't match the shape.
- **Doc tests**: none in the server module (nextest skips doc tests; `cargo test --doc` is default-features).
- **Carried doc nits** (infra iter-2): contributions file INF-11 line, jq pin :911, questions dep-kind wording — folded into code-summary, not re-edited upstream.
- **U4 invariant note** (WF3/C6a): no standalone insert_links_tx ships; `vcs_create_changeset_tx`'s `object_links` param is the mechanism — document in mod.rs docs that U4 must pass every manifest blob_hash.
- **`StreamExt` spelling** + `Internal(String)` variant consistent spelling throughout.

## Verification gates (order)

1. `cargo test --no-run -p oxidean-vcs --features server` (compile sweep incl. gated tests)
2. `cargo nextest run -p oxidean-vcs --features server` (full U3 suite green on sqlite+tempfile)
3. `cargo test -p oxidean-vcs --features server` (fallback arm parity)
4. `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`
5. `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server`
6. `cargo nextest run -p oxidean-db` (facade additions + no regression)
7. `cargo fmt --check`
8. `make dead-code-check` + `make docs-check` (lane parity)
9. `make rpc-sync-check` (no wire surface in this unit — regen-clean)

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
