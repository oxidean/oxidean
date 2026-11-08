# Unit Test Instructions — u4-vcs-changeset-core

All crate tests live in `crates/oxidean-vcs/src/{changeset_store,ref_registry,read_index,text_diff}.rs` under `#[cfg(all(test, feature = "server"))]`. Real sqlite `Database` (existing fixture) + `tempfile` objects dir for read-side tests. Zero mocks — the facade contract is exercised against real schema incl. the 0054 sqlite parity no-op. `dialect_vcs.rs` coverage lives in `crates/oxidean-db/tests/dialect_vcs.rs` (DATABASE_URL-gated, not the crate suite).

## Exact command (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --features server          # primary
cargo test -p oxidean-vcs --features server                 # fallback arm
DATABASE_URL=sqlite:./var/u4.db cargo test -p oxidean-db --test dialect_vcs -- --nocapture
```

## Coverage matrix — changeset_store

| Test | Asserts |
|------|---------|
| `canonical_id_golden_vectors` | fixed input → fixed `cs_…` id, `manifest_hash`, `idem_key` (hand-computed SHA-256 vectors; repo-variance: same manifest different repo → different id) |
| `create_then_get_roundtrip` | row + file_changes + object_links all land in ONE tx; `vcs_get_changeset` reads back all fields |
| `idempotent_resubmit` | same input twice → same id, ONE row; `idem_key` match short-circuits before write |
| `concurrent_identical_creates` | `tokio::join` two creates same input → same id, one row |
| `concurrent_divergent_same_parent` | two creates same parent different manifests → distinct ids, both rows land |
| `empty_parent_bootstrap` | `parents: []` legal — creates root changeset |
| `missing_parent` | parent id not in table → `ParentNotFound` (sentinel `vcs.not_found:parent` decode) |
| `missing_object` | manifest blob not in `vcs_objects` → `ObjectNotFound{hash:Some}` after post-sentinel probe |
| `object_links_completeness` | every manifest `blob_hash` gets an object_links row (AC3.5.2) |
| `sentinel_nonmatch_exact` | `"vcs.not_found:parentXYZ"` / prefix/suffix variants → `Internal`, never typed decode |
| `skipped_db_fails_closed` | `DbNotConfigured` on create/get paths |
| `invalid_input_never_echoes` | `InvalidInput(msg)` carries reason, never the rejected path/mode/value |

## Coverage matrix — ref_registry

| Test | Asserts |
|------|---------|
| `ref_name_validation_arms` | every SR4.1 arm: regex pass/fail, `..`, `//`, leading/trailing `/`+`.`, `@{`, `HEAD`, `refs/` prefix, `cs_`-id-shaped (`ChangesetId::parse`), pure 40/64-hex |
| `cas_all_outcome_arms` | create-only (None→Some absent row); move (Some→Some matching expected); delete (Some→None); absent-delete → `RefNotFound`; assert-absent noop (None→None → Ok, no row); retried-CAS conflict (expected≠actual → `RefConflict{actual}`) |
| `cas_no_preread_toctou` | conflict path returns `actual` from the CAS row itself (single round-trip) |
| `ref_list_and_resolve` | list returns rows; `resolve` maps name→head; id-shaped `from` routed to `resolve_git_oid` not refs |
| `mysql_race_divergence_documented` | (doc-only note — the R18 raw-Err→Internal arm is MySQL-only; sqlite can't reproduce create-race) |

## Coverage matrix — read_index

| Test | Asserts |
|------|---------|
| `log_walk_cursor_no_dup` | multi-page log walk over same-second commits — zero duplicates across pages; `next_cursor {"n"}` resumes |
| `log_from_ref_and_git_oid` | `from` resolves ref-name → head; 40/64-hex → `resolve_git_oid` |
| `log_frontier_cap` | injected `seen_max=4` → `FrontierTooLarge` |
| `merge_base_cases` | linear→self; divergent→common ancestor; no-ancestor→`BaseNotFound`; `(lo,hi)` orientation cached |
| `tree_materialize_and_crash_resume` | tree rows materialized; `tree_snap` written last; partial state (entries w/o snap) re-materializes on next call |
| `tree_entry_filter_raw_suffix` | `path_prefix` filters on index_key suffix (payload has no path field) |
| `file_at_and_delete` | file read at cs; deleted path → `PathNotFound` |
| `path_history_rename_visibility` | `path_touch` + `\x01r` rename rows both visible; rename chain follows `old_path` |
| `payload_path_self_describe_guard` | injected mismatched `payload.path` row via `vcs_upsert_derived_index_rows` → excluded |
| `blame_attribution_rename_chain` | lines attribute to introducing changeset; rename-chain touches collected ≤ `blame_touches_max` |
| `blame_shard_cap` | shard payloads ≤32KB; oversize → next chunk row |
| `diff_path_ordered` | candidates emit in `BTreeSet` path order; cursor `{"k":path}` resumes |
| `diff_candidates_cap` | injected `diff_candidates_max` → `FrontierTooLarge` |
| `existence_check_every_walk` | missing changeset input → `NotFound`/`ChangesetNotFound`, never empty results (U4-AR-02) |
| `cursor_malformed` | garbage base64 / >1KB / wrong shape → `CursorMalformed` |
| `mark_max_cap` | injected `mark_max` → `FrontierTooLarge` on merge_base |
| `skipped_db_fails_closed` | `DbNotConfigured` |

## Coverage matrix — text_diff

| Test | Asserts |
|------|---------|
| `golden_empty_to_full` | `[]` → lines = one add hunk |
| `golden_full_to_empty` | lines → `[]` = one delete hunk |
| `prepend_append_middle` | hunk positions correct (old_start/new_start/len) |
| `shifted_not_edited` | rename-relevant: lines moved → minimal hunks, not whole-range replace |
| `binary_null_lines` | `\0`-bearing `&[u8]` lines diff verbatim (no UTF-8 assumption — U4-AR-06) |
| `crlf_verbatim` | `\r\n` endings preserved in hunk content identity |
| `line_cap_coarse_fallback` | `old.len()+new.len() > 200_000` → single whole-range replace hunk |

## Gaps carried forward

- pg/mysql execution only via the widened `db-dialects` leg (single-commit landing) — local via `DATABASE_URL=…`.
- The R18 MySQL CAS create-race `Err`→`Internal` arm is unreproducible on sqlite; covered by code inspection + dialect test's happy-path CAS arms.
- Cold tree/file folds O(history) — SR4.11 registered residual; test asserts correctness not asymptotics.
- Rust coverage collected-not-gated until U7 feature unification.
- `versioning.toml depends_on` + `Makefile db-matrix` edits deferred to build-and-test (fingerprint-bound; infra cicd-pipeline deferral section).
