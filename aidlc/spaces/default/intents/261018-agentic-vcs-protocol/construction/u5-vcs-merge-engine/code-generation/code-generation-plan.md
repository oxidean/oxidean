# Code Generation Plan — u5-vcs-merge-engine

## Deliverables

1. [ ] **`crates/oxidean-vcs/src/merge_engine.rs`** — `MergeEngine`, `MergeCaps`, `MergeOutcome`, `MergeError`, `ConflictRegion`, `Merge3Fail`, `ConflictKind`, `Classified`, internal `TreeMap`/`SideManifest`/provenance types; private primitives `row_probe`, `full_tree`, `is_ancestor`, `side_manifest_pair`, `compose_provenance`, `classify_path`, `is_text`, `read_blob_capped`, `merge3` + `run_merge3` (JoinHandle seam — codegen MUST), `manifest_diff`, `local_blob_id`; in-file `#[cfg(all(test, feature = "server"))]` suite (unit-test-instructions.md).
2. [ ] **`crates/oxidean-vcs/src/lib.rs`** — the existing `#[cfg(feature = "server")]` module block gains `pub mod merge_engine;` — one line, never unconditional.
3. [ ] **Zero Cargo.toml delta** — `hex`/`sha2`/`tokio`/`futures-util`/`bytes` all already `server` deps (TD5.1).
4. [ ] **Zero edits to u1–u4 sources** (BR5.19) — `changeset_store.rs`/`ref_registry.rs`/`read_index.rs`/`text_diff.rs`/`object_store/`/`vcs.rs`/facades are fingerprint-bound to other units.

## Pinned corrections (review findings — MUST implement)

