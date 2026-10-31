# Security Design — u5-vcs-merge-engine

## SD5.1 — Trust boundaries

```
client ──U7 transport/auth──> MergeEngine::{merge, rebase}
  untrusted inputs: repo_id, changeset ids (a/b/cs/new_parent),
                    author_name/email, message, MergeCaps (host-tuned)
engine ──facade──> oxidean-db (trusted SQL; facade String errors opaque)
engine ──get──> ObjectStore (trusted bytes; capped reads)
engine ──put──> ObjectStore (pass-2 only — links irrevocable)
engine ──spawn_blocking──> merge3 (CPU-bound pure computation)
```

Everything caller-supplied is validated BEFORE the first write-eligible call; DB rows are trusted shapes, never trusted sentinels; stored manifests/trees carry already-validated paths/modes (their own `create` enforced the grammar). `MergeCaps` are host-configurable — `MergeCaps::validate` at construction pins `merge_blob_max ≤ max_object_bytes` (else `QuotaExceeded(Object)` fires deterministically mid-pass-2).

## SD5.2 — Entry validation pipeline (ordered, before any write)

Engine params are `&str`/`String` (NOT `ChangesetId`) — the engine owns parsing so the guard ordering and echo-discard pin below are enforceable end-to-end (u7's wire DTOs do type the ids, but the engine-side `InvalidInput` is the channel with the echo-free guarantee). Order matches WF1.1 exactly (bounds LAST):

1. `db.is_skipped()` → `DbNotConfigured` (eager — a raw facade `String` failure would pre-empt the typed variant otherwise).
2. `ChangesetId::parse` per id arg → `InvalidInput`; parse's error text EMBEDS the raw id — discarded; fixed `"changeset id: malformed"` constructed (no value echo).
3. `vcs_get_changeset` row probes → `NotFound` (manifest never loaded for guards).
4. Deterministic input bounds mirrored from `compose_author` (`changeset_store.rs:439-461`): `name.trim()` nonempty; `email` nonempty (NOT trimmed — mirror exactly); neither contains `<`, `>`, `\n`, `\r`, bytes `<0x20`, or `0x7f`; `composed := format!("{} <{}>", name.trim(), email)` then `composed.chars().count() ≤ 255` (CHARS); `message.len() ≤ 65_535` (BYTES, incl. synthesized default). Documented duplication — `create` still validates authoritatively. Placed HERE (and re-pinned at WF1.9/WF5.5) so caller-input `InvalidInput` provably cannot fire after pass-2 `put`s.

## SD5.3 — Two-pass write discipline (the unit's central security shape)

`ObjectStore::put` inserts the `vcs_object_links` row on its success path (a `QuotaExceeded(Repo)`/`Stream` abort unlinks+deletes, but a committed link is irrevocable — no unlink facade exists) and linked objects are never GC-eligible. Therefore:

```
PASS 1 (zero writes): full_tree ×3 → rename pass → per-path classify
   diverged-blob paths run merge3 → record {path → (mode, local sha256(bytes))}
   — bytes DISCARDED, no put —
→ conflicts?  → MergeError::Conflict (zero rows, zero links)
→ manifest_diff → changes.len() ≤ merge_paths_max → TooLarge
→ deterministic create-input re-check (SD5.2.4 mirror)
PASS 2 (checkpoint passed): per merged path re-read the three inputs via
   read_blob_capped (no pass-1 input caching — ≤10k×12MiB is not a cache),
   re-run merge3 (deterministic → identical bytes/id) → put(stream, repo_id)
   → assert PutOutcome.hash == pass-1 sha256 ⇒ Internal
   → raced deletion mid-read maps ObjectStoreError::NotFound → Internal
→ store.create (FR7.4)
```

Deterministic-abort completeness (verified by reviewer pass-3 sweep): nothing `manifest_diff` emits is `create`-rejectable (paths/modes pass through stored-validated entries; `old≠path` by construction; dup keys impossible in `BTreeMap`); `parents` 1-2 ≤64; `committed_at:None`. The residual post-checkpoint surface is infra-only: `put` quota/Io/Db, `create` tx failure, raced sentinel decodes (`ParentNotFound`/`ChangesetNotFound`→`NotFound`, `ObjectNotFound`→`Internal`) — named in SR5.11.

## SD5.4 — Bounded traversal architecture

| Walk | Bound | Over → |
|------|-------|--------|
| `is_ancestor` BFS (each direction) | `ancestor_probe_max` 250k nodes | `TooLarge` |
| `side_manifest_pair` mark phase (shared `ancestors(base)`) | `ancestor_probe_max` | `TooLarge` |
| each side-exclusive walk | `side_walk_max` 25k nodes | `TooLarge` |
| collected `kind=="rename"` rows across the whole side walk | `rename_probe_max` 10k | `TooLarge` |
| `full_tree` accumulation | `merge_paths_max` 10k entries, mid-walk abort | `TooLarge` |
| blob read | `take(merge_blob_max.saturating_add(1))` | `TooLarge` |
| merge3 line count (byte-count pre-check, combined per side pair) | `(n_base + n_side + 2) > LINE_CAP` 200k | `Merge3Fail::TooLarge` |
| merged output | `merge_blob_max` | `TooLarge` |
| emitted manifest | `merge_paths_max` | `TooLarge` |
| conflict list | `merge_conflict_max`+1 collected; wire truncates 500+flag | — |

Uncapped paths named in SR5.11: cold `tree()` materialize (u4-owned, O(first-parent history), amortized), stored-manifest row loads (bounded by stored size). `merge3` runs in `spawn_blocking` — the async executor never stalls on CPU diff.

Raced-deletion residual (named, accepted): `vcs_changeset_parents` returns `Ok(vec![])` on a node deleted mid-walk — the branch silently terminates (false ancestry answer or a surprising-but-valid ∅-base replay; converges to `ParentNotFound`→`NotFound` at `create`). SR5.11 names raced *sentinel* decodes; this silent-empty shape is the same accepted-race class.

## SD5.5 — Provenance integrity design

- One shared mark phase (`ancestors(base)`, ≤250k) backs BOTH side walks — symmetric under a dominated `merge_base` pick (SR4.11).
- `compose_provenance`: chains compose transitively (`old→p→q` ⇒ `prov[old]=q`); cycles dropped; multi-writer `old` keys dropped (ambiguous ⇒ no provenance — a false conflict is worse than a missed rename); final target must exist in the side's tip tree.
- Detection ordering: rename/rename cross-path pass FIRST (claimed targets suppress per-path re-emission), then rename-vs-delete (`old` renamed by one side, absent on the other while `∈ b_t` → `rename` conflict), then per-path classify. rename-vs-modify stays `delete` kind on `old` with `detail` naming the rename target.
- Emission (manifest_diff): `prov[old]==p` ∧ `old ∉ merged` ∧ `old ∈ first_tree` ∧ `(mode,blob)` == provenance-side tree; multi-source same-target ⇒ byte-first `old` wins; delete arm skips emitted-rename `old`s (no phantom `delete{old}` falsifying `path_touch`/`blame`).
- Rebase provenance: source changeset's OWN manifest only — never a target-side walk (phantom renames on disjoint targets).

## SD5.6 — merge3 byte-faithfulness design

- `split_lines` drops terminators/trailing-empty → caller derives `ends_nl = buf.last() == Some(&b'\n')` (empty ⇒ false).
- Byte-count `b'\n'` per side (iterator filter — `memchr` is NOT a dep) BEFORE `split_lines`: `(n_base + n_side + 2) > LINE_CAP` → `Merge3Fail::TooLarge` pre-allocation (the `+2` covers `n+1` lines per unterminated buffer — a raw `n_base+n_side` count under-bounds by up to 2 when both sides lack trailing `\n`).
- Emission: `line + b"\n"` iff `i < len-1 || ends_nl` — byte-faithful reconstruction.
- Last-line sameness compares `(line_bytes, ends_nl)` PAIRS — a trailing-newline-only divergence (incl. the no-hunk side reaching its own final line) ⇒ `content` conflict, never a silent pick.
- `Merge3Fail = TooLarge | Regions(Vec<ConflictRegion>)` — the cap is representable in the inner error; `JoinError` ⇒ `Internal`.

## SD5.7 — Error channel design

```
From                                    → MergeError
IndexError::BaseNotFound                → NoCommonAncestor
IndexError::{NotFound,PathNotFound}     → NotFound
IndexError::FrontierTooLarge            → TooLarge
IndexError::CursorMalformed             → Internal (engine-minted cursor = corruption)
IndexError::InvalidInput                → InvalidInput
IndexError::DbNotConfigured             → DbNotConfigured
IndexError::Internal                    → Internal
ChangesetError::{ParentNotFound,ChangesetNotFound} → NotFound
ChangesetError::ObjectNotFound          → Internal (integrity loss)
ChangesetError::InvalidInput            → InvalidInput
ChangesetError::DbNotConfigured         → DbNotConfigured
ChangesetError::Internal                → Internal
ObjectStoreError::QuotaExceeded(scope)  → QuotaExceeded(scope) — scope preserved
ObjectStoreError::DbNotConfigured       → DbNotConfigured
ObjectStoreError::{NotFound,InvalidHash,Stream,Config,Db,Io,Internal} → Internal
io::Error (blob read_to_end)            → Internal
facade String (vcs_get_changeset etc.)  → Internal (opaque)
JoinError                               → Internal
```

`Display for MergeError::Conflict` renders `"conflict: {n} paths"` — count only (paths ride the typed envelope to the CALLER; `warned()` logs `%e`). `MergeError` does NOT derive `PartialEq` (`VcsConflictEntry` lacks it — u1-claimed; recorded deviation, tests field-compare).

`VcsConflictEntry.detail` is the only user-content field in the conflict envelope: it carries PATH text only (never blob content), truncated at 200 chars on a char boundary (SR5.6/BR5.8). No per-detail wire flag exists — `VcsConflictData.truncated` marks entry-count truncation only; detail truncation is silent-by-design (named upstream).

## SD5.8 — Observability design

`tracing` on typed-error returns (walk-cap trips, conflict counts, `spawn_blocking` failures, assert mismatches) matching the u4 `warn!`/`info!` precedent. Logs carry ids + counts only — never path/message/author/blob payload.
