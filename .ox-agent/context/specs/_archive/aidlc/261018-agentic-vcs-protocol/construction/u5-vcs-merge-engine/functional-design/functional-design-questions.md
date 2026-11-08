# Functional Design Questions — u5-vcs-merge-engine

Unit: `u5-vcs-merge-engine` — MergeEngine inside `oxidean-vcs`'s `server` feature. Reads via the u2 facade + U4 (`ReadIndex::{merge_base,tree}` — note tree/merge_base also lazily WRITE derived-index rows inside those read APIs), `ChangesetStore::{get,create}`, `ObjectStore::{get,put}`; changeset writes ONLY through `ChangesetStore::create` (plus `ObjectStore::put` for merged blobs + the lazy index rows inside read calls). Never moves a ref (G5).

## Q1 — Merge orientation, fast-path, and result shape

**Decision:** wire `a`/`b` map to `ours`/`theirs`; merge changeset `parents = [a, b]`; `changes` = `diff(tree(a) → merged_tree)` — the changed-file list is always relative to the **first** parent (FR2.4, AC2.4.1). `MergeEngine::merge(repo_id, a, b, author_name, author_email, message) -> Result<MergeOutcome, MergeError>` where `MergeOutcome = FastForward{head} | Merged{changeset_id}` maps to `VcsMergeResponse{changeset_id, fast_forward?}` (u7's mapping).

- Existence-check via `vcs_get_changeset` row probes (`ChangesetStore::get` pulls the full manifest — overkill for a presence check; the engine uses the bare row probe for guards and `get` only where the manifest is actually needed) → `NotFound` on absent — an absent head is never `NoCommonAncestor`.
- **Own ancestor test, not the merge_base pick (devsecops finding 6).** `ReadIndex::merge_base` is documented (SR4.11) to possibly return a *dominated* ancestor on exotic topologies. The fast-path and rebase guards need TRUE ancestry ⇒ the engine runs `is_ancestor(repo, anc, desc)`: BFS from `desc` over `vcs_changeset_parents` (pub facade, `vcs.rs:2453`), membership-test `anc`, nodes visited capped at `ancestor_probe_max` (over → `TooLarge`). `anc == desc` ⇒ true.
- `is_ancestor(a,b)` → `FastForward{head: b}`; `is_ancestor(b,a)` → `FastForward{head: a}`; `a == b` subsumed (AC2.4.3 — no invented merge node).
- Else full 3-way: `ReadIndex::merge_base` supplies the base tree (a *dominated* pick is still a valid common ancestor — correctness holds, conflicts may be noisier; recorded). `BaseNotFound` → `NoCommonAncestor`; `FrontierTooLarge` → `TooLarge`.
- **Author:** wire DTOs carry none — `merge`/`rebase` take `author_name`/`author_email` params; u7 supplies session identity, u8 the CLI identity. Validation happens once inside `ChangesetStore::create` (`compose_author`) — no re-check in the engine (BR5.16).
- **Default messages (deterministic — hashed into the id):** merge `None` → `format!("merge {b} into {a}")`; rebase `None` → source changeset's message verbatim.
- **Idempotent:** `create` returns the pre-existing id on `idem_key` hit (`changeset_store.rs:547`) — resubmit/concurrent-identical merges converge (BR5.11).

## Q2 — Rename provenance via a base-bounded ancestry manifest walk (redesigned — devsecops BLOCKER)

**Problem found:** `path_touch` index keys are hashed-path-major (`sha256(path)\x01committed_at\x01cs`); the facade offers exact-key probe or whole-kind keyset scan only — **no query enumerates a side's rename rows**, and a whole-kind scan is cross-path/cross-era (meaningless bound). The per-path classify also never fires a rename check: `(b=None, o=Some(Y), t=None)` resolves at step 1 before any rename rule — silent rename/rename corruption.

**Working mechanism (no new facade surface):** `side_manifest_pair(repo, tip_a, tip_b, base)` — ONE mark phase (BFS over `vcs_changeset_parents` from `base`, covering `base ∪ ancestors(base)`, bounded by `ancestor_probe_max` = 250_000 — the ancestry-closure scale, NOT `side_walk_max`; the earlier 4_096 bound would `TooLarge` every mature repo — reviewer BLOCKER), then two side-exclusive BFS walks from each tip skipping marked nodes (each ≤ `side_walk_max` = 25_000 nodes — a DELIBERATE tightening vs u4's 250k `diff_candidates_max`: the merge path is synchronous and pays a `vcs_list_file_changes` row load per visited node where u4's diff endpoint amortizes; 25k still covers realistic branch divergence, and overflow is honest-typed `TooLarge`, never silent — reviewer nit 3). Per visited node read `vcs_list_file_changes(node)` (pub, per-changeset, NO repo_id — `vcs.rs:2475`). Deliberately REIMPLEMENTED inside `merge_engine.rs` over pub facade calls rather than promoting u4's private helper (u4's source manifest is fingerprint-bound; this unit must not edit `read_index.rs`).

