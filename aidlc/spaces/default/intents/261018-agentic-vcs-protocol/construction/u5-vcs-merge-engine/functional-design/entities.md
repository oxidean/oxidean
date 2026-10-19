# Entities — u5-vcs-merge-engine

MergeEngine owns **no persisted entities** — no new tables, no new derived-index kinds, no new facade methods. Its persisted footprint is exactly what `ChangesetStore::create` already writes (changeset row, parents join, file_changes rows, `path_touch` index rows) plus content-addressed object blobs via `ObjectStore::put` (u3-owned). `tree`/`merge_base` reads lazily write u4-owned derived-index rows inside those APIs — not merge writes.

## Owned in-crate types (`crates/oxidean-vcs/src/merge_engine.rs`, `server` feature — NEW file only; no u4 files edited, BR5.19)

| Type | Shape | Notes |
|------|-------|-------|
| `MergeEngine` | `{db: Database, index: ReadIndex, store: ChangesetStore, objects: ObjectStore, caps: MergeCaps}` | Constructed from handles u7 already owns. |
| `MergeCaps` | `{merge_blob_max: u64 = 4MiB, merge_paths_max: usize = 10_000, ancestor_probe_max: usize = 250_000, side_walk_max: usize = 25_000, rename_probe_max: usize = 10_000, merge_conflict_max: usize = 500}` | `merge_paths_max` caps BOTH per-tree accumulation AND the emitted manifest (`changes.len()` checked before `create` — reviewer 5). `ancestor_probe_max` bounds `is_ancestor` walks AND the `side_manifest_pair` mark phase (ancestry-closure scale). `side_walk_max` bounds only the side-EXCLUSIVE delta — 25k is a deliberate tightening vs u4's 250k `diff_candidates_max` (synchronous path, per-node manifest loads; honest-typed overflow). `MergeCaps::validate` at `MergeEngine::new` pins `merge_blob_max ≤ objects.max_object_bytes` — else every merged blob in the gap deterministically trips `put`'s streamed cap mid-pass-2 (reviewer-3 F4). |
| `MergeOutcome` | `FastForward { head: ChangesetId }` \| `Merged { changeset_id: ChangesetId }` | `FastForward` covers merge fast-path AND rebase no-op; rebase callers infer no-op from identical id (`VcsRebaseResponse` has no flag — recorded). |
| `MergeError` | `NotFound` \| `InvalidInput(String)` \| `NoCommonAncestor` \| `TooLarge` \| `QuotaExceeded(QuotaScope)` \| `Conflict(Vec<VcsConflictEntry>)` \| `DbNotConfigured` \| `Internal(String)` | `Display` for `Conflict` renders count-only (`"conflict: {n} paths"` — `warned()` logs `%e`; listing entries would leak paths — devsecops 3). `DbNotConfigured` propagates u4's dedicated-variant convention. Full bridge table pinned in Q8 — `QuotaExceeded` carries u3's `QuotaScope` (Object vs Repo) so u7 names the scope in `vcs.quota_exceeded`; never folded. **No `PartialEq`** — `VcsConflictEntry` lacks it and `vcs_types.rs` is u1-claimed (recorded deviation; tests field-compare). |
| `ConflictKind` | `Content` \| `Binary` \| `Mode` \| `Type` \| `Rename` \| `Delete` → `"content"\|"binary"\|"mode"\|"type"\|"rename"\|"delete"` | Pinned taxonomy (AC2.4.1/2.4.2); open string on the wire. |
| `ConflictRegion` (internal) | `{ours_range: Range<usize>, theirs_range: Range<usize>}` | `merge3` unstable ranges; feeds `detail` only. |
| `Merge3Fail` (internal) | `TooLarge` \| `Regions(Vec<ConflictRegion>)` | `merge3`'s inner error — the cap must be representable inside `spawn_blocking` (devsecops 7); caller maps `TooLarge`→`MergeError::TooLarge`, `Regions`→`content` conflict. |
| `SideManifest` (internal) | `BTreeMap<old_path, new_path>` (composed+filtered) | Built by `side_manifest_pair` — ONE shared `ancestors(base)` mark (≤`ancestor_probe_max`) + two side-exclusive walks (≤`side_walk_max` nodes, `vcs_list_file_changes` rows ≤`rename_probe_max`); `compose_provenance` resolves chains transitively, drops cycles/ambiguous olds, keeps only tip-tree-visible targets. REIMPLEMENTED in `merge_engine.rs` (u4's `collect_side_paths` stays private; `read_index.rs` untouched). |
| `TreeMap` (internal) | `BTreeMap<String, TreeEntry>` | Whole-tree accumulation via `tree()` cursor loop; `merge_paths_max` enforced DURING accumulation. |

## Consumed entities (NOT owned — read-only or write-through)

| Entity | Owner | Use |
|--------|-------|-----|
| `vcs_changesets` rows | u2 | `vcs_get_changeset` presence probes (manifest not loaded for guards). |
| `vcs_changeset_parents` | u2 | `vcs_changeset_parents` — `is_ancestor` BFS + `side_manifest` walks. |
| `vcs_file_changes` | u2 | `vcs_list_file_changes(changeset_id)` (no repo_id — global ids) per visited node — rename provenance; single call for rebase. |
| derived index `merge_base`/`tree_entry`/`path_touch` | u4 | `merge_base` for base TREE only (pick may be dominated — never for ancestry decisions); `tree()` for the three trees. `path_touch` NOT used for provenance (key grammar can't enumerate per-side — the devsecops blocker). |
| object blobs | u3 | `get` (capped `take(merge_blob_max+1)` reads — probe+open only, zero writes), `put` (merged outputs, `stream::once(Ok(Bytes))`) — **pass-2 only**: `put` repo-links unconditionally and linked objects are never GC-eligible; pre-checkpoint aborts therefore write NOTHING (devsecops 1). |
| `split_lines` (pub(crate), read_index.rs:352) | u4 | crate-internal import for `merge3` line splitting — shared, not copied. |
| `text_diff::diff_lines` + `LINE_CAP` | u4 | merge3's alignment input; >`LINE_CAP` combined lines ⇒ `TooLarge` before calling. |
| `canonical_changeset` / `ChangesetStore::create` | u4 | single write path; idempotent `idem_key` return. |
| `VcsConflictEntry` | u1 (oxidean-core) | wire DTO reused — no local re-declaration. |

## Rejected entity candidates

- **Merge-state / conflict-session table** — rejected: merge is synchronous and total; no resumable state (FR7.4).
- **`merge_result` derived-index kind** — rejected: `merge_base` caching suffices; outcome caching needs object-GC invalidation for no gain.
- **Changeset-type/merge flag column** — rejected: `parents.len() >= 2` derives it.
- **Rename-provenance index kind** — rejected: per-side enumeration is structurally impossible in the hashed-path-major grammar; the ancestry walk is the mechanism (bounded, cache-free, honest).
