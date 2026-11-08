# Functional Spec — u4-vcs-changeset-core

Three components — `ChangesetStore`, `RefRegistry`, `ReadIndex` — inside `oxidean-vcs`'s `server` feature (plus `text_diff`, the shared line-diff module u5 reuses). All state flows through the C5 façade; no SQL in this crate; no auth/transport concerns. `ObjectStore` is consumed read-only (`get(hash)`) for blame/diff — the C6-granted u3→u4 edge.

## WF1 — `ChangesetStore::create(input) -> Result<ChangesetId, ChangesetError>`

The WAL-equivalent write path.

1. **Validate input** (BR4.4/Q2): path/kind/mode/old_path/blob_hash field rules; `author` composition + ≤255; `committed_at` normalized to `%Y-%m-%dT%H:%M:%SZ` (offsets→UTC, fractions rejected, malformed → `InvalidInput`); parents deduped order-preserving. Violation → `InvalidInput`.
2. **Canonicalize** via `canonical_changeset()` (BR4.1 byte-exact recipe): `manifest_json` → `manifest_hash`; `canonical_input` → `idem_key` (full sha256 hex) + `id` (`cs_`+ULID_UPPER[0:16]). `committed_at` = input or `Utc::now()` — never hashed.
3. **Derive index rows**: `ReadIndex::update_for(id, committed_at, &[FileChangeInput]) -> Vec<VcsDerivedIndexUpsert>` — one `path_touch` per change + one `old_path`-keyed row per rename (BR4.12); `committed_at` feeds the key grammar.
4. **Assemble tx inputs**: `VcsChangesetInsert{id, repo_id, parents_json, author, message, committed_at, manifest_hash, idem_key}`, `VcsFileChangeInsert` rows, `object_links` = **every** manifest `blob_hash` (BR4.6), `index_upserts` from step 3.
5. **`vcs_create_changeset_tx`** (single tx: idem lookup → parent check → object check → insert-if-absent → file changes → links → upserts → commit). Returns `Ok(id)`.
6. **Decode** (Q7 exact literals): `vcs.not_found:parent` → `ParentNotFound`; `vcs.not_found:object` → `ObjectNotFound`; `vcs.invalid_input:*` → `InvalidInput`; else `Internal`.
7. Idempotent races resolve inside the tx — `Ok(existing_id)` identical by construction (BR4.2). Returns the deterministic `id`.

## WF2 — `ChangesetStore::get/lineage/resolve_git_oid`

1. `get(repo_id, id)`: `cs_<26>` shape check → `InvalidInput`; `vcs_get_changeset` → miss `ChangesetNotFound`; hit → row + `vcs_list_file_changes` → `ChangesetRecord{row, manifest}`. **cs_-only** — oid inputs are u7's dispatch to `resolve_git_oid` (C6 asymmetry, pinned).
2. `lineage(repo_id, id)`: `vcs_get_changeset` → decode `parents` JSON; malformed stored JSON → `Internal`.
3. `resolve_git_oid(repo_id, oid)`: 40/64-hex shape check → `InvalidInput`; `vcs_lookup_import_link` → `Option<ChangesetId>`; never triggers lazy import (u7 seam).

## WF3 — `RefRegistry::{cas, list, delete}`

No `vcs_get_ref` exists — C5 pins `{list, cas_update, delete}`; point reads are `vcs_list_refs` scans (Q6).

1. `cas(repo_id, name, expected_old, new)`:
   a. name grammar → `InvalidInput` (BR4.9);
   b. `new = Some(id)` → `vcs_get_changeset` → miss → `TargetNotFound`;
   c. `vcs_cas_ref_update` → `Applied(row)` → `Ok(Ref)`;
   d. `Conflict{actual:Some}` → `RefConflict{actual}`;
   e. `Conflict{actual:None}` → `RefNotFound` when `new=None` or `expected_old=Some` (atomic absent-delete/move → not_found, AC5.2.2); `Ok(Ref{name,changeset_id:None})` when `expected_old=None,new=None` (assert-absent noop — BR4.10);
   f. retried landed CAS → `RefConflict` by design (non-idempotent).