- Caps: mark ≤ `ancestor_probe_max`; side-exclusive nodes per side ≤ `side_walk_max`; collected rename rows ≤ `rename_probe_max` (10_000) — over ⇒ `TooLarge`, never silent rename-blindness (BR5.17).
- **Chain composition + tip filter (reviewer 4):** raw rows are `old_path→new_path` — compose transitively (`old→p→q` ⇒ `prov[old]=q`, the tip-visible target); cycles dropped. A key surviving multiple writers (same `old` renamed in two side-exclusive branches — possible inside a side containing its own merge) is DROPPED (ambiguous ⇒ no provenance, never a false conflict). Finally: `prov[old]` counts only if its final target still exists in that side's tip tree — a rename lineage ending in deletion provides no provenance.
- **Detection = separate cross-path pass:** `old→y` on ours, `old→z` on theirs, `y != z` ⇒ one `rename` conflict on the ours-side target `y`, `detail = "targets: {y}, {z}"` truncated to 200 chars at a char boundary (two ≤512-char paths can overflow — deterministic cut). Both `{y, z}` are marked rename-claimed: the per-path classify SKIPS claimed targets entirely (reviewer 3 — otherwise theirs-also-wrote-Y double-emits on one path, violating BR5.7).
- **rename-vs-delete (reviewer minor 1):** `old ∈ prov_x` (side x renamed) AND `old ∈ b_t` AND `old ∉ other_side_tree` (the other side deleted it) ⇒ one `rename` conflict on `prov_x[old]`, `detail` names the deleting side + both paths; the target is claimed. Without this arm the pick rules silently keep the rename and drop the deleter's intent — Git types this `rename/delete`; an auto-pick can't be silently wrong (Q3). rename-vs-modify stays the bare `delete` conflict on `old` but `detail` appends the rename target (`renamed to {prov[old]}`) so the envelope names both intents.
- **Rebase source correction (reviewer 2):** rebase's rename emission uses the SOURCE changeset's OWN manifest — `vcs_list_file_changes(cs)` rename rows composed/filtered the same way — one call, no walk. (Walking the target side would mint phantom renames from unrelated history and is a wasted full-history traversal on disjoint targets.)

## Q3 — Per-path three-way classification

Union over `keys(base) ∪ keys(ours) ∪ keys(theirs)` — trees are paged (`tree()` clamps limit to `SCAN_PAGE=500`, returns cursor): the engine accumulates whole trees through a cursor loop into `BTreeMap`s, enforcing `merge_paths_max` DURING accumulation (over → `TooLarge`).

| case | result |
|------|--------|
| `o == t` (unchanged, both-added-identically, both-deleted) | take `o` |
| `o == b` | take `t` |
| `t == b` | take `o` |
| both diverged | sub-classify (first hit wins, BR5.7): |

