# Logical Components — u5-vcs-merge-engine

## LC5.1 — `oxidean-vcs` crate layout delta (all `#[cfg(feature = "server")]`)

```
src/lib.rs              — server block gains `pub mod merge_engine`
src/merge_engine.rs     — MergeEngine, MergeCaps, MergeOutcome, MergeError,
                          ConflictRegion, Merge3Fail, ConflictKind,
                          Classified, SideManifest (internal),
                          TreeMap (internal), MergeProvenance (internal)
                          + unit/integration tests in-file (u4 precedent)
```

Zero new files elsewhere; zero edits to u1-u4 sources (BR5.19); zero new Cargo.toml entries (TD5.1).

## LC5.2 — `MergeEngine`

- `new(db: Database, index: ReadIndex, store: ChangesetStore, objects: ObjectStore, caps: MergeCaps) -> Result<Self, MergeError>` — `caps.validate()` pins: `merge_blob_max ≤ objects.config.max_object_bytes` (`ObjectStore::config` is `pub(crate)`; else `QuotaExceeded(Object)` fires deterministically mid-pass-2 — reviewer-3 F4); `merge_blob_max < u64::MAX` (`take(max+1)` overflow); `merge_paths_max ≤ 10_000` (Q9's client-parity invariant — `create` itself has no manifest cap, so a host-tuned larger value silently emits a manifest no client could submit). Any violation → `MergeError::InvalidInput` (host misconfig surfaces through the caller-visible typed channel).
- `merge(repo_id, a: &str, b: &str, author_name: &str, author_email: &str, message: Option<String>) -> Result<MergeOutcome, MergeError>` — WF1 pipeline (SD5.2 entry: is_skipped → parse → row_probe → bounds → ancestry probes → merge_base → three `full_tree`s → `side_manifest_pair` → rename pass → two-pass classify → `manifest_diff` → input pre-check → pass-2 re-read+merge3+puts → `create{parents:[a,b], author_*, message: message.or(default "merge {b} into {a}"), committed_at:None, changes}`). Params are `&str`, NOT `ChangesetId` — the engine owns `ChangesetId::parse` so SD5.2's guard ordering + echo-discard pin are enforceable end-to-end.
- `rebase(repo_id, cs: &str, new_parent: &str, author_name: &str, author_email: &str, message: Option<String>) -> Result<MergeOutcome, MergeError>` — WF5: `old_base = parents(cs)[0]` or ∅; `new_parent == old_base` → `FastForward{cs}`; `is_ancestor(cs,new_parent)` → `InvalidInput`; same classify pipeline with source-manifest provenance; `create{parents:[new_parent]}`.
- Engine NEVER touches `vcs_refs` (BR5.12) and never writes outside `ChangesetStore::create` + `ObjectStore::put`.

## LC5.3 — `MergeCaps` + `MergeError`

- `MergeCaps { merge_blob_max: u64 = 4MiB, merge_paths_max: usize = 10_000, ancestor_probe_max: usize = 250_000, side_walk_max: usize = 25_000, rename_probe_max: usize = 10_000, merge_conflict_max: usize = 500 }` — `Default` impl; constructor-injectable for cap tests. `validate(&self, objects: &ObjectStore) -> Result<(), MergeError>` pins the three bounds in LC5.2 (`merge_blob_max ≤ max_object_bytes`, `merge_blob_max < u64::MAX`, `merge_paths_max ≤ 10_000`) → `InvalidInput` on violation.
- `MergeError` variants per entities/Q8 — 8 variants incl. `DbNotConfigured`; `Display` for `Conflict` count-only; no `PartialEq` (recorded deviation).
- `MergeOutcome { FastForward{head}, Merged{changeset_id} }` — FastForward covers merge fast-path + rebase no-op.

## LC5.4 — Internal primitives (private to merge_engine.rs)

- `row_probe(repo_id, id) -> Result<VcsChangesetRow, MergeError>` — `vcs_get_changeset`; `None` → `NotFound`. Guards only — manifest never loaded.
- `full_tree(repo_id, cs: Option<&str>) -> Result<TreeMap, MergeError>` — `None` (root-rebase ∅ base) returns an empty `TreeMap` WITHOUT calling `tree()`; `Some` runs the `index.tree(repo,cs,None,MERGE_PAGE=500,cursor)` cursor loop (`cursor` param `Option<&str>` via `.as_deref()`; returned cursor `Option<String>`), mid-walk `merge_paths_max` abort. u4's `SCAN_PAGE` stays private — `MERGE_PAGE` is u5's own const.
- `is_ancestor(repo_id, anc, desc) -> Result<bool, MergeError>` — engine-owned BFS over `vcs_changeset_parents`, `visited ≤ ancestor_probe_max`; `anc == desc` → true.
- `side_manifest_pair(repo_id, tip_a, tip_b, base) -> Result<(SideManifest,SideManifest), MergeError>` — ONE mark BFS `base ∪ ancestors(base)` ≤ `ancestor_probe_max`; per-tip walk skipping marked ≤ `side_walk_max`; per-node `vcs_list_file_changes` rows accumulated, collected `kind=="rename"` rows ≤ `rename_probe_max` across each whole side (per-node loads themselves uncapped — SR5.11).
- `compose_provenance(raw_rows, tip_tree) -> SideManifest` — transitive chain compose, cycle drop, multi-writer `old` drop, tip-target filter.
- `classify_path(b: Option<&TreeEntry>, o: Option<&TreeEntry>, t: Option<&TreeEntry>, prov: &SideManifest) -> Classified` — pick / `delete` / `type` / `mode` / blob-diverge arm (WF2 order — `type` before `mode` before byte reads); `Classified` = `Resolved(Option<TreeEntry>)` (None = clean deletion) | `Conflict(VcsConflictEntry)`. `prov` is carried in so the rename-vs-modify arm emits `delete` on `old` with `detail = "renamed to {prov[old]}"` (BR5.7/Q2).
- `is_text(bytes) -> bool` — `false` iff a `0x00` byte appears in the first 8 KiB (BR5.5/WF0.13); gates diff3 eligibility — non-regular modes (symlink/tree) use pick rules regardless.
- `read_blob_capped(hash) -> Result<Vec<u8>, MergeError>` — `objects.get` → `AsyncReadExt::take(merge_blob_max.saturating_add(1))` → `read_to_end` — `io::Error` → `Internal`; over → `TooLarge`.
- `merge3(base,o,t)` pure `Vec<u8>` compute + a private `run_merge3(base,o,t) -> JoinHandle<Result<Vec<u8>, Merge3Fail>>` wrapper owning the `spawn_blocking` call — the wrapper seam is a codegen MUST (the BR5.10 interleave-probe detector binds to this `JoinHandle` await, LC-INF); newline byte-count pre-check per pair (`(n_base + n_side + 2) > LINE_CAP` → `TooLarge`); `(lines,ends_nl)` model; caller maps `Regions` → `content` conflict; `JoinError` → `Internal`.
- `manifest_diff(first_tree, merged, prov, prov_tree) -> (Vec<FileChangeInput>, BTreeSet<String>)` — output + the emitted-rename `old` set that feeds the delete-skip; byte-first-`old` pick on multi-source. Ordering pin: the rename emission pass fully populates the `old` set BEFORE the delete arm evaluates — a delete arm running first would emit phantom `delete{old}` rows falsifying `path_touch`/`blame`.
- `local_blob_id(bytes) -> String` = `hex::encode(Sha256::digest(bytes))` — the pass-1 mint; pass-2 `PutOutcome.hash` asserted equal (`Internal` on mismatch).

## LC5.5 — Consumed surfaces (no new surface)

Facade: `vcs_get_changeset` (+`VcsChangesetRow`), `vcs_changeset_parents`, `vcs_list_file_changes` (repo-less; +`VcsFileChangeRow`). Stores: `ChangesetStore::create` (+ `CreateChangesetInput`/`FileChangeInput`/`ChangeKind`/`ChangesetError`/`canonical_changeset`), `ReadIndex::{merge_base,tree}` (+ `TreeEntry`/`IndexError`), `ObjectStore::{get,put}` (+ `ObjectStoreError`/`QuotaScope`/`PutOutcome`/`PutKind`; put stream `E = Infallible`), `Database::is_skipped`. Crate-internal: `read_index::split_lines` (`pub(crate)`), `text_diff::{diff_lines,LINE_CAP}`, `oxidean_core::vcs_types::{ChangesetId,VcsConflictEntry}`. `hex`+`sha2` already server deps (pass-1 id mint); `tokio` (`spawn_blocking`, `io-util`); `futures-util` (`stream::once`); `bytes` (`Bytes`).

## LC5.6 — Merge-table emission order (determinism pins)

Rename pass → claimed set → per-path classify in `BTreeMap` key order (shared conflict list, path-sorted at emit, cap `merge_conflict_max+1`); `merged` `BTreeMap` built alongside; pass-2 iterates the recorded merged-path set in `BTreeMap` order; `manifest_diff` output unsorted — `canonical_changeset` owns the sort. Rebase replays the same pipeline with `prov` from `vcs_list_file_changes(cs)`.

## LC5.7 — Error↔wire contract (u7 consumes)

Per WF6: `NotFound`→`vcs.not_found`; `InvalidInput`→`vcs.invalid_input`+m; `NoCommonAncestor`→`vcs.no_common_ancestor`; `TooLarge`→`vcs.too_large`; `QuotaExceeded(scope)`→`vcs.quota_exceeded`; `Conflict`→`vcs.conflict`+`VcsConflictData{conflicts,truncated}`; `DbNotConfigured`→`db.not_configured`; `Internal`→`internal` envelope. u7 owns activity rows (`vcs.merge`/`vcs.rebase`) and any ref CAS after a `Merged` outcome.

## LC5.8 — Superseded FD lines (codegen: do NOT read these)

- `functional-spec.md` WF1.7 "collection stops at `merge_conflict_max`" → collect `+1` (BR5.8/SR5.6; the extra row drives the `truncated` flag).
- `functional-design-questions.md` "validated once … no re-check" → struck; dual validation is the pin (BR5.16).
- `functional-design-questions.md` old `merge3 -> Result<_, Vec<ConflictRegion>>` shape → `Merge3Fail{TooLarge|Regions}`.
- `rules.md` BR5.22 "memchr-count" → `memchr` is not a dep; byte-count via `iter().filter` (WF4.1/SD5.6).
