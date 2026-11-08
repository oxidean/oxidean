# Code Generation Plan — u6-vcs-git-interop

## Deliverables

1. [ ] **`crates/oxidean-git/src/backend.rs`** — `GitBackend` trait gains 8 methods (TD6.2 authoritative arities): `cat_commit(repo,sha)->RawCommit`; `commit_name_status(repo,base:Option<&str>,sha,max_bytes)->Vec<NameStatusRow>`; `tree_flat(repo,sha,max_bytes)->Vec<FlatTreeEntry>`; `rev_list(repo,tips:&[String],max)->RevList` (PARENTS-FIRST — backend reverses children-first `--topo-order` output in-crate, never `--reverse`); `verify_commit_oid(repo,rev)->Option<String>`; `check_refname(repo,refname)->bool`; `write_object(repo,bytes)->oid`; `write_commit(repo,entries,parents,author,committer,message)->oid`. `is_ancestor` NOT added (backend.rs:735 reused). New backend-facing types: `RawCommit`, `GitIdent` (byte-tolerant `Vec<u8>` fields), `NameStatusRow`/`NameStatusKind`, `FlatTreeEntry`/`TreeEntryKind`, `TreeWriteEntry`, `RevList{oids,truncated}`.
2. [ ] **`crates/oxidean-git/src/cli.rs`** — `CliGitBackend` impls for all 8 methods; shared `run_git_stdout_bounded` reader (incremental drain, kill+`GitError::Process("oxidean:stdout-bound-exceeded")` on overflow); `classify_push_error` ported to a shared private helper used by BOTH ref-move methods AND `commit_files`; `actor_capability: &str` param on `fast_forward_ref`/`force_update_ref` replacing the hardcoded `"admin"`; in-file test additions (cli.rs:3662-suite style).
3. [ ] **`crates/oxidean-vcs/src/git_interop.rs`** — NEW `server`-gated module: `GitInterop` (`new`/`import_chain`/`lazy_import`/`export`), `InteropCaps` + `validate`, `ImportPage`, `ImportHead`, `ExportOutcome`, `InteropError`, `ProtectionChecker` trait, internal `CursorState`/`TipSnapshot`/`ImportMarker`/`IdentSidecar`; private primitives `claim`, `release`, `decode_cursor`, `encode_cursor`, `link_lookup`, `linked_lookup`, `ident_sidecar_read`, `resolve_tips`, `import_one`, `is_ancestor_cs` (mirrored), `export_one`, `walk_export_set`, `sanitize_ident`/`refuse_ident` (mirrored `compose_author` guards), `is_git_oid` (40-only), mirrored `manifest_diff`/`full_tree`-accumulate; in-file `#[cfg(all(test, feature = "server"))]` suite (unit-test-instructions.md).
4. [ ] **`crates/oxidean-vcs/src/lib.rs`** — the `#[cfg(feature = "server")]` block gains `pub mod git_interop;` — one line, never unconditional.
5. [ ] **`crates/oxidean-vcs/Cargo.toml`** — `server` feature gains `oxidean-git` + `async-trait` ONLY (all codec deps — `base64`/`serde`/`serde_json`/`sha2`/`hex`/`bytes`/`tokio`/`tracing`/`futures-util`/`chrono` — already present; `sha2` mints the claim nonce over `{repo_id, started_at, u64::from(RandomState)}`, no `rand`/`uuid`).
6. [ ] **`crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0055_vcs_derived_index_nullable_covering.sql`** — three dialect files (auto-discovered, no manifest): pg `DROP NOT NULL`; mysql `MODIFY ... VARCHAR(32) NULL`; sqlite table-rebuild preserving FK + `ON DELETE CASCADE` AND recreating `idx_vcs_derived_index_covering`.
7. [ ] **`crates/oxidean-db/src/vcs.rs`** — `covering_changeset` `Option` BOTH sides: `VcsDerivedIndexUpsert.covering_changeset: Option<&str>` AND `VcsDerivedIndexRow.covering_changeset: Option<String>` + `map_index_row!` `try_get` decode (u2-claimed — recorded deviation).
8. [ ] **`crates/oxidean-core/src/vcs_types.rs`** — `VcsGitImportHead.changeset_id: Option<ChangesetId>`; `VcsGitImportResponse.skipped_tips: u64` (u1-claimed — recorded deviation, wire-superset).
9. [ ] **Compile-forced `Option` wraps** — `crates/oxidean-vcs/src/read_index.rs` (`Some()` at :727/:885/:904/:1500 + test :2300/:2503 + row-type `Option<String>` :243), `crates/oxidean-vcs/src/changeset_store.rs` (`Some()` :571), `crates/oxidean-db/tests/dialect_vcs.rs` (`Some()` at :96/:392/:398 + new NULL round-trip arms).
10. [ ] **Compile-forced `actor_capability` callers** — `crates/oxidean-api/src/repo/sync_fork.rs` (:227) + `crates/oxidean-api/src/mirror/engine.rs` (:536/:863/:991) all pass the `"admin"` literal (behavioral no-op).
11. [ ] **Zero edits to u1–u5 production logic** beyond the recorded deviations above — `merge_engine.rs`/`ref_registry.rs`/`object_store/` facades stay fingerprint-bound.

