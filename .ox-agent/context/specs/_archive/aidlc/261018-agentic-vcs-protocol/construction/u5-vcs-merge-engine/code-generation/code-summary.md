# Code Summary — u5-vcs-merge-engine

## What landed

`crates/oxidean-vcs/src/merge_engine.rs` (2,743 lines incl. the in-file test module) + one `pub mod merge_engine;` line in `lib.rs` under `#[cfg(feature = "server")]`. Zero Cargo.toml/Cargo.lock/ci.yml/docs edits.

## Implementation map (design → code)

- **Entry pipeline** (`merge:402`, `rebase:491`): `is_skipped` → `parse_changeset_id` (`&str` params; fixed `"changeset id: malformed"` — parse's own echo text discarded) → `row_probe` → `check_inputs` (mirrored `compose_author`/message bounds, run at entry AND re-checked at the checkpoint).
- **Ancestry**: `is_ancestor` — engine-owned bounded BFS (`visited ≤ ancestor_probe_max`, `anc==desc`→true); `merge_base` used for base-tree selection only.
- **Trees**: `full_tree(repo, Option<&str>)` — `None` returns empty map without `tree()` (root rebase); MERGE_PAGE=500 cursor loop `.as_deref()`; mid-walk `merge_paths_max` abort.
- **Provenance**: `side_manifest_pair` — one shared mark BFS `base ∪ ancestors(base)` ≤ `ancestor_probe_max`; `side_walk` per tip skipping marked ≤ `side_walk_max`, collected `rename` rows ≤ `rename_probe_max`; `compose_provenance` — transitive chains, cycle/multi-writer drop, tip-tree filter.
- **Classification** (`classify_all`): rename/rename first → rename-vs-delete both directions (claimed targets suppress re-emission) → per-path `classify_path` (`prov_union` carried for rename-vs-modify `delete`+detail) → `push_conflict` collects `merge_conflict_max`+1 → sorted `MergeError::Conflict`.
- **Two-pass**: pass 1 `classify_all` mints `{path → (mode, sha256)}` with ZERO `put`s → `manifest_diff` → `merge_paths_max` → `check_inputs` re-check → pass 2 `put_merged` re-reads all three inputs via `read_blob_capped`, re-runs `run_merge3`, `put`, asserts `PutOutcome.hash == mint` → `Internal` on divergence → `store.create`.
- **merge3**: `(n_base + n_side + 2) > LINE_CAP` byte-count pre-check (no memchr); `ends_nl` caller-derived; boundary-map region classification; last-line `(bytes, ends_nl)` pair sameness; `Merge3Fail{TooLarge|Regions}`; `run_merge3` → `JoinHandle` (BR5.10 detector seam).
- **Error bridge**: `map_index_err`/`map_changeset_err`/`map_object_err` + `io::Error`/facade-`String`/`JoinError` arms — full SD5.7 table; `QuotaExceeded(scope)` preserved; `Conflict` Display count-only; `detail200` char-boundary cut; `MergeError` no `PartialEq`.

## Deviation (write-set expansion)

`changeset_store.rs`/`ref_registry.rs`/`read_index.rs`/`text_diff.rs` — **doc-comment-only** de-bracket of broken/private intra-doc links (11 sites). Pre-existing at HEAD; fails `RUSTDOCFLAGS="-D warnings" cargo doc` (ci.yml:338) under the pinned rust:1.99.0 toolchain — a latent CI red independent of u5. Recorded in source-manifest.json; lapses u4's CG fingerprint → u4 re-verify at checkpoint (accepted cost, same as u4's `vcs.rs` deviation).

## Gate results

| Gate | Result |
|------|--------|
| `cargo test --no-run -p oxidean-vcs --features server` | clean |
| `cargo nextest run -p oxidean-vcs --features server` | **128/128** |
| `cargo test -p oxidean-vcs --features server` | 128/128 |
| `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server` | clean — **both** nightly and pinned 1.99.0 (after the de-bracket fix) |
| `cargo nextest run -p oxidean-db` | 74/74 |
| `cargo fmt --check` | clean |

## Test suite

58 merge-engine tests (41 `#[tokio::test]` + 17 `#[test]`) + 70 inherited (U3/U4 suites still green) = 128: fast-forward both directions + equal-heads + zero-row probes, clean merge + idempotence + message defaults, every conflict kind, rename chain/multi-writer/cycle/tip-filter/emission/byte-first-old/delete-skip, rebase root/no-op/descendant-cycle/rollback/disjoint/merge-node/source-manifest-provenance, merge3 trailing-newline/CRLF/terminators/LINE_CAP+2, MERGE_PAGE paging, all cap arms incl. `MergeCaps::validate` trio, two-pass zero-write probes (conflict path link-mint + abort path no-mint), error-bridge arms, spawn_blocking interleave probe, Conflict-Display count-only, malformed-id no-echo.
