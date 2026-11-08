# Security Requirements — u5-vcs-merge-engine

## SR5.1 — Bounded traversals, typed overflow (BR5.9/BR5.17)

Every walk carries a named injectable cap and overflows to `TooLarge`, never truncation: `is_ancestor` BFS + `side_manifest_pair` mark phase ≤ `ancestor_probe_max` (250_000 — ancestry-closure scale, matching u4's mark bound); each side-exclusive walk ≤ `side_walk_max` (25_000 — deliberate tightening vs u4's 250k: the merge path is a synchronous WRITE op already paying 2×250k `is_ancestor` BFSs + a 250k mark phase + up to 3 cold tree materializations; u4's read endpoint carries the same per-node `vcs_list_file_changes` cost under its own 250k — devsecops 4 corrected the amortization claim; overflow is honest-typed `TooLarge`); collected rename rows ≤ `rename_probe_max` (10_000). Cap constants live on `MergeCaps` (constructor-injectable) so tests exercise `TooLarge` without large fixtures.

## SR5.2 — Blob and manifest size bounds (BR5.9/BR5.21)

Blob fetch = `take(merge_blob_max + 1)` → over ⇒ `TooLarge` — never a `binary` verdict, never a truncated diff3. Merged output re-checked `≤ merge_blob_max` post-merge3 before `put`. `merge_paths_max` (10_000) bounds EACH accumulated tree during the paged cursor loop (mid-walk abort) AND the emitted `changes.len()` before `create` — `ChangesetStore::create` has no manifest-count cap itself (verified `changeset_store.rs:480-503`), so the engine is the sole guard; without it the engine could persist a manifest no wire client could submit. `LINE_CAP` pre-checked against combined line counts BEFORE `diff_lines` — its whole-range-hunk fallback must never be reached.

## SR5.3 — Conflict ⇒ zero persistent writes — TWO-PASS classify (BR5.3)

`ObjectStore::put` inserts `vcs_object_links` unconditionally and GC never reaps linked objects (`vcs_delete_object_if_unlinked` refuses; no link-delete facade exists — devsecops 1). An early `put` therefore leaks a permanently quota-charged object on every abort — an unbounded quota-burn vector. **Pinned shape:** pass 1 classifies all paths and runs `merge3`, recording each merged path's locally-computed `sha256` id (content-derived — same hash `put` mints) and discarding bytes; zero `put`s. `manifest_diff` + its cap check and the deterministic create-input bounds (message ≤65_535, author shape — mirrored constants, `create` validates authoritatively) also run pre-checkpoint, so no deterministic abort can fire post-`put`. Only after ALL of that resolves clean does pass 2 re-run `merge3` per merged path (deterministic → identical bytes/id) and `put`, then `create`. Pre-checkpoint aborts write zero changeset/file_changes/index rows AND zero object rows/links; the only remaining leak window is infra failure inside pass-2/`create` itself — named residual (SR5.11). u4's lazy derived-index writes inside `tree`/`merge_base` reads are index mechanics, not merge writes.

## SR5.4 — Determinism is the integrity boundary (BR5.4/BR5.11)

Merge outcome is a pure function of `{repo, parents, trees, provenance, author, message}` — same inputs ⇒ same merged tree ⇒ same content-derived id ⇒ resubmission idempotent via u4's `idem_key`. Determinism pins: `BTreeMap` path ordering everywhere; conflict list sorted by path byte order; rename/rename emitted on the ours-side target; multi-source same-target ⇒ byte-first `old` wins; default message `merge {b} into {a}` deterministic. `committed_at: None` → `Utc::now()` inside `create` and is NOT id material (u4 recipe), so the id stays pure.

## SR5.5 — Input validation + guard ordering (BR5.2/BR5.14)

Changeset ids parse via `ChangesetId::parse` (crate-side grammar, no facade call); malformed ⇒ `InvalidInput` without echoing the value. Presence via `vcs_get_changeset` row probes (manifest never loaded for guards). Merge: `is_ancestor` self-check included in both direction probes. Rebase: `new_parent == old_base` short-circuits BEFORE the descendant probe (no-op honest return, not a cycle error); `is_ancestor(cs,new_parent)` incl. equality ⇒ `InvalidInput`. Messages never echo rejected inputs.

## SR5.6 — Memory and envelope bounds

Three trees + union ≤ 30k entries worst case in memory (three × `merge_paths_max`) — bounded, synchronous, no streaming needed. Conflict list: collection stops at `merge_conflict_max + 1`; `detail` fields truncate to 200 chars at a character boundary and carry path names only — NEVER blob content. Claimed-target suppression (rename pass) exists only on conflict emission — it cannot strand a clean merge.

## SR5.7 — Compute hygiene (BR5.10/BR5.22)

`merge3` runs inside `tokio::task::spawn_blocking` on owned `Vec<u8>` — the async executor never blocks on a CPU diff; `JoinError` ⇒ `Internal`. diff3 inputs are only regular-mode, ≤cap, NUL-free blobs (mode check → `type` conflict BEFORE any byte read; size → `TooLarge` BEFORE the 8 KiB NUL probe). `split_lines`' terminator drop is compensated by explicit `(lines, ends_nl)` pairs — byte-faithful reconstruction, and `ends_nl` joins the last-line sameness test so a trailing-newline-only divergence conflicts rather than silently picking.

## SR5.8 — Provenance integrity, no phantom renames (BR5.17/BR5.23)

Rename provenance must never mint a rename the history doesn't justify: chains compose transitively to the tip-visible target; cycles dropped; a `old` key surviving multiple writers on one side is dropped (ambiguous ⇒ no provenance — a false conflict is worse than a missed rename); final target must exist in the side's tip tree. Emission requires `old ∈ first_tree` AND `old ∉ merged` AND `(mode,blob)` equality vs the provenance side's tree — else honest `delete`+`add`. Rebase provenance comes from the source changeset's OWN manifest only — never a target-side walk (phantom renames on disjoint targets).

## SR5.9 — No auth inside; fail-closed config (BR4.16 precedent)

Zero authorization — u7 owns session/PAT checks. `DbNotConfigured` is emitted by an eager `db.is_skipped()` at `merge`/`rebase` ENTRY (u4-store convention — a raw-facade `String` failure would otherwise pre-empt the typed variant, devsecops-reviewer 2). `MergeError` never stringifies internals for wire data; the Q8 bridge is exhaustive across all 23 consumed error variants (IndexError 8 / ChangesetError 6 / ObjectStoreError 9) + `io::Error` + facade `String` + `JoinError` — an unmapped variant defaults `Internal`, never panics.

## SR5.10 — Error bridge completeness + scope preservation (BR5.18/BR5.25)

`QuotaExceeded(QuotaScope)` carries u3's scope through unchanged → `vcs.quota_exceeded` (never folded into `TooLarge`). `IndexError::CursorMalformed` ⇒ `Internal` inside `full_tree` — every cursor there is engine-minted, so malformed means index corruption, not caller input. `ChangesetError::ObjectNotFound` ⇒ `Internal` (a stored manifest's blob missing = integrity loss). Facade `String` errors ⇒ `Internal` opaque. `MergeError` does NOT derive `PartialEq` (`VcsConflictEntry` lacks it — u1-claimed; tests field-compare).

