# Code Summary — u6-vcs-git-interop

## What landed

`crates/oxidean-vcs/src/git_interop.rs` (~1,650 lines of production code + ~650 lines of in-file tests), plus the cross-crate seam work the approved design pinned:

- `crates/oxidean-vcs/Cargo.toml` + `lib.rs` — `server` feature gains `async-trait`, `oxidean-core`, `oxidean-db`, `oxidean-git`; `pub mod git_interop;` under `#[cfg(feature = "server")]`.
- `crates/oxidean-git` — eight new `GitBackend` methods (`verify_commit_oid`, `check_refname`, `cat_commit`, `commit_name_status`, `tree_flat`, `rev_list`, `write_object`, `write_commit`) + `is_ancestor` + the bounded-stdout helper (`STDOUT_BOUND_SENTINEL` = `oxidean:stdout-bound-exceeded`) + `actor_capability` plumbed through `commit_files`/`fast_forward_ref`/`force_update_ref` so hook enforcement sees the real actor.
- `crates/oxidean-db` — `covering_changeset` → `Option` (row + upsert), migration `0055_vcs_derived_index_nullable_covering.sql` for all three dialects (sqlite table-rebuild; pg/mysql ALTER), dialect round-trip coverage for NULL covering.
- `crates/oxidean-core` — `VcsGitImportHead.changeset_id` → `Option<ChangesetId>`; `VcsGitImportResponse.skipped_tips: u64`.
- `crates/oxidean-api` — `mirror/engine.rs`, `repo/sync_fork.rs` pass `"admin"` for system ref moves (user-attributed ops keep the real capability).
- `crates/oxidean-vcs/src/{changeset_store,read_index}.rs` — `Some(...)` wraps for the `Option` column.

## Implementation map (design → code)

- **Import** (`import_chain`/`lazy_import`): `is_skipped` → limit/`from` validation → nonce claim (`vcs:import` `active` row, TTL reclaim) → tip resolution (`from` head→tag→raw, or heads+tags peel with `skipped_tips` for non-commit targets) → cursor decode (inbound byte bound → b64url → version → tips/oid/name/`skip` bounds) → full bounded `rev_list` (`truncated` ⇒ `TooLarge` — `--max-count` can only cut newest, so a parents-first window requires the complete bounded list) → per-oid `import_one` → link + `linked:{cs}` row → nonce-verified cursor upsert → nonce-verified release.
- **`import_one`**: refusal inventory runs BEFORE any write (parents ≤64, message ≤65 535, valid timestamps, UTF-8, mirrored `compose_author` sanitize-then-refuse, sidecar byte bound) → `tree_flat` + `commit_name_status` (bounds = `import_tree_max` × 1024/2048) → gitlink drop+warn, mode/path/size checks → blob `put`s → `store.create` → `ident:`/`ident-cs:` sidecar upserts.
- **Export** (`export`): `is_skipped` → branch/`actor_capability`/head validation → post-order DFS to first linked+verifiable anchor (gc'd links count as unlinked) → `export_one` per node (stored manifest folded over parents[0]'s `tree_flat`, blob writes dedup'd by VCS hash through `seen`, sidecar idents else `author`+`committed_at` fallback) → same-anchor no-op → git-only-tip `RefConflict{actual:null}` (never clobbered, even under force) → ff gate (changeset-DAG probe, git-side `is_ancestor` fallback; `force` only relaxes ff) → `ProtectionChecker.check` → exactly ONE hook-gated move (`fast_forward_ref`/`force_update_ref`; `Denied`→`Forbidden{push_hook}`, `Conflict`→re-read `actual`).
- **Error surface**: `InteropError` (no `PartialEq`), complete `map_git_err`/`map_*_err` bridges, `warned()` warn-at-construction, `to_app_error` wire bridge for u7 (existing codes only; `db.not_configured` literal for `DbNotConfigured`).
- **No new tables**: `active`/`cursor`/`ident:{oid}`/`ident-cs:{cs}`/`linked:{cs}` rows ride `vcs_derived_index` under `kind="vcs:import"`; `covering_changeset` NULL on markers (migration 0055).

## Gate results

| Gate | Result |
|------|--------|
| `cargo check -p oxidean-vcs --features server` | clean |
| `cargo check --workspace` | clean |
| `cargo fmt --check` | clean |
| `cargo clippy -p oxidean-vcs --features server` + git/db/api/core | clean |
| `cargo test -p oxidean-vcs --features server` | **147/147** (19 new `git_interop`) |
| `cargo test -p oxidean-git` | 49/49 + suite (`GIT_CONFIG_GLOBAL=/dev/null`; host `commit.gpgsign` makes one squash test environmental-fail otherwise) |
| `cargo test -p oxidean-api --lib` | 231/231 |
| `cargo test -p oxidean-db` (+`oxidean-core`) | all suites pass |

Round-trip golden test: exporting an imported tip reproduces the **exact** source commit oid (idents, tree, parents, message byte-identical through the sidecar).
