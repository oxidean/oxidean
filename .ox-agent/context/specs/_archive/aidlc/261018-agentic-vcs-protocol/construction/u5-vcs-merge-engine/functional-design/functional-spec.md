# Functional Spec — u5-vcs-merge-engine

Actor-facing workflows for `MergeEngine`. `repo_id` resolution from `(owner,name)` is u7's; the engine speaks repo ids. Facade names are exact: `vcs_get_changeset`, `vcs_changeset_parents`, `vcs_list_file_changes`, `vcs_query_derived_index` (via `ReadIndex`).

## WF0 — shared primitives

- `row_probe(repo, cs)` → `vcs_get_changeset` presence check (manifest not loaded) → `NotFound`; facade `String` errors → `Internal`.
- `is_ancestor(repo, anc, desc)` → `anc==desc` true; else BFS from `desc` over `vcs_changeset_parents` (parents arrive in stored order — `parents[0]` = first parent), nodes ≤ `ancestor_probe_max` (over → `TooLarge`); `anc ∈ visited`.
- `full_tree(repo, cs)` → cursor-loop `index.tree(repo, cs, None, MERGE_PAGE=500, cursor)` (own const — `SCAN_PAGE` is u4-private) into `BTreeMap<path, TreeEntry>`; `|map| > merge_paths_max` ⇒ abort mid-accumulation `TooLarge` (BR5.26). `full_tree(∅)` = empty `BTreeMap` — never a `tree()` call.
- `side_manifest_pair(repo, tip_a, tip_b, base)` → ONE mark phase BFS from `base` over `vcs_changeset_parents` covering `base ∪ ancestors(base)` (≤ `ancestor_probe_max` — reviewer 1 BLOCKER: the ancestry closure is repo-scale, never `side_walk_max`), then two side-exclusive BFS walks skipping marked (≤ `side_walk_max` nodes each); `vcs_list_file_changes(node)` per node; raw `kind=="rename"` rows ≤ `rename_probe_max` (over → `TooLarge`).
- `compose_provenance(raw_rows, tip_tree)` → transitive chain composition (`old→p→q` ⇒ `old→q`), cycles dropped, multi-writer `old` keys dropped (ambiguous = none), final target must exist in `tip_tree`.
- `read_blob_capped(hash)` → `objects.get(hash).take(merge_blob_max+1).read_to_end()`; `len > max` ⇒ `TooLarge` (BR5.21 ordering).
- `is_text(bytes)` → no `0x00` in first 8 KiB (BR5.5).

## WF1 — `merge(repo_id, a, b, author, message)`

1. `db.is_skipped()` → `DbNotConfigured` FIRST (eager check, u4-store convention). `ChangesetId::parse` both → `InvalidInput` — parse's error text embeds the raw id and is DISCARDED; a fixed `"changeset id: malformed"` is constructed (devsecops 8). `row_probe` both → `NotFound`. Deterministic input bounds (message ≤65_535, author shape) also enforced here — fail-fast mirrors of `create`'s checks so no caller-input `InvalidInput` can fire post-`put` (reviewer-2 blocker).
2. `is_ancestor(a,b)` → `FastForward{b}`; `is_ancestor(b,a)` → `FastForward{a}` (BR5.2).
3. `index.merge_base(repo,a,b)` → `BaseNotFound`⇒`NoCommonAncestor`, `FrontierTooLarge`⇒`TooLarge`.
4. `b_t=full_tree(base)`, `o_t=full_tree(a)`, `t_t=full_tree(b)`; `(prov_o, prov_t)=side_manifest_pair(a, b, base)` then `compose_provenance` per side vs its own tip tree.
5. **Rename pass (cross-path, BEFORE per-path classify):** for each `old` in `prov_o ∩ prov_t` with `prov_o[old] != prov_t[old]` → `rename` conflict on path `prov_o[old]`, `detail="targets: {prov_o[old]}, {prov_t[old]}"` truncated ≤200 chars at a char boundary; mark `{prov_o[old], prov_t[old]}` rename-claimed — classify skips claimed targets (BR5.7: no path emits twice). Then rename-vs-delete: for each `old ∈ b_t` where exactly one side's `prov` maps `old` and the OTHER side's tree lacks `old` → `rename` conflict on `prov_x[old]` (`detail` names the deleting side + both paths, ≤200 chars); claim the target (reviewer minor 1 — never a silent keep).
6. **Per-path classify — PASS 1 (detect-only, zero puts):** over `keys(b_t)∪keys(o_t)∪keys(t_t) minus claimed` (WF2) → resolved `TreeEntry`s + conflicts. Diverged-blob paths run `merge3`; a clean result records `{path → sha256(merged_bytes)}` computed LOCALLY (`sha2`+`hex` — content-derived, same hash `put` would mint) and the bytes are DISCARDED — no `put` runs in pass 1 (BR5.3: `put` repo-links unconditionally; linked objects are never GC-eligible, so a `put` before the conflict-free checkpoint leaks a quota-charged object on every abort — devsecops 1).
7. Any conflict → `MergeError::Conflict` — one shared list across rename/classify passes, sorted by path byte order, collection stops at `merge_conflict_max`; wire-side constructor truncates further if needed (BR5.3: zero rows AND zero object links on abort).
8. `changes = manifest_diff(o_t, merged, prov_t, t_t)`; `changes.len() > merge_paths_max` ⇒ `TooLarge` — BEFORE pass 2 (reviewer-2 blocker: a large-divergence merge deterministically oversizes and must not abort post-`put`).
9. **Deterministic create-input pre-check** — exact mirror of `compose_author` (`changeset_store.rs:439-461`): `name.trim()` NONEMPTY; `email` NONEMPTY; neither part may contain `<`, `>`, `\n`, `\r`, bytes <0x20, or 0x7f; composed `"name <email>"` must satisfy `chars().count() ≤ 255` (CHARS); `message.len() ≤ 65_535` (BYTES, incl. the synthesized default). Documented duplication — `create` still validates authoritatively — BEFORE pass 2 so caller-input `InvalidInput` can't fire post-`put`.
10. **PASS 2 (conflict-free, inputs pre-validated):** for each pass-1 merged path re-run `merge3` (deterministic → identical bytes) and `put(stream, repo_id)`; the entry's blob id takes `PutOutcome.hash` as authoritative — `assert(PutOutcome.hash == pass1_sha256) ⇒ Internal` on mismatch (nit F5 — loud, not silent).
11. `store.create(CreateChangesetInput{parents:[a,b], author_*, message.unwrap_or("merge {b} into {a}"), committed_at:None, changes})` → `Merged{changeset_id}`. Remaining post-checkpoint aborts (mid-pass-2 `put` quota/Io, `create` tx infra) can leak linked objects — named residual (SR5.11): content-valid, retry-converging via the link-membership probe.