2. `list(repo_id, cursor, limit)`: `vcs_list_refs` passthrough (keyset on name); fresh repo → `[]`.
3. `delete(repo_id, name)`: administrative `vcs_delete_ref` → `false` → `RefNotFound`.
4. Ref **resolution** (used by WF4 `from`): `vcs_list_refs` keyset pages scanned to **match-or-exhaustion** — no `name ≥ target` early-stop (PG `ORDER BY name` under a locale collation isn't bytewise; an early-stop can skip the row → silent `NotFound`). Refs are bounded — scan until `name == target` → head id, or exhaustion → `NotFound`. Id-vs-ref precedence: `cs_<26>` → changeset, 40/64-hex → `resolve_git_oid`, else ref scan (BR4.9 now rejects id-shaped ref names, so precedence is belt-and-suspenders).

## WF4 — `ReadIndex::log(from, limit, cursor)`

1. Resolve `from` per WF3.4 precedence → `NotFound`/`InvalidInput`; then `vcs_get_changeset` existence check → `NotFound` (needed anyway for `committed_at`).
2. Cursor: absent → `n=0`; present → base64 JSON `{"n":int}` (malformed → `CursorMalformed`→`invalid_input`).
3. Heap walk seeded at `from`: max-heap on `(committed_at, id)`; in-memory `seen` set = pushed ∪ emitted ∪ frontier (dedup never serialized — Q4); pop → emit `LogEntry` (full `ChangesetRecord` incl. per-entry `vcs_list_file_changes` manifest — wire requires it) → push unseen parents. **Discard the first `n` pops**, then emit `limit` (clamp `1..=500`) → `next_cursor = {"n": n+emitted}` iff frontier non-empty.
4. `seen`/visited bound 65536 → `FrontierTooLarge` (`vcs.too_large`).

## WF5 — `ReadIndex::tree(changeset_id, path_prefix, limit, cursor)`

1. `vcs_get_changeset` → `NotFound` first. Probe `tree_snap {cs}`; miss → `materialize(cs)`: fold `tree(parent[0]) + delta(cs)` first-parent until snap/root; write `tree_entry` rows in chunked upsert calls; `tree_snap` in a separate FINAL call (crash leaves entries-but-no-snap → retry recomputes, LWW-identical — entities invariants).
2. Scan: `index_key=None, cursor=Some("{cs}\x01")` (inclusive start — a file whose path equals `path_prefix` exactly must not be skipped by the exclusive `>` bound), read until key leaves `{cs}\x01` prefix; `path_prefix` filters payload `path`; page ≤500 + `{"k":last_index_key}` cursor.

## WF6 — `ReadIndex::diff(from, to, paths?, limit, cursor)`

1. Existence-check both ids → `NotFound`.
2. Candidate path set: `paths` filter when given; else the union of both sides' `path_touch`-covered paths reachable between `merge_base(from,to)` and each tip (via `vcs_list_file_changes` on the walk's changesets), falling back to full `tree()` scans on both sides when the base is absent; compare `{mode,blob}` per path → `DiffEntry{path,kind,old_mode?,new_mode?,old_blob?,new_blob?}`; `paths` filter before paging.
3. Patches/`totals`: when requested, `text_diff` diffs the two blobs' lines via `ObjectStore::get` (read path — BR4.17); output paging is the caller's (≤500 files, patch bounds) — entries only here.

## WF7 — `ReadIndex::path_history(path, limit, cursor)` + `blame(path, at, limit, cursor)`

1. `path_history`: scan `index_key=None, cursor=Some("{sha256(path)[0:32]}\x01")` until prefix-break, keeping rows where `payload.path == arg` (hash is the index mechanism; payload equality is the semantic guarantee) — key order IS chronological (committed_at embedded); never-touched path → `[]` (legit empty). Rename rows appear on both new- and old-path scans; the old-path-keyed row's payload `path` = the OLD path, `kind:"rename"` (Q3).
2. `blame(path, at)`: `vcs_get_changeset(at)` → `NotFound`; cache probe `{at}\x01{sha256(path)[0:32]}\x01{chunk}` scans → hit pages out; miss → collect `path_touch` rows (incl. `old_path` rename chain), order by **graph position** (ancestor distance via walk — timestamps advisory at second precision), diff successive blobs (`ObjectStore::get` + `text_diff`); attribute each line to its introducing changeset (oldest-toucher fallback); write sharded `blame` rows (≤32KB payload each) → page.
3. `at` omitted → caller resolves the ref tip first (u7); the index takes a concrete id.