## Pinned corrections (review findings — MUST implement)

- **`is_skipped` FIRST at all three public entry points** (SR6.9/WF1.1/WF4.1): `DbNotConfigured` before ANY git argv, claim, or facade call. Fail-closed — tested per entry.
- **Caller-limit vs traversal-bound typing** (SD6.6): caller `limit ∉ 1..=import_commit_cap` ⇒ **`InvalidInput`**; every traversal/data bound (`rev_list_max`/`cursor_tips_max`/`import_tree_max`/`export_chain_max`/`export_ancestor_probe_max`/`cursor_payload_max_bytes`/`import_blob_max`/`export_blob_max`) ⇒ **`TooLarge`**. `import_commit_cap` overflow is the ONE `InvalidInput` trip.
- **Cursor decode = SD6.2 five-step pipeline** (in order, steps 1–3 before argv/indexing): (1) opaque `cursor.len() > cursor_payload_max_bytes` ⇒ `ImportState`; (2) b64url→JSON malformed ⇒ `ImportState`; (3) field revalidation `v==1`, `tips.len ≤ cursor_tips_max`, tip oids 40-hex non-dash-led, tip names ≤256 chars no NUL, `skip` deserializable ⇒ `ImportState`; (4) post-`rev_list` `skip > list.len()` ⇒ `ImportState` BEFORE indexing; (5) `skip>0` ⇒ `list[skip-1]==last_oid` continuity, `last_oid` oid-shaped. Encode-side applies the SAME field bounds — a >256-char refname refuses `TooLarge` at encode rather than minting an unresumable cursor; the emitted byte cap checks the **b64url opaque string** (not inner JSON).
- **Ident sanitize-then-refuse ONCE** (WF3.5/BR6.23): name/email sanitize `{control,0x7f}→'?'` FIRST, then refuse `<`/`>` in the sanitized part OR empty post-sanitize; composed `format!("{} <{}>", name.trim(), email)` `chars().count() ≤ 255`; `message.len() ≤ 65_535` bytes; `rfc3339(unix)` unrepresentable ⇒ `InvalidInput`. Mirrored `compose_author` guards (changeset_store.rs:439-461 private — mirrored, never edited). Every refused shape mints ZERO changeset rows.
- **Refusal inventory is TOTAL** (SR6.7): >64 parents, >65_535-byte message, unrepresentable timestamps, paths >512 chars/`..`/control-byte, author `<`/`>`/empty/composed >255 — each `InvalidInput`, no degradation, no changeset minted.
- **`ident-cs:` sidecar absence ⇒ WF5.3 fallback, NOT Internal** (LC6.7): `ident_sidecar_read` returns `Option<IdentSidecar>`; hit ⇒ RAW bytes; absent ⇒ `author→both`, `committer.date = committed_at` unix UTC; present-but-malformed ⇒ `Internal`. Absence is expected for every native changeset.
- **`IndexError` bridge = u5 parity** (LC6.7 supersedes FD-Q8): `CursorMalformed→Internal` (index cursors are interop-minted, never caller input), `FrontierTooLarge→TooLarge`, `DbNotConfigured→DbNotConfigured`, all others→`Internal`. NOT `ImportState`.
- **Bounded-reader sentinel** (SD6.4/SD6.7): `run_git_stdout_bounded` overflow ⇒ `GitError::Process` carrying literal `oxidean:stdout-bound-exceeded`; interop's `GitError` arm maps sentinel-bearing `Process`→`TooLarge`, other `Process`/`Io`→`Internal` opaque. `Denied`→`Forbidden{push_hook}`, `Conflict`→`RefConflict` (re-read forward link), `NotFound`→`NotFound`, `InvalidArg`→`InvalidInput`.
- **Export ordering is load-bearing** (SD6.5/WF4): changeset-side ff gate pre-mint → write FULL object chain → git-side `is_ancestor` fallback post-mint → `checker.check(repo, refname, old, minted_tip, actor)` → exactly ONE hooked classified move. The checker needs `(old, minted_tip)` — objects MUST write before the check; a denied verdict leaves only unreferenced gc-reaped objects (benign). `Err(_)` ⇒ `Internal` opaque.
- **Two-phase ff gate** (WF4.4/4.6): changeset-side `link_lookup(old)` FORWARD + `is_ancestor_cs`/`∈ set` ⇒ ff; else DEFER to post-mint git-side `is_ancestor(old, minted_tip)` ⇒ ff. Else split on provenance: UNLINKED old ⇒ `RefConflict{actual:None}` **even under force** (a git-only tip is never clobbered — force overrides ff-ness, not provenance-blindness); LINKED old ⇒ `force ? proceed : RefConflict{actual:old_cs}`. `actual` is `Option<ChangesetId>` — NEVER a git oid.
- **`ProtectionChecker` shape** (LC6.3): `async fn check(repo, refname, old: Option<&str>, new, actor_capability) -> Result<ProtectionBlockReasons, String>` — NO force param (intent = f(old,new) like `check_ref_update`, protection/mod.rs:842-856). Empty `reasons` ⇒ pass; non-empty ⇒ `Forbidden{verdicts}` wire-verbatim via `vcs_types.rs:394`. Hook-denied moves encode `ProtectionBlockReasons{reasons:["push_hook"], ..Default}` + fixed message.
- **Lazy import seeds `tips=[oid]` DIRECTLY** (WF2.3): bypasses `resolve_tips`/`from` resolution — a branch literally named the 40-hex target cannot shadow it. LOOP across pages: per-page `link_lookup(oid)` break ⇒ `{done:true,cursor:None}`; `page.done ∧ unlinked` ⇒ `Internal`; cursor carries forward. `verify_commit_oid(oid) None` ⇒ `NotFound`; `is_git_oid` is 40-only (64-hex ⇒ `InvalidInput`).
- **`linked:{cs}` reverse rows** (entities.md): written alongside EVERY `link_insert` — export anchors + ff gate + `walk_export_set` probes consume them; `verify_commit_oid` existence-probe treats gc'd anchors as unlinked (re-mint heals). `insert=false` with stored cs ≠ ours (divergent link) ⇒ `Internal`; same stored cs ⇒ early-skip idempotent.
- **Import marker protocol** (WF0/BR6.6): claim upserts `active{state:active,nonce,started_at}` with `covering=None` (NULL needs 0055) → re-read → foreign active ⇒ `ImportState`; TTL `import_lock_ttl_secs` stale reclaim. Release nonce-verified; mismatch ⇒ silent skip (never error); `ok` selects `idle`/`failed` terminal observability only.
- **Per-commit import** (WF3): `cat_commit` → parents `link_lookup` (miss ⇒ `Internal`) → `commit_name_status` + `tree_flat` (>import_tree_max ⇒ `TooLarge`) → `FileChangeInput`s (either-side gitlink ⇒ skip+`warn!`; `size > import_blob_max` ⇒ `TooLarge` PRE-READ; `cat_blob` → `objects.put` streamed) → refusal inventory → `store.create` → `ident:{oid}` + `ident-cs:{cs}` sidecars (`Some(cs)` covering; serialized > `cursor_payload_max_bytes` ⇒ `InvalidInput` pre-create) → caller `link_insert` + `linked:`.
- **Per-node export** (WF5): base git tree = `tree_flat(repo, parents[0]_git_oid, import_tree_max × 1024)` — parents[0] matches the import diff-base; merge parents contribute nothing. `index.tree` pages fold with cap-DURING-accumulation (≤`import_tree_max`); changed/new blobs via VCS-blob→git-oid seen-map then `objects.get` `take(export_blob_max+1)` → `write_object`; unchanged reuse base-flat oid. `mktree -z` + `commit-tree` via `write_commit` (`{:06o}` modes, stdin message — argv `-m` spawn-fails on NUL). Commit message verbatim.
- **Non-commit tip drops** (WF1.3/WF1.5): `None`-peeled tips (tag→tree/blob) drop + `warn!` + `skipped_tips += 1` — counted AND surfaced on `ImportPage`, never silent.
- **`-z` rename row order** (entities `NameStatusRow`): empirically `R<score>\0<src>\0<dst>\0` — THREE records, src-then-dst same as non-`-z` (verified against real `git diff-tree -z`; supersedes the reviewer's claimed reversal).
- **`-c` overrides are top-level argv** (SD6.4): `-c diff.renameLimit=10000` / `-c commit.gpgSign=false` appear BEFORE the subcommand token.
- **`check_refname` oracle, not `validate_treeish`** (SD6.4): `git check-ref-format` — u1's `validate_treeish` accepts `a//b`/`x.lock`/`a@{b}`; never reused.
- **Error/message hygiene** (SD6.7): `field: reason` messages only — no stderr, rejected values, paths, blob contents, identities, or auth material. `ChangesetId::parse` text embeds the raw id — DISCARDED; fixed `"changeset id: malformed"`. `InteropError` no `PartialEq` (u5 convention — `ProtectionBlockReasons` lacks it; recorded deviation, tests field-compare). Facade `String`/checker `Err`/`io::Error` ⇒ `Internal` opaque.
- **Error bridge TOTAL** (SD6.7): `ObjectStoreError` all 9 variants (`QuotaExceeded(scope)` preserved, `DbNotConfigured`, 7→`Internal`); `ChangesetError` (`ParentNotFound|ChangesetNotFound→NotFound`, `InvalidInput→InvalidInput`, `ObjectNotFound|Internal→Internal`, `DbNotConfigured`); `IndexError` per the u5-parity rule above; `GitError` all 6; `io::Error`; facade `String`; checker `Err`.
- **`InteropCaps::validate` at `new`** (SD6.1/LC6.2): `import_blob_max ≤ objects.config.max_object_bytes`, `export_blob_max ≤ same`, `*_max ≤ usize::MAX / 2048` (bounds the largest derived-bound multiplier — `×1024`/`×2048`/`×41` byte bounds can't wrap), zero caps ⇒ `InvalidInput` on host misconfig.
- **Observability** (SD6.8): `info_span!("vcs.git_interop", unit="vcs.git_interop", op, repo_id)`; `warn!` gitlink skips + non-commit drops; `info!` claim/release/page/outcome; `debug!` continuity checks — ids + counts only, NEVER payload/stderr/auth/ref-values.
- **Named residuals** (SR6.10 — document, don't silently fix): `list_refs` unbounded stdout (post-materialization cap only), per-move full-clone I/O, no `run_git*` timeout, cold `ReadIndex::tree` materialize, raced-empty parent walks.
- **Wire contract for u7** (LC6.6): `DbNotConfigured→db.not_configured`; `NotFound→vcs.not_found`; `InvalidInput→vcs.invalid_input`+m; `ImportState→vcs.import_state`+m (code EXISTS, vcs_types.rs:331); `Forbidden{verdicts}→vcs.forbidden`+`VcsForbiddenData`; `RefConflict→vcs.ref_conflict`+`VcsRefConflictData{actual:Option<ChangesetId>}`; `TooLarge→vcs.too_large`; `QuotaExceeded→vcs.quota_exceeded`; `Internal→rpc.internal` (app.rs:838 literal).
- **Commit shape**: production delta lands `feat(vcs):` SemVer-prefixed; artifact bookkeeping in the separate `docs(aidlc)` commit (u5 precedent).

## Source manifest write-set

`crates/oxidean-vcs/src/git_interop.rs`, `crates/oxidean-vcs/src/lib.rs`, `crates/oxidean-vcs/src/read_index.rs`, `crates/oxidean-vcs/src/changeset_store.rs`, `crates/oxidean-vcs/Cargo.toml`, `crates/oxidean-git/src/backend.rs`, `crates/oxidean-git/src/cli.rs`, `crates/oxidean-db/src/vcs.rs`, `crates/oxidean-db/tests/dialect_vcs.rs`, `crates/oxidean-db/migrations/postgres/0055_vcs_derived_index_nullable_covering.sql`, `crates/oxidean-db/migrations/mysql/0055_vcs_derived_index_nullable_covering.sql`, `crates/oxidean-db/migrations/sqlite/0055_vcs_derived_index_nullable_covering.sql`, `crates/oxidean-core/src/vcs_types.rs`, `crates/oxidean-api/src/repo/sync_fork.rs`, `crates/oxidean-api/src/mirror/engine.rs`.

## Verification gates (order)

1. `cargo test --no-run --workspace --features server` (compile sweep — catches every `Option`/`actor_capability` ripple)
2. `cargo nextest run -p oxidean-vcs --features server` (U6 suite + U3–U5 regressions)
3. `cargo nextest run -p oxidean-git` (new plumbing + classified move methods)
4. `cargo nextest run -p oxidean-db` (dialect_vcs NULL arms on sqlite; pg/mysql via docker compose per project memory — docker-first)
5. `cargo test -p oxidean-vcs --features server` (fallback arm parity)
6. `cargo clippy --workspace --all-targets -- -D warnings`
7. `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`
8. `cargo doc --workspace --no-deps`
9. `cargo doc -p oxidean-vcs --no-deps --features server`
10. `cargo machete` (both new deps must resolve as used)
11. `cargo fmt --check`
12. `make dead-code-check` (lane parity: knip + machete + clippy dead_code)

CI legs that gate this delta (no ci.yml edits): `rust-build` (workspace compile + nextest archive), `api-rust` (5 shards — `oxidean-git`/`oxidean-core`/`oxidean-api` surfaces), `vcs-server` (`-E 'package(oxidean-vcs)'` archive filter), `db-dialects` (`dialect_vcs` probes per dialect), `dead-code`/`rustfmt` arms.

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