## WF2 — per-path classify

1. `o==t`→`o`; `o==b`→`t`; `t==b`→`o`.
2. one side absent → `delete`.
3. either mode ∈ {0o120000, 0o040000} → `type`.
4. `o.mode != t.mode` → `mode` (BR5.24 conservative).
5. `o.blob != t.blob`: `read_blob_capped` both (over → `TooLarge`); `!is_text` either → `binary`; both text → `spawn_blocking(merge3)`; clean ⇒ record `{path → (o.mode, local_sha256(merged_bytes))}` — **PASS 1 NEVER `put`s** (BR5.3 two-pass: `put` links irrevocably; pass-2 on the conflict-free path re-`merge3`s + `put`s and takes `PutOutcome.hash`); regions ⇒ `content`, `detail="regions: n"`.

## WF3 — `manifest_diff(first_tree, merged, prov, prov_tree)` (BR5.23)

- `p ∈ merged, ∉ first` → provable rename iff `prov[old]==p` AND `old ∉ merged` AND `old ∈ first_tree` AND `(merged[p].mode, merged[p].blob) == (prov_tree[p].mode, prov_tree[p].blob)` ⇒ `rename{path,old_path,mode,blob}`; else `add`.
- `p ∈ first, ∉ merged` → `delete{path, mode: first[p].mode}` — SKIP any `p` already claimed as the `old_path` of an emitted `rename` row (devsecops 5 — else every emitted rename also emits a phantom `delete` that falsifies `path_touch`/`blame`).
- both, diverged → `modify{path, mode, blob}`.
- `changes.len() > merge_paths_max` ⇒ `TooLarge` before `create`.
- (Renames without provable lineage surface as delete+add — honest fallback.)

## WF4 — `merge3(base, ours, theirs)` — inside `spawn_blocking`, returns `Result<Vec<u8>, Merge3Fail>` (`Merge3Fail = TooLarge | Regions(Vec<ConflictRegion>)` — devsecops 7: the cap must be representable in the inner error)