## SR5.11 — Residual-risk register (owned here)

- **Dominated merge_base pick** — u4's `merge_base` may return a dominated ancestor on exotic topologies (SR4.11). Accepted: the base feeds only the 3-way classify (a dominated base widens the "both-diverged" surface toward conflicts — conservative direction, never silently wrong); the shared mark phase makes provenance symmetric under any picked base.
- **Cold `tree()` materialize is unbounded** — on a missing `tree_snap`, u4's materialize walks first-parent to the nearest snapshot or root with NO cap (`read_index.rs:819-829`), paying `vcs_list_file_changes` per node — up to 3× per merge. u4-owned (SR4.11 precedent); amortized after first read. The merge's own caps cannot bound it — named honestly rather than claimed covered.
- **Stored manifests are unbounded on read** — `vcs_list_file_changes` is unpaged; a stored changeset's manifest Vec is bounded only by what was written (u7's 10k cap is write-side; `create` itself is unbounded). Provenance reads could return >10k rows for one node; `rename_probe_max` caps COLLECTED rows, not the read. Accepted — manifests are engine/wire-written under caps; a >10k stored manifest indicates prior bypass, surfaced not truncated.
- **Base ancestry >250k can never merge** — `ancestor_probe_max` trips `TooLarge` on `is_ancestor`/mark before any merge work; u4 parity (`mark_max` 250k). Accepted tripwire, not silent.
- **Pass-2 merge3 rerun cost** — merged paths re-diff on the clean path only (≤ paths×LINE_CAP work, deterministic); the abort path pays nothing extra. Accepted for the zero-link guarantee.
- **Post-checkpoint infra aborts can leak linked objects** — a mid-pass-2 `put` failure (`QuotaExceeded`, `Io`, `Db`), a `create` tx infra failure, or a raced sentinel decode (`ParentNotFound`/`ChangesetNotFound` → `NotFound` on a repo delete between probe and tx; `ObjectNotFound` → `Internal` integrity) leaves prior pass-2 objects repo-linked and quota-charged with no reclaim path (no unlink facade exists). Bounded: only after all deterministic gates pass (`merge_blob_max ≤ max_object_bytes` is enforced at `MergeCaps::validate` so `QuotaExceeded(Object)` can't fire deterministically — reviewer-3 F4); the leaked bytes are content-valid merged output; a retried merge's `put` hits the link-membership probe and skips re-charge (`vcs_get_object_link` → early return). No fix exists under BR5.19 (no new facade surface) — GC could never distinguish these from live objects anyway (they ARE linked, correctly, as content); named honestly per devsecops 1's truthfulness bar.
- **Side-manifest walk cost** — O(side-exclusive nodes) `vcs_list_file_changes` queries per merge inside `side_walk_max`; a 25k-node side is an accepted worst case (~25k small queries, synchronous) with `TooLarge` as the tripwire.
- **Merge union memory** — three ≤10k trees materialized concurrently (~30k entries, few MB); accepted for a synchronous engine op.
- **Rename/delete conflicts block rather than auto-keep** — deliberate (Git parity): the deleter's intent is never silently dropped. Callers resolve by rebasing or accepting one side — no resumable merge state exists (synchronous-total design, BR5.3).
- **No ref motion / no merge-state table** — the engine never writes `vcs_refs`; aborted merges leave no session rows to expire or leak.
- **Truncated `detail` at 200 chars** — a 512-char path pair can cut mid-name; deterministic char-boundary cut. The `truncated` flag on `VcsConflictData` marks the 500-ENTRY cap only — per-detail cuts are unflagged on the wire (devsecops 14; accepted — paths in detail are already ≤512 each).
- **`MergeError` non-`PartialEq`** — u1-claimed `VcsConflictEntry` lacks the derive; deviating from u4's enum convention rather than editing a fingerprint-bound file. Tests field-compare.

## SR5.12 — Observability on typed-error and traversal paths

Structured `tracing` on typed-error returns (`TooLarge`/`NoCommonAncestor`/`Conflict` counts, `is_ancestor`/walk trips, `spawn_blocking` failures) matching the u4 `warn!`/`info!` precedent. Logs carry ids and counts only — never path/message/author payload (conflict paths reach the CALLER through the typed envelope; they don't ride logs). **Pinned:** `Display for MergeError::Conflict` renders `"conflict: {n} paths"` — `warned()` logs `%e`, so a listing impl is a path-to-logs leak (devsecops 3). `ChangesetId::parse` error text embeds the raw id — DISCARDED; a fixed `field: reason` `InvalidInput` is constructed (devsecops 8).
