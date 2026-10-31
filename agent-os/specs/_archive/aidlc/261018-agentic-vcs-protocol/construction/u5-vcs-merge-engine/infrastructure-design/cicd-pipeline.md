# CI/CD Pipeline — u5-vcs-merge-engine

Zero CI edits. `merge_engine.rs` + its in-file test module ride the existing `vcs-server` leg unchanged — U3's machinery (`--features server` archive flag, `-E 'package(oxidean-vcs)'` filter, Makefile arms, `rust-artifacts-cleanup` wiring) covers it exactly as U4's did. No migrations (U5 consumes only existing VCS tables/facades), no new crates (TD5.1 — `hex`/`sha2`/`tokio`/`futures-util`/`bytes` all already server deps), no wire surface (U7 owns it).

## Lane coverage

| Change | Lane | Gate |
|---|---|---|
| `crates/oxidean-vcs/src/merge_engine.rs` + `lib.rs` server-block line | **`vcs-server` leg** (existing — `-E 'package(oxidean-vcs)'` on `nextest-archive`) | the U5 suite (below) |
| zero Cargo.toml delta | `dead-code` | existing arms cover the file: `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings`; `cargo doc -p oxidean-vcs --no-deps --features server`; `cargo machete` (no new deps to audit) |

## U5 suite surface (all `#[cfg(all(test, feature = "server"))]`, in-file per u4 precedent)

Tests seed state through the stores themselves (`ObjectStore::put`, `ChangesetStore::create`, `RefRegistry::cas` → facade `vcs_cas_ref_update`) against the same file-backed-sqlite `Database` harness the u3/u4 suites use (`testutil::test_db()` → `(TempDir, Database)` — the `TempDir` must stay alive for the test's scope; never `:memory:`) — **no new test-harness surface, no DB services** (the `db-dialects` leg stays U4-scoped: merge correctness is dialect-independent — all dialect variation lives below the facade in `vcs.rs`/`vcs_derived_index`, already covered there).

- **Merge basics**: fast-forward BOTH directions (`is_ancestor(a,b)` → `FastForward{b}`; `is_ancestor(b,a)` → `FastForward{a}`), `a == b` equal-heads → `FastForward`, **zero new rows** on the fast path (probe `vcs_get_changeset`/tree state), clean three-way merge → `Merged{id}` with `parents:[a,b]`, deterministic idempotence (same inputs → same changeset id via content-derived canonical id), concurrent-sibling creates, message defaults (merge `None` ⇒ `merge {b} into {a}`; rebase `None` ⇒ source message verbatim — the determinism backing idempotence).
- **Conflicts — one arm each** through `MergeError::Conflict(entries)`: `content` (merge3 regions), `binary` (NUL-in-8KiB side diverging), `mode` (exec-bit divergence), `type` (file↔symlink), `delete` (delete-vs-modify), `rename` (rename/rename + rename/delete), count-order (`BTreeMap` path sort), `+1` collection + `truncated` flag at `merge_conflict_max`, `detail` ≤200-char char-boundary cut on a path-bearing entry (BR5.8).
- **Rename provenance**: chain `A→B→C` compose, multi-writer `old` ambiguity drop, cycle drop, rename-vs-modify `delete`+`detail` naming target, emitted-rename delete-skip (no phantom `delete{old}`), byte-first-`old` multi-source pick.
- **Rebase**: root-rebase ∅ base (`full_tree(None)` — no `tree()` call), no-op `new_parent == old_base` → `FastForward{cs}`, descendant/cycle target → `InvalidInput`, ancestor rollback legal, **disjoint target legal** (BR5.14), merge-node first-parent replay (extra parents dropped), `parents:[new_parent]` output.
- **merge3 byte-faithfulness**: trailing-newline-only divergence → `content` conflict (last-line `(bytes,ends_nl)` pair), `\r\n` verbatim, terminator emission `i < len-1 || ends_nl`, `(n_base+n_side+2) > LINE_CAP` → `TooLarge` (unterminated-buffer +2 bound).
- **Paging**: `full_tree` cursor loop exercised past `MERGE_PAGE=500` (>500-entry fixture asserting `.as_deref()` cursor threading — every other fixture stays <1 page).
- **Caps** (constructor-injected small `MergeCaps` — the pin for testability): `merge_blob_max` over → `TooLarge`; `merge_paths_max` on `full_tree` mid-walk + emitted manifest; `ancestor_probe_max`; `side_walk_max`; `rename_probe_max` (collected rename rows across whole side); `MergeCaps::validate` rejection trio (`merge_blob_max > max_object_bytes`, `merge_blob_max == u64::MAX`, `merge_paths_max > 10_000` → `InvalidInput`).
- **Two-pass zero-write**: conflict path asserts zero residue — `vcs_get_changeset` (no new changeset), `vcs_list_file_changes` (no rows), `vcs_get_object_link` on the pass-1 mint (no link). The deterministic-abort path (bad author/message) fires at the ENTRY mirrored pre-check — before pass-1 merge3 runs, so no mint exists to probe; it asserts the `{no changeset, no file_changes}` pair. Pass-1/pass-2 hash assert covered by construction (identical determinism) — a targeted corruption test is NOT feasible without a seam, asserted by inspection instead, named.
- **Error bridge**: representative variant per arm (`CursorMalformed`→`Internal`, `FrontierTooLarge`→`TooLarge`, `BaseNotFound`→`NoCommonAncestor`, `ParentNotFound`/`ChangesetNotFound`→`NotFound`, `ObjectNotFound`→`Internal`, `QuotaExceeded(scope)` preserved, `DbNotConfigured` via `is_skipped` database); malformed id → `InvalidInput` with fixed text (raw id absent); `MergeError::Conflict` Display renders `"conflict: {n} paths"` (no paths).
- **`spawn_blocking` interleave probe — bound to the merge3 helper, not `merge()`**: a `#[tokio::test(flavor = "current_thread")]` test calls the private `spawn_blocking`-wrapped merge3 helper directly (in-file test modules reach private items), spawning a oneshot-send probe task immediately before awaiting the helper's `JoinHandle`, then `try_recv`s after it returns — under `spawn_blocking` the probe is polled during the handle await so the send lands (pass); had the diff run inline the probe could not have been polled until after return (fail). Binding the probe at `merge()`'s level does NOT detect the violation — `merge()` yields at every facade/index/object await before merge3 runs, so the probe lands either way; and merge3's payload must exceed what a first-poll `JoinHandle` completes within (a Ready-on-first-poll handle yields a false failure). This is a real detector for the BR5.10 off-runtime pin — a bare "large merge completes" test would be a smoke, not a detector, and is NOT claimed as one.

## What U5 does NOT change

- **`vcs-server` leg / `nextest-archive` flags** — zero edits; `-E 'package(oxidean-vcs)'` picks up the suite automatically. The `NO_TESTS_RUN` sharpness pin carries (whole suite `server`-gated).
- **`db-dialects` leg** — stays `dialect_probe or dialect_vcs`; merge exercises the dialect-independent crate layer (facade SQL is dialect-neutral by construction).
- **Makefile / TESTING.md / ci.yml / coverage-rust** — zero edits; same default-features coverage caveat as U3/U4 (recorded, not a gate).
- **`rust-artifacts-cleanup`** — no new archive consumer.

## Rollback

`git revert` — one source file + one `lib.rs` line.