1. Byte-count `b'\n'` in each buffer FIRST (`iter().filter(|&&b| b == b'\n').count()` — `memchr` isn't a dep and must not be added, TD5.1; `lines ≈ count+1`); `base_n + side_n > LINE_CAP` per pair ⇒ `Merge3Fail::TooLarge` — BEFORE `split_lines` materializes ~4M line vecs from a newline-dense blob (devsecops 10); then `lines = split_lines(buf)` (`Vec<Vec<u8>>` — terminators dropped) with `ends_nl` derived CALLER-side: `buf.last() == Some(&b'\n')` (empty ⇒ false — nit F7; `split_lines` returns no tuple).
2. `diff_lines(base_lines, ours_lines)`, `diff_lines(base_lines, theirs_lines)` → hunks; complement = per-side base-aligned match ranges.
3. Alignment walk: stable → emit taken side's lines **with its own terminators** (`line + b"\n"` iff terminator flag set — `i < len-1 || ends_nl`); both-same → either; both-diverged → `ConflictRegion`. A region covering the blob's last line compares `(line_bytes, ends_nl)` PAIRS for sameness — ours `x\ny` vs theirs `x\ny\n` is a both-same-line region with DIVERGENT `ends_nl` ⇒ `content` conflict, never a silent pick (reviewer 6 hole + nit 5 — ends_nl joins the last-line sameness test outright). The no-hunk side is covered: a side-slice reaching THAT side's final line carries `ends_nl` into the sameness pair even when that side produced no hunk — `base "a\nb\n"`, ours `"a\nb"`, theirs edits the last line ⇒ conflict, not a silent take-theirs (reviewer-2 nit 9).
4. `Merge3Fail::Regions` on any region; else `Ok(merged_bytes)`; `len > merge_blob_max` ⇒ `TooLarge` at the async caller. `JoinError` ⇒ `Internal`.

## WF5 — `rebase(repo_id, cs, new_parent, author, message)`

1. **Entry: `db.is_skipped()` → `DbNotConfigured` FIRST** (eager check, u4-store convention — otherwise the raw-facade `String` failure pre-empts the typed variant, reviewer-2 finding 2). Parse+`row_probe` both → `NotFound`. `old_base = parents(cs)[0]` or ∅.
2. `new_parent == old_base` → `FastForward{cs}` (BR5.14 — incl. ≥2-parent nodes).
3. `is_ancestor(cs, new_parent)` → `InvalidInput` (descendant/cycle).
4. `o_t=full_tree(new_parent)`, `t_t=full_tree(cs)`, `b_t=full_tree(old_base or ∅)`; `prov = compose_provenance(vcs_list_file_changes(cs) rename rows, t_t)` — the SOURCE changeset's own manifest (reviewer 2: not a target-side walk — phantoms + wasted traversal).
5. Same TWO-PASS classify pipeline — **same ordering discipline as WF1 (reviewer-3 blocker: the reorder must hold here too):** pass 1 zero puts → conflicts ⇒ `Conflict` (AC2.5.2 identical envelope); `changes = manifest_diff(o_t, replayed, prov, t_t)`; `changes.len() > merge_paths_max` ⇒ `TooLarge`; deterministic input bounds (caller `message` ≤65_535 bytes — `cs.message` is stored-valid; author shape — mirrored `compose_author` constants) — ALL BEFORE pass 2.
6. Pass 2 (conflict-free): re-`merge3`+`put` per merged path (`PutOutcome.hash` authoritative, asserted equal to pass-1 sha256 ⇒ `Internal` on mismatch); `create{parents:[new_parent], author_*, message: message.or(cs.message), committed_at:None, changes}` → `Merged{head}` → `VcsRebaseResponse.head`.

## WF6 — error → wire mapping (u7 consumes; unique mapping pinned)

| `MergeError` | wire |
|---|---|
| `NotFound` | `vcs.not_found` |
| `InvalidInput(m)` | `vcs.invalid_input` + m (no echoed values) |
| `NoCommonAncestor` | `vcs.no_common_ancestor` |
| `TooLarge` | `vcs.too_large` |
| `QuotaExceeded` | `vcs.quota_exceeded` |
| `Conflict(entries)` | `vcs.conflict` + `VcsConflictData{conflicts, truncated}` |
| `DbNotConfigured` | `db.not_configured` (u4-convention dedicated variant) |
| `Internal(m)` | `internal` envelope |

Bridge per Q8 (the complete enumeration — this line mirrors it exactly): `IndexError::{BaseNotFound→NoCommonAncestor, NotFound|PathNotFound→NotFound, FrontierTooLarge→TooLarge, CursorMalformed→Internal (engine-minted cursor), InvalidInput→InvalidInput, DbNotConfigured→DbNotConfigured, Internal→Internal}`; `ChangesetError::{ParentNotFound|ChangesetNotFound→NotFound, ObjectNotFound→Internal (integrity), InvalidInput→InvalidInput, DbNotConfigured→DbNotConfigured, rest→Internal}`; `ObjectStoreError::{QuotaExceeded(scope)→QuotaExceeded(scope), DbNotConfigured→DbNotConfigured, rest→Internal}`; `io::Error→Internal`; facade `String`→`Internal`; `JoinError`→`Internal`.

## WF7 — acceptance probes (feed code-generation)

- both-add-distinct → union tree, one changeset, 2 parents, manifest vs `a`.
- same-file disjoint line edits → clean merged blob (object store readable), `modify` entry.
- same-line both sides → `content` conflict + zero rows for the canonical pair.
- binary both-changed → `binary`; symlink divergence → `type`; chmod-vs-edit → `mode`; delete-vs-modify → `delete`; both-rename-same-source → `rename` (on ours-target path, detail names both).
- ancestor pair → `FastForward` + zero new rows; `a==b` same.
- rebase: descendant target → `InvalidInput`; `new_parent==old_base` → `FastForward` same id; merge-node rebase → single-parent result; root rebase onto unrelated → legal replay.
- caps: each of `merge_paths_max/merge_blob_max/side_walk_max/rename_probe_max/ancestor_probe_max/LINE_CAP` → `TooLarge`.
- idempotent resubmit → identical id, one row.
- provenance bonus: theirs-side clean rename → merge manifest carries `rename` (path_history continuity check).