1. `delete` — one side absent, other differs from base.
2. `type` — either side `mode == 0o120000` or `0o040000` (symlink/tree) mismatched or both-diverged. NEVER diff3'd. (Tree entries can't appear in real trees — defensive kind for a `16384` mode reaching the table.)
3. `mode` — `o.mode != t.mode` (same-blob mode clash, or mode-change vs content-change). **Deliberately over-conservative** (devsecops 11): a chmod-vs-edit pair produces a `mode` conflict where git auto-merges; pinned because an auto-pick can't be silently wrong — recorded decision, not an oversight.
4. `binary`/`content` — `o.blob != t.blob`, both regular files: read both blobs capped (`AsyncReadExt::take(merge_blob_max + 1)` on `ObjectStore::get`'s `File`; over → `TooLarge`, BR5.21 — size check BEFORE text test, never folded into the predicate), then text test → binary conflict or `merge3` → clean ⇒ put merged blob, resolved `{path, o.mode, new_blob}`; regions ⇒ `content` conflict, `detail: "regions: n"`.

**Text test (u5-NEW — corrected claim, devsecops 4):** blob is diff3-eligible iff no `0x00` byte in its first 8 KiB. `text_diff` is explicitly binary-safe — NO shared rule exists; "mergeable" (u5) deliberately diverges from "diffable" (u4) and this is recorded, not claimed as shared.

## Q4 — `merge3` mechanics (terminator-aware — reviewer 6)

- `merge3(base, ours, theirs) -> Result<Vec<u8>, Vec<ConflictRegion>>` inside `merge_engine.rs`, run under `tokio::task::spawn_blocking` (JoinError → `Internal`). Inputs are owned `Vec<u8>` — no borrowed-lifetime issue across the boundary.
- **Line model:** `split_lines` (`pub(crate)`, `read_index.rs:352`) splits on `b'\n'` and DROPS terminators + the trailing empty segment — `"a"` and `"a\n"` produce identical vectors. To keep the trailing-newline distinction the merge carries `lines: Vec<Vec<u8>>` PLUS `ends_nl: bool` per input; a line `i` had a terminator iff `i < len-1 || ends_nl`. Two sides whose only delta is trailing-newline produce different `(lines, ends_nl)` pairs — visible to the alignment, never silently dropped.
- **diff calls:** `text_diff::diff_lines(&[u8-slices]) -> Vec<LineHunk>` (`text_diff.rs:16`) — `LineHunk{old_start,old_len,new_start,new_len}` exposes base-coordinate ranges per side (verified — complement construction is viable; no fictional helpers claimed).
- **`LINE_CAP` pre-check (per CALL):** `lines(base).len()+lines(side).len() > LINE_CAP` ⇒ `TooLarge` before each `diff_lines` — its whole-range fallback is never reachable from merge (BR5.22).
- **Alignment walk:** stable region (one side changed) → emit that side's lines WITH that side's terminators; both-same → either; both-diverged → `ConflictRegion`. A region touching the blob's last line also compares `ends_nl` — trailing-newline-only divergence inside an unstable region ⇒ `content` conflict, not a silent pick.
- **Serialization (deterministic, BR5.4):** each emitted line writes `line + b"\n"` when its terminator flag is set, else raw `line` — byte-faithful reconstruction, two implementers can't diverge.
- Output cap: `merged.len() > merge_blob_max` → `TooLarge` checked by the async caller after Join (before `put`).

## Q5 — Conflict surface

- `MergeError::Conflict(Vec<VcsConflictEntry>)` — `VcsConflictEntry` reused from oxidean-core (server feature already depends on it). Entries sorted by `path` byte order; `detail` ≤ 200 chars, no content echo; engine stops collecting at `merge_conflict_max` (500) + 1 — `VcsErrorCode::conflict` truncates + flags.
- **`PartialEq` deviation (devsecops 13):** `VcsConflictEntry` lacks `PartialEq` ⇒ `MergeError` does NOT derive `PartialEq` (departs from u4's error-enum convention). Tests field-compare. Adding the derive upstream would edit `vcs_types.rs` (u1-claimed) — refused; recorded.
- Kinds: `content | binary | mode | type | rename | delete` (`ConflictKind::as_str`).

## Q6 — Rebase semantics

`rebase(repo_id, changeset_id, new_parent_id, author_*, message) -> Result<MergeOutcome, MergeError>`:

- Row-probe existence both → `NotFound`.
- `old_base = parents[0]` or ∅ (root legal — replay = apply adds; `tree(∅)` is a synthesized empty `BTreeMap`, NOT a `tree()` call — `tree()` existence-checks and ∅ has no id, devsecops 10).
- **Guards:** `new_parent == old_base` → `FastForward{changeset}` honest no-op — APPLIES to ≥2-parent merge nodes too (pinned: no-op means "already there", not "collapse the merge"; the collapse desire isn't expressible on this wire). `is_ancestor(changeset, new_parent)` (incl. equality) → `InvalidInput` — descendant/cycle target (AC2.4.3; no dedicated code, `invalid_input` is the honest kind).
- **Disjoint target: LEGAL** (devsecops 10 pin) — replay needs only `old_base`; `is_ancestor` false → proceed. Same `merge`-class replay pipeline; conflicts = same envelope (AC2.5.2).
- Merge-node rebase: replay `diff(first_parent → node)`, extra parents dropped — linear history out (BR5.13).
- Result: `parents=[new_parent]`, `changes = diff(tree(new_parent) → replayed)`, `message.or(cs.message)`. `head` = new id; identical id on the no-op (callers infer no-op from equality — `VcsRebaseResponse` has no flag; recorded).
- Manifest renames: provable through the SOURCE changeset's own manifest (`vcs_list_file_changes(cs)` — reviewer 2: NOT a target-side walk — that mints phantoms); else delete+add.

## Q7 — Tree→manifest mapper (devsecops 7 + reviewer 5/7)

`manifest_diff(first_tree, merged, prov, prov_tree) -> Vec<FileChangeInput>`:

- `p ∈ merged, ∉ first` → provable rename iff `prov[old]==p` AND `old ∉ merged` AND **`old ∈ first_tree`** (the old_path must have existed at the first parent — provenance vs a path the parent never had is a lie, reviewer 7) AND `(mode, blob)` equality vs `prov_tree[p]` (the rename came from that side's tree, not a coincidental same-path add) → `rename{path, old_path, mode, blob}`; else `add`. When multiple qualifying `old`s map to the same target `p` (chained/composed provenance), the byte-first `old` wins — BTreeMap iteration order is already byte order; only one `rename` row may claim `p` (reviewer nit 4).
- `p ∈ first, ∉ merged` → `delete{path, mode: first[p].mode}` — `validate_change` requires a `LEGAL_MODES` mode even on deletes; carry the deleted entry's mode.
- both, diverged → `modify{path, mode, blob}`.
- **`changes.len() > merge_paths_max` ⇒ `TooLarge` BEFORE `create`** (reviewer 5 — the 10k wire bound is RPC-side only; the engine must enforce it itself or it can persist manifests no client could submit).
- Output unsorted — `canonical_changeset` sorts (u4 single-sorts; no double sort).

## Q8 — Error bridge (complete enumeration — devsecops 5 + reviewer 8)

| From | To `MergeError` |
|---|---|
| `IndexError::BaseNotFound` | `NoCommonAncestor` |
| `IndexError::{NotFound,PathNotFound}` | `NotFound` |
| `IndexError::FrontierTooLarge` / `IndexError::CursorMalformed` | `TooLarge` / `Internal` — inside `full_tree` every cursor is ENGINE-minted, so malformed means index corruption, not caller input (reviewer minor 2; u4's `InvalidInput` reading only fits caller-supplied cursors) |
| `IndexError::InvalidInput` / `IndexError::DbNotConfigured` / `IndexError::Internal` | `InvalidInput` / `DbNotConfigured` (dedicated variant — u4 convention) / `Internal` |
| `io::Error` (blob `read_to_end` inside `read_blob_capped`) | `Internal` (devsecops 9 — fourth error channel beyond the 23 store/index/object variants) |
| `ChangesetError::{ParentNotFound,ChangesetNotFound}` | `NotFound` (raced head/parent delete) |
| `ChangesetError::ObjectNotFound` | `Internal` (integrity loss — a stored manifest's blob is gone) |
| `ChangesetError::{InvalidInput,Internal,*}` | same-name/`Internal` |
| `ObjectStoreError::QuotaExceeded(scope)` | **`QuotaExceeded(scope)`** — scope carried (Object vs Repo) so u7 can name it in `vcs.quota_exceeded`; NEVER folded into `TooLarge` |
| `ObjectStoreError::DbNotConfigured` | `DbNotConfigured` (u4 maps it to `IndexError::DbNotConfigured` — `read_index.rs:1770`; the earlier `Internal`-defensive claim misstated the precedent — reviewer-2 finding 4) |
| `ObjectStoreError::{NotFound,InvalidHash,Stream,Config,Db,Io,Internal}` | `Internal` (defensive) |
| facade `String` results (`vcs_get_changeset`, `vcs_changeset_parents`, `vcs_list_file_changes`) | `Internal` — untyped facade strings are opaque infra failures |

`MergeError` variants: `NotFound | InvalidInput(String) | NoCommonAncestor | TooLarge | QuotaExceeded(QuotaScope) | Conflict(Vec<VcsConflictEntry>) | DbNotConfigured | Internal(String)` — all through `warned()`; `Conflict`'s Display is count-only (devsecops 3). `IndexError::PathNotFound→NotFound` is a dead arm (only `file_at`/`compute_blame` emit it — `read_index.rs:1525`; the engine calls neither) — kept for exhaustiveness, devsecops 16/reviewer-3 F6.

## Q9 — Read/write edges (dependency discipline)

- `MergeEngine{db, index, store, objects, caps}` — facade: `vcs_get_changeset`, `vcs_changeset_parents`, `vcs_list_file_changes`, `vcs_query_derived_index` (via ReadIndex only). NO new facade methods (BR5.19); NO u4 file edits (the side-walk is reimplemented over the facade — `read_index.rs` is u4-fingerprint-bound).
- Activity rows (`vcs.merge`/`vcs.rebase`) are u7's. Ref moves: none (G5).
- `merge_paths_max` default re-pinned `10_000` — must stay ≤ the wire's `changes ≤ 10_000` manifest bound (`vcs_types.rs`) so a merge can never persist a manifest a client couldn't submit.