- **Entry pipeline** (SD5.2, WF1.1 order — bounds LAST): `db.is_skipped()` → `DbNotConfigured` FIRST; `ChangesetId::parse` per `&str` id arg → `InvalidInput` with fixed `"changeset id: malformed"` (parse's own error text embeds the raw id — DISCARD it); `vcs_get_changeset` row probes → `NotFound`; mirrored `compose_author` bounds (`changeset_store.rs:439-461` exactly: `name.trim()` nonempty, `email` nonempty NOT trimmed, no `<` `>` `\n` `\r` bytes `<0x20` `0x7f`, `composed = format!("{} <{}>", name.trim(), email)` then `composed.chars().count() ≤ 255` CHARS, `message.len() ≤ 65_535` BYTES incl. synthesized default) — pre-check at entry AND re-check at WF1.9/WF5.5 so no caller `InvalidInput` can fire after pass-2 `put`s.
- **&str params** (SD5.2): `merge(repo_id, a:&str, b:&str, author_name:&str, author_email:&str, message:Option<String>)`, `rebase(repo_id, cs:&str, new_parent:&str, author_name:&str, author_email:&str, message:Option<String>)` — the engine owns parsing; typed `ChangesetId` params would push parse to u7 where the raw value leaks.
- **Two-pass write discipline** (SD5.3 — THE central shape): `ObjectStore::put` links objects on its success path; linked objects are never GC-eligible; no unlink facade exists. PASS 1 = zero `put`s: `full_tree`×3 → `side_manifest_pair` → rename pass → per-path `classify_path` → merge3 on blob-diverged paths → record `{path → (mode, local sha256)}` — merged bytes DISCARDED. Checkpoint: conflicts → `MergeError::Conflict` (zero rows, zero links) → `manifest_diff` → `changes.len() ≤ merge_paths_max` → `TooLarge` → deterministic input re-check. PASS 2 (only after checkpoint): per merged path re-read all three inputs via `read_blob_capped` (no pass-1 input caching), re-run `run_merge3` (deterministic → identical bytes), `put(stream, repo_id)`, assert `PutOutcome.hash == pass-1 sha256` → `Internal` on mismatch; then `store.create`. Residual post-checkpoint surface is infra-only (named in SR5.11).
- **MergeCaps + validate** (LC5.2/5.3): `{merge_blob_max:u64=4MiB, merge_paths_max:usize=10_000, ancestor_probe_max:usize=250_000, side_walk_max:usize=25_000, rename_probe_max:usize=10_000, merge_conflict_max:usize=500}`; `Default` impl; `validate(&self, objects:&ObjectStore)` pins `merge_blob_max ≤ objects.config.max_object_bytes` (`config` is `pub(crate)`), `merge_blob_max < u64::MAX` (the `take(max+1)` overflow), `merge_paths_max ≤ 10_000` (Q9 client-parity — `create` has no manifest cap) → `InvalidInput` per violation. Constructor-injectable for cap tests.
- **Traversal bounds** (SD5.4): `is_ancestor`/`side_manifest_pair` mark BFS ≤ `ancestor_probe_max`; each side-exclusive walk ≤ `side_walk_max`; collected `kind=="rename"` rows across each whole side ≤ `rename_probe_max` (per-node loads uncapped — SR5.11); `full_tree` accumulates ≤ `merge_paths_max` with mid-walk abort; blob read `take(merge_blob_max.saturating_add(1))`; merged output ≤ `merge_blob_max`; emitted manifest ≤ `merge_paths_max`; conflict list collects `merge_conflict_max`+1 (the +1 drives `truncated`, NOT `merge_conflict_max` — LC5.8 supersedure).
- **Provenance** (SD5.5): ONE shared mark phase `base ∪ ancestors(base)`; per-tip walks skip marked nodes; per-node `vcs_list_file_changes` rows accumulated. `compose_provenance`: transitive chain compose (`old→p→q` ⇒ `prov[old]=q`), cycles dropped, multi-writer `old` keys dropped (ambiguous ⇒ no provenance), final target must exist in the side's tip tree. Detection ORDER: rename/rename cross-path pass FIRST (claimed targets suppress per-path re-emission) → rename-vs-delete (`old` renamed by one side, absent on other while `∈ b_t` → `rename` conflict on renaming side's target) → per-path classify (rename-vs-modify stays `delete` kind on `old`, `detail` names target). Emission: `prov[old]==p` ∧ `old ∉ merged` ∧ `old ∈ first_tree` ∧ `(mode,blob)` == provenance-side tree; multi-source same-target ⇒ byte-first `old`; delete arm skips emitted-rename `old`s — emitted-rename set FULLY POPULATED before delete arm evaluates (phantom `delete{old}` falsifies `path_touch`/`blame`). Rebase: provenance from `vcs_list_file_changes(cs)` ONLY — never a target-side walk.
- **merge3 byte-faithfulness** (SD5.6): `split_lines` drops terminators/trailing-empty → `ends_nl = buf.last() == Some(&b'\n')` caller-derived (empty ⇒ false); byte-count `b'\n'` per side via `iter().filter` (memchr NOT a dep — LC5.8) BEFORE `split_lines`: `(n_base + n_side + 2) > LINE_CAP` → `Merge3Fail::TooLarge` (the +2 covers n+1 lines per unterminated buffer); emission `line + b"\n"` iff `i < len-1 || ends_nl`; last-line sameness compares `(line_bytes, ends_nl)` PAIRS — trailing-newline-only divergence ⇒ `content` conflict; `Merge3Fail{TooLarge | Regions(Vec<ConflictRegion>)}` — `Regions` → `content` conflict.
- **`run_merge3` JoinHandle seam** (LC5.4 — codegen MUST): private `fn run_merge3(base,o,t) -> JoinHandle<Result<Vec<u8>, Merge3Fail>>` wrapping `spawn_blocking` — the BR5.10 interleave-probe test binds to this handle await; inline-in-awaitable would evade detection.
- **Error bridge** (SD5.7 — complete): `IndexError::{BaseNotFound→NoCommonAncestor, NotFound|PathNotFound→NotFound, FrontierTooLarge→TooLarge, CursorMalformed→Internal (engine-minted cursor = corruption), InvalidInput→InvalidInput, DbNotConfigured→DbNotConfigured, Internal→Internal}`; `ChangesetError::{ParentNotFound|ChangesetNotFound→NotFound, ObjectNotFound→Internal, InvalidInput→InvalidInput, DbNotConfigured→DbNotConfigured, Internal→Internal}`; `ObjectStoreError::{QuotaExceeded(scope)→QuotaExceeded(scope) scope PRESERVED, DbNotConfigured→DbNotConfigured, NotFound|InvalidHash|Stream|Config|Db|Io|Internal→Internal}`; `io::Error` (blob read_to_end) → `Internal`; facade `String` failures → `Internal` (opaque); `JoinError` → `Internal`.
- **MergeError shape** (LC5.3): `NotFound | InvalidInput(String) | NoCommonAncestor | TooLarge | QuotaExceeded(QuotaScope) | Conflict(Vec<VcsConflictEntry>) | DbNotConfigured | Internal(String)`; `Display` for `Conflict` renders `"conflict: {n} paths"` — COUNT ONLY (paths ride the typed envelope; `warned()` logs `%e`); `VcsConflictEntry.detail` carries PATH text only, truncated ≤200 chars on a char boundary, NO per-detail wire flag; `MergeError` does NOT derive `PartialEq` (`VcsConflictEntry` lacks it — recorded deviation, tests field-compare).
- **Merge semantics** (WF1): `is_ancestor(a,b)` → `FastForward{b}`; `is_ancestor(b,a)` → `FastForward{a}`; `a==b` → `FastForward{a}`; all fast paths emit ZERO new rows; merge parents `[a,b]`; manifest relative to first parent `a`; `merge_base` may return dominated ancestor — legal for base-tree selection, NEVER for ancestry decisions (engine's own bounded BFS decides). `create{parents:[a,b], author_*, message: message.or("merge {b} into {a}"), committed_at:None, changes}`.
- **Rebase semantics** (WF5): `old_base = parents(cs)[0]` or ∅ (root: `full_tree(None)` returns empty map WITHOUT calling `tree()`); `new_parent == old_base` → `FastForward{cs}`; `is_ancestor(cs,new_parent)` → `InvalidInput` (descendant/cycle); ancestor rollback + disjoint targets LEGAL; merge-node source replays against `parents[0]` (extra merge parents dropped); `create{parents:[new_parent], author_*, message: message.or(cs.message), committed_at:None, changes}`.
- **Classified shape** (LC5.4): `Classified = Resolved(Option<TreeEntry>)` (None = clean deletion) | `Conflict(VcsConflictEntry)`; `classify_path(b,o,t,prov:&SideManifest)` — `prov` carried so rename-vs-modify emits `delete` on `old` with `detail="renamed to {prov[old]}"`; WF2 order: `type` before `mode` before byte reads.
- **Wire contract** (LC5.7 — u7 consumes): `NotFound→vcs.not_found`; `InvalidInput→vcs.invalid_input`+m; `NoCommonAncestor→vcs.no_common_ancestor`; `TooLarge→vcs.too_large`; `QuotaExceeded(scope)→vcs.quota_exceeded`; `Conflict→vcs.conflict`+`VcsConflictData{conflicts,truncated}`; `DbNotConfigured→db.not_configured`; `Internal→internal` envelope.
- **is_text** (BR5.5/WF0.13): `false` iff `0x00` in first 8 KiB; non-regular modes (symlink/tree) use pick rules regardless of bytes.
- **No ref/store writes** (BR5.12): engine NEVER touches `vcs_refs`; writes only through `ChangesetStore::create` + `ObjectStore::put`. No new facade methods, no new index kinds, no host-crate deps.
- **Observability** (SD5.8): `tracing` on typed-error returns (walk-cap trips, conflict counts, spawn failures, hash-assert mismatches) — ids + counts only, NEVER paths/messages/author/blob payload.
- **Stale-FD supersedures** (LC5.8): conflict collect is `merge_conflict_max`+1 (NOT `merge_conflict_max`); dual validation is the pin (NOT "validated once"); `Merge3Fail` is the inner error (NOT `Vec<ConflictRegion>`); no memchr.
- **Commit shape**: `crates/oxidean-vcs/src/merge_engine.rs` + `lib.rs` line land in ONE `feat(vcs):` commit (u3/u4 precedent — artifact bookkeeping in the separate `docs(aidlc)` commit).

## Source manifest write-set

`crates/oxidean-vcs/src/merge_engine.rs`, `crates/oxidean-vcs/src/lib.rs`. Nothing else — no Cargo.toml/Cargo.lock/ci.yml/docs edits, no migrations.

## Verification gates (order)

1. `cargo test --no-run -p oxidean-vcs --features server` (compile sweep)
2. `cargo nextest run -p oxidean-vcs --features server` (full crate suite — U3+U4 regressions + U5 suite green)
3. `cargo test -p oxidean-vcs --features server` (fallback arm parity)
4. `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`
5. `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server`
6. `cargo nextest run -p oxidean-db` (facade non-regression)
7. `cargo fmt --check`
8. `make dead-code-check` + `make docs-check` (lane parity)

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