## WF8 — `ReadIndex::merge_base(a, b)` + `file_at(cs, path)`

1. `merge_base`: `vcs_get_changeset` on BOTH → `NotFound` (never `NoCommonAncestor` for absent input); sort inputs to `(lo, hi)`; cache probe `{lo}\x01{hi}` → hit returns; miss → mark `lo`'s ancestors (FIFO-BFS, cap 250k → `FrontierTooLarge`), FIFO-BFS **levels** from `hi`, first level containing marked nodes → candidates → tie-break `(committed_at,id)`; write cache row; none → `BaseNotFound` (`vcs.no_common_ancestor`). Canonical orientation makes the computed answer call-order-independent and key-consistent (BR4.14); the pick is a minimal-BFS-distance common ancestor — deterministic, contract-adequate, possibly dominated (documented).
2. `file_at`: existence-check; first-parent fold per BR4.13 — delta touch → `{blob,mode}` or delete-not-found; `tree_snap` ancestor → entry; root → `PathNotFound`.

## Acceptance mapping

| AC | Covered by |
|----|-----------|
| AC2.1.1 create accepts inputs, returns id, ref-free | WF1 (BR4.3) |
| AC2.1.2 two transports distinct fields | WF1.1 blob_hash-only; content_b64 is u7 (BR4.17) |
| AC2.1.3 entry fields, no full-tree pointer | WF1.1, BR4.4 |
| AC2.1.4 empty-parent bootstrap | WF1.2, BR4.3 |
| AC2.1.5 nonexistent parent → vcs.not_found | WF1.6 (`ERR_NO_PARENT` sentinel) |
| AC2.2.1/2.2.2 idempotent resubmit incl. concurrent | BR4.1+BR4.2, WF1.7 |
| AC2.2.3 server fields not in id | BR4.1, Q2 |
| AC2.3.1 same-parent concurrency | BR4.2 + unique keys (BR4.12) |
| AC2.3.2 atomic write boundary | BR4.7, WF1.5 |
| AC2.6.1/2.6.2 modes/symlinks/rename round-trip | BR4.4 + WF2.1 |
| AC4.1.1 log ancestor walk + metadata | WF4 (LogEntry = full record) |
| AC4.1.2 paginated reads | WF4–WF7 cursors, BR4.15 clamp |
| AC4.1.3 not_found vs invalid_input | BR4.15 + entry existence checks |
| AC4.2.1 diff entries | WF6 |
| AC4.2.2 merge_base / no_common_ancestor | WF8.1 |
| AC4.3.1 path_history | WF7.1 (chronological in key) |
| AC4.3.2 blame lazy from history+diffs | WF7.2 |
| AC4.4.1 tree on demand, cacheable, paginated | WF5 |
| AC4.5.1 changeset_get record+manifest | WF2.1 |
| AC4.5.2 file_get at cs+path | WF8.2 (inline bytes: u7 joins `ObjectStore::get`) |
| AC5.1.1 CAS iff expected matches | WF3.1c–d |
| AC5.1.2 concurrent CAS one winner | facade atomic CAS |
| AC5.1.3 non-idempotent retried CAS | WF3.1f, BR4.10 |
| AC5.1.4 create never moves refs; no FF | BR4.3/BR4.8 |
| AC5.2.1 empty ref_list, no implicit main | WF3.2, BR4.11 |
| AC5.2.2 create/delete CAS + absent→not_found | WF3.1a–e |
| AC5.2.3 no silent merges | BR4.11 |
| AC7.1.2/AC7.4.2 import-id convergence + resolve | BR4.1 canonical ids, WF2.3 |
| AC3.5.2 object-link completeness | BR4.6 |

## Derived view (logical)

```
ChangesetStore ──create_tx──▶ [vcs_changesets + vcs_file_changes + vcs_object_links + vcs_derived_index(path_touch)]
     │                              ▲
     └── update_for (pure) ──▶ ReadIndex ──reads──▶ changeset/file-change/derived rows
                                   │                    ▲
                                   └── lazy kinds: tree_snap/tree_entry/merge_base/blame
                                   └── ObjectStore::get (read-only; blame/diff bytes via text_diff)
RefRegistry ──cas/list/delete──▶ vcs_refs
```

`ChangesetStore → ReadIndex` is the only component edge (one-way hook); `ReadIndex → ObjectStore` is read-only; no back-edges.
