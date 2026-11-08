# Code Generation Plan — u4-vcs-changeset-core

## Deliverables

1. [ ] **`crates/oxidean-vcs/Cargo.toml`** — `server` feature gains `dep:serde`, `dep:serde_json`, `dep:base64` (`"0.23"`), `dep:hex` (`"0.4"`), `dep:oxidean-core` (path; promoted from dev-dep). `client` stays dep-free (LC4.7).
2. [ ] **`crates/oxidean-vcs/src/lib.rs`** — `#[cfg(feature = "server")] pub mod {changeset_store, ref_registry, read_index, text_diff};` — never unconditional.
3. [ ] **`crates/oxidean-vcs/src/changeset_store.rs`** — `ChangesetStore`, `CreateChangesetInput`, `FileChangeInput` (defined HERE — read_index imports it), `ChangesetRecord`, `ChangesetError` (closed enum, exact variants below), `canonical_changeset()` (the single impl — U5/U6/U7 import it), `resolve_git_oid` (pub — read_index's `from` hex arm uses it), private `is_changeset_id`, sentinel decode.
4. [ ] **`crates/oxidean-vcs/src/ref_registry.rs`** — `RefRegistry`, `Ref` (domain shape, not wire), `RefError`; `resolve` is `pub(crate)` (read_index `from` ref arm); CAS arms per SD4.6.
5. [ ] **`crates/oxidean-vcs/src/read_index.rs`** — `ReadIndex`, `ReadIndexCaps` (injectable), `LogEntry/DiffEntry/TreeEntry/FileEntry/PathTouch/BlameLine`, `IndexError`; `update_for` is a **static `pub(crate)` fn returning owned `PathTouchRow`** (U4-AR-01); per-walk existence check (U4-AR-02).
6. [ ] **`crates/oxidean-vcs/src/text_diff.rs`** — `pub fn diff_lines(old: &[&[u8]], new: &[&[u8]]) -> Vec<LineHunk>` (byte slices — lossless non-UTF-8, U4-AR-06); `LineHunk{old_start,old_len,new_start,new_len}`; LINE_CAP=200k coarse fallback.
7. [ ] **`crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql`** — MySQL: four `ALTER TABLE … MODIFY COLUMN` re-declarations (`index_key VARCHAR(560)`, `refs.name`, `file_changes.path`, `file_changes.old_path NULL` — all `CHARACTER SET utf8mb4 COLLATE utf8mb4_bin`); pg/sqlite: comment-only parity files (LC4.7).
8. [ ] **`crates/oxidean-db/tests/dialect_vcs.rs`** — `DATABASE_URL`-gated skip-pass, `SERIAL` mutex, `db.migrate()` then facade exercise: `vcs_create_changeset_tx` round-trip, all CAS arms, sentinel text, case-variant path/ref/`old_path` under 0054 collation, `information_schema` collation assert (dialect_probe.rs:8-26 contract).
9. [ ] **`.github/workflows/ci.yml`** — `db-dialects` probe `-E` filter → `'binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)'` (:810); `:814-815` comment refresh.
10. [ ] **`docs/TESTING.md`** — db-dialects row updated to the qualified `or` filter form (:271).
11. [ ] **Unit tests** — `#[cfg(all(test, feature = "server"))]` in-module; real sqlite `Database` + `tempfile` objects dir (unit-test-instructions.md matrix).

## Pinned corrections (review findings — MUST implement)

- **Canonical ID** (SD4.2/LC4.2): `cs_` + 26 uppercase Crockford chars from first 128 bits of `SHA-256(canonical_input)`; canonical input = `{repo_id, parents(sorted), manifest_json, author, committed_at, message}`; `manifest_json` is hash material only — store `manifest_hash`, never `manifest_json`. `idem_key` = full 256-bit digest. `ChangesetId::parse` is the ID-shape oracle (oxidean-core); private `is_changeset_id` wraps it.
- **One transaction** (SD4.3): validate ALL inputs BEFORE any DB call; then `vcs_create_changeset_tx(changeset_row, file_changes, object_links, derived_index_upserts)`; idempotent resubmit = `vcs_find_changeset_by_idem` probe → same `idem_key` returns existing record without writing. Parents ≤64, each must exist (sentinel `vcs.not_found:parent` → `ParentNotFound`; empty parents = bootstrap allowed). Object existence post-sentinel probe → `ObjectNotFound{hash}`.
- **Sentinels** (SD4.5): exactly four literals, whole-string exact match: `vcs.not_found:parent`, `vcs.not_found:object`, `vcs.invalid_input:parents`, `vcs.invalid_input:file_change`. All other DB errors → `Internal`. MySQL CAS create-race raw `Err("vcs cas_ref create: …")` → `Internal` (R18 — NOT a fifth sentinel).
- **Error enums** (LC4.6 — closed sets, exact shapes):
  `ChangesetError::{ParentNotFound, ObjectNotFound{hash:Option<String>}, ChangesetNotFound, InvalidInput(String), Internal(String), DbNotConfigured}`
  `RefError::{RefNotFound, RefConflict{actual:String}, TargetNotFound, InvalidInput(String), Internal(String), DbNotConfigured}`
  `IndexError::{NotFound, PathNotFound, BaseNotFound, FrontierTooLarge, CursorMalformed, InvalidInput(String), Internal(String), DbNotConfigured}`
  `InvalidInput(String)` carries a reason, never the rejected value.
- **CAS mapping** (SD4.6): `Applied(row)` / `Conflict{actual:Option<String>}`; `expected_old=None,new=None` = assert-absent no-op → `Ok`; absent delete/move → `RefNotFound`; `actual:Some` → `RefConflict{actual}`; `actual:None` → unreachable absent/NULL-head → `RefNotFound`. No pre-read TOCTOU.
- **Ref names** (SR4.1): `^[A-Za-z0-9][A-Za-z0-9._/-]{0,254}$` + reject `..`, `//`, leading/trailing `/` or `.`, `@{`, `HEAD`, `refs/` prefix, `ChangesetId::parse`-shaped, pure 40/64-hex.
- **Derived-index keys** (SD4.9): separator `\x01`; `path_touch` = `{sha256(path)[0:32]}\x01{committed_at}\x01{cs}` + rename old-path row `…\x01r` (AR-01); `tree_entry` = `{cs}\x01{path}`; `tree_snap` = `{cs}`; `merge_base` = `{lo}\x01{hi}`; `blame` = `{at}\x01{hash32}\x01{chunk:04}`. `old_path` unique across manifest (SR4.1); `old_path` need not be disjoint from `path`. Payload field = `changeset_id`.
- **update_for ownership** (U4-AR-01): static `pub(crate) fn ReadIndex::update_for(input) -> Vec<PathTouchRow>` — owned rows; `ChangesetStore` borrows them at the `create_tx` call site (no borrowed-locals).
- **Walks** (SD4.8): `log` = `BinaryHeap<(String,String)>` keyed raw `(committed_at,id)` strings (no `Ord` on `ChangesetId`); `merge_base` mark+level BFS capped `mark_max`, `(lo,hi)` orientation, cache row; `diff` `BTreeSet` path-ordered candidates; `blame` touch cap `blame_touches_max`, order by ancestry position, `ObjectStore::get → File → bytes → split b'\n'`; `tree`/`file_at` first-parent fold, `tree_entry` rows first, `tree_snap` LAST (crash-resume); existence check via `vcs_get_changeset` before EVERY read walk (U4-AR-02) — missing inputs never degrade to empty results.
- **Caps** (SD4.8): `ReadIndexCaps{seen_max:65536, mark_max:250_000, diff_candidates_max:250_000, blame_touches_max:65536}` — injectable for tests; breach → `FrontierTooLarge`.
- **Cursors** (SD4.10): base64url-nopad compact JSON ≤1KB; shapes `{"n":int}` / `{"k":key}` / `{"k":path}` / `{"k":shard,"off":int}` / `{"k":refname}`; malformed → `CursorMalformed`.
- **Validation** (SR4.1): paths reject NUL/ctrl, `..`/`//`, leading/trailing `/`, `.`/`..` segs; mode ∈ `{33188,33261,40960,16384}`; parents ≤64; message ≤65535; author ≤255; path ≤512; committed_at `%Y-%m-%dT%H:%M:%SZ` no fractions; payload < MySQL TEXT 64KB; blame shard ≤32KB.
- **Reads of hashed-path rows** (SR4.10): `path_touch`/blame payloads self-describe `path`; verify `payload.path == arg` post-decode (hash collision guard). `tree_entry` filters on the raw path suffix of `index_key` — its payload is `{mode,blob}` only.
- **Object reads** (SD4.8): `ObjectStore::get` returns `tokio::fs::File` → `AsyncReadExt` → bytes → split `b'\n'` → `&[u8]` lines into `text_diff`; `ObjectStoreError::{NotFound→Internal (defensive), DbNotConfigured→DbNotConfigured, _→Internal}`. No object reads during changeset creation (SR4.9 — objects validated by row existence, never bytes).
- **Observability** (SR4.12): `tracing` at sentinel decode, CAS conflict, `FrontierTooLarge`, `CursorMalformed`, target/ref conflict; index-write counts on create/materialize/blame. IDs/hashes/counts only — NEVER paths, messages, author payloads (gc.rs precedent).
- **Fingerprint-bound deferrals** (infra cicd-pipeline): `versioning.toml depends_on` and `Makefile db-matrix` are NOT in this commit — deferred to build-and-test post-unit-close.
- **Commit shape**: everything above lands in ONE commit (sequencing constraint — filter+test+migrations+crate+docs atomic); `feat(vcs):` scope for crate+migrations+dialect test, or split `feat(db)`/`feat(vcs)`/`ci`/`docs` per house convention (u3 precedent: separate per-scope commits allowed when the tree lands green as a unit).

## Source manifest write-set

`crates/oxidean-vcs/{Cargo.toml, src/changeset_store.rs, src/ref_registry.rs, src/read_index.rs, src/text_diff.rs, src/lib.rs}`, `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql`, `crates/oxidean-db/tests/dialect_vcs.rs`, `.github/workflows/ci.yml`, `docs/TESTING.md`, `Cargo.lock`. **NOT** `versioning.toml`, **NOT** `Makefile` (deferred — see above).

## Verification gates (order)

1. `cargo test --no-run -p oxidean-vcs --features server` (compile sweep)
2. `cargo nextest run -p oxidean-vcs --features server` (full crate suite green — U3+U4 tests)
3. `cargo test -p oxidean-vcs --features server` (fallback arm parity)
4. `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`
5. `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server`
6. `cargo nextest run -p oxidean-db` (migration compile + facade non-regression)
7. `DATABASE_URL=sqlite:./var/u4.db cargo test -p oxidean-db --test dialect_vcs -- --nocapture` (local dialect exec — sqlite leg)
8. Docker: `docker compose` mysql/postgres + `DATABASE_URL=… cargo test -p oxidean-db --test dialect_vcs` (real-dialect exec where practical)
9. `cargo fmt --check`
10. `make dead-code-check` + `make docs-check` (lane parity)

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
