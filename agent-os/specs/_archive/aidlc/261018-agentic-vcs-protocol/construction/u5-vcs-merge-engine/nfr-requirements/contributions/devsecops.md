## Contribution

**Agent:** aidlc-devsecops-agent
**Stage:** nfr-requirements
**Unit:** u5-vcs-merge-engine

### Findings

1. **[BLOCKER] "GC-eligible orphan" claim is false — aborted merges leave permanently linked, quota-charged objects.** `ObjectStore::put` unconditionally inserts `vcs_object_links(repo_id, hash)` on success (`object_store/mod.rs:303-305`); GC orphan eligibility requires `l.hash IS NULL` (`vcs.rs:550/565/581`) and `vcs_delete_object_if_unlinked` refuses when a link exists. No link-delete facade exists — a merged blob `put` before a later conflict/TooLarge/Internal abort is repo-linked, never GC-eligible, permanently counted in `vcs_repo_object_counts`. Unbounded quota-burn vector (repeat conflicting merges ≈ merge_blob_max×clean-paths per attempt). Resolution adopted: two-pass classify — pass 1 runs merge3 and records locally-computed `sha256` blob ids (content-derived, `sha2`+`hex` already server deps) with zero puts; pass 2 (conflict-free only) re-runs merge3 per merged path and `put`s. Aborts write nothing.
2. **[Major] Bounds story misses the largest unbounded traversal: `materialize` inside `tree()`.** On a missing `tree_snap`, materialize walks the first-parent chain to the nearest snapshot or root with NO cap (`read_index.rs:819-829`), paying `vcs_list_file_changes` per node — the exact cost `side_walk_max` was tightened for. A cold-index merge pays this up to 3×. Companion hole: `vcs_list_file_changes` is unpaged — per-node manifest Vecs bounded only by stored size. Fix: residual entries for cold materialize cost and uncapped stored-manifest reads.
3. **[Major] `warned()` + `MergeError::Conflict` Display is an un-pinned path-to-logs leak.** BR5.18 routes every typed return through `warned()` → `warn!(error = %e)`; `Conflict` entries carry `path` — nothing pins Display to render count-only. Pin: `Display for Conflict = "conflict: {n} paths"`; entries exit only via the typed envelope.
4. **[Minor] 250k/25k asymmetry justification is factually wrong.** u4's `collect_side_paths` pays identical `vcs_list_file_changes` per side-exclusive node under a 250k cap (`read_index.rs:1312`). Real rationale: merge is a synchronous WRITE path already paying 2×250k is_ancestor + 250k mark + 3 materializations. Also un-named: repos whose base ancestry exceeds 250k can never merge.
5. **[Minor] `manifest_diff` emits a phantom `delete{old}` beside every emitted rename.** Rename emission requires `old ∈ first_tree` ∧ `old ∉ merged` — exactly the delete arm's trigger. Literal codegen emits `rename{old→p}` + `delete{old}` in one manifest. Fix: delete arm skips olds claimed by an emitted rename row.
6. **[Minor] `DbNotConfigured` has no `MergeError` variant — folds to `Internal`.** Diverges from u4's dedicated-variant convention; add the variant or correct SR5.9 wording.
7. **[Minor] `merge3`'s signature can't carry `TooLarge` — LINE_CAP check placement self-contradictory.** WF4.1 puts the check inside merge3 whose Err is `Vec<ConflictRegion>`. Fix: widen internal return to a `Merge3Fail` enum {TooLarge, Regions(Vec)}.
8. **[Minor] `ChangesetId::parse` echoes the rejected value.** `parse` errs `"invalid changeset id: {raw}"` (`vcs_types.rs:192`) — a map_err leaks the raw id. Pin: discard the parse message; construct fixed text.
9. **[Minor] Bridge misses the `io::Error` channel.** `read_blob_capped`'s `read_to_end` is a fourth error source absent from the 23-variant enumeration — needs `io::Error → Internal`.
10. **[Minor] LINE_CAP trips after `split_lines` materializes ~4M line vecs.** A 4MiB newline-dense blob allocates ~100MB before the check. Fix: `memchr`-count `b'\n'` pre-check inside the spawn_blocking closure before split_lines.
11. [Nit] TD5.1 dep attribution: `take` is `tokio::io::AsyncReadExt::take` (`get` returns `tokio::fs::File`), not futures-util's.
12. [Nit] TD5.2 phantom type: `VcsFileChangeInput` doesn't exist (wire type is `ManifestEntry`); real consumed types are `changeset_store::{FileChangeInput, ChangeKind}`.
13. [Nit] TD5.4 signature: `tree()` cursor *param* is `Option<&str>`; the `Option<String>` is the returned cursor.
14. [Nit] SR5.11 flag conflation: `VcsConflictData.truncated` flags the >500-entry cap; a 200-char `detail` cut is unflagged on the wire.
15. [Nit] TD5.5 untestable-as-stated: `LINE_CAP` isn't a `MergeCaps` field — the test needs real >200k-line blobs (feasible ≤4MiB).
16. [Nit] Q8 dead mapping: `IndexError::PathNotFound → NotFound` is unreachable (only `file_at` emits it; engine never calls `file_at`).

### Verified correct

- Variant counts exact: IndexError 8 + ChangesetError 6 + ObjectStoreError 9 = 23.
- `ChangesetError::ObjectNotFound → Internal` right; `CursorMalformed → Internal` right (engine-minted cursors).
- `create` has no manifest-count cap — engine is sole guard.
- `put` hash = `bytes_to_hex(sha256)` — locally reproducible (`object_store/mod.rs:215-242`).
- Parent checks repo-scoped; no cross-repo traversal.
- `split_lines` pub(crate) `read_index.rs:352`; `SCAN_PAGE` private; `LINE_CAP=200_000`; `diff_lines(&[&[u8]],&[&[u8]])`.
- `VcsConflictEntry` lacks PartialEq; `VcsErrorCode::conflict` truncates at 500+flag.
- tokio features cover spawn_blocking + io-util.
