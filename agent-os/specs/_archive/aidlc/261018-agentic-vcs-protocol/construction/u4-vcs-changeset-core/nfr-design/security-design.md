# Security Design — u4-vcs-changeset-core

## SD4.1 — Trust boundaries

```
client ──U7 transport/auth──> ChangesetStore / RefRegistry / ReadIndex
  untrusted inputs: CreateChangesetInput fields, ref names, changeset ids,
                    git oids, paths, cursors, limits
stores ──facade──> oxidean-db (trusted SQL site — sentinel strings decode)
ReadIndex ──get──> ObjectStore (trusted bytes — read path only, C6 edge)
stores ──clock──> committed_at injection seam (Option<String> in input)
```

Everything caller-supplied is validated before the first DB call; DB rows are trusted shapes but never trusted *sentinels* (exact-match only); ObjectStore bytes are trusted content (they back references the tx already verified).

## SD4.2 — Validation pipeline (SR4.1 — ordered, total, before any DB call)

Phase A (field shape): `path` 1..=512, POSIX `/`, no `\0`, no control bytes (<0x20, 0x7f), no leading/trailing `/`, no `.`/`..` segments, no `//`; `old_path` present iff `kind=="rename"` (same grammar); `blob_hash` present iff `kind!="delete"` (64-hex); `mode ∈ {33188, 33261, 40960, 16384}` (corrected set — register supersedes FD's 41471).
Phase B (manifest level): `path` unique across manifest; `old_path ≠ path` per entry; `old_path` unique across manifest (AR-01 — a path sources at most one rename; disjointness from `path` NOT required).
Phase C (scalars): `author` ≤255 `Name <email>` free of `<>`, newlines, control bytes; `message` ≤65535 bytes; `parents` ≤64 deduped order-preserving; `committed_at` → normalize `%Y-%m-%dT%H:%M:%SZ` (offsets→UTC, fractions rejected, DATETIME range).
Any violation → `InvalidInput`, zero DB calls made. Ordering rationale: a mid-tx PK/constraint error must never surface as `Internal`, and MySQL `INSERT IGNORE` + `TEXT` columns truncate silently — the validator is the only guarantee the stored row equals what was hashed.

## SD4.3 — `canonical_changeset()` (SR4.5 — single implementation, U5/U6/U7 consume)

```
fn canonical_changeset(repo_id, parents[], author, message, changes[]) ->
    { manifest_json, manifest_hash, canonical_input, idem_key, id }
```

- `manifest_json`: `serde_json::to_string` of `{"changes":[…]}` — entries sorted by byte-order `path` then `old_path` (None first); fixed key order `path,kind,mode,old_path,blob` via serde struct declaration order; `skip_serializing_if` on optionals (never `null`); `mode` i64; `blob` bare 64-hex.
- `manifest_hash` = sha256(manifest_json) hex. `manifest_json` itself is hash-material only — never persisted (no column).
- `canonical_input` = `{"v":1,"repo":<id>,"parents":<array>,"author":"<canonical>","message":"<verbatim>","manifest_hash":"<hex>"}` — `repo` inclusion is the recorded deviation (global id PK + repo-scoped reads require it).
- `idem_key` = full sha256(canonical_input) hex (64 chars, commits the full digest — the 128-bit id prefix keeps a distinguishing key).
- `id` = `cs_` + ULID body: `sha256(canonical_input)[0:16]` packed MSB-first into 26 uppercase Crockford chars (`0-9 A-H J-K M-N P-T V-Z`) — hand-rolled (~20 lines), unit-tested against `ChangesetId::parse` round-trip and golden vectors.
- `committed_at` never enters the canonical input (AC2.2.3).

## SD4.4 — Create pipeline (WF1 mechanics)

```
validate (SD4.2)
→ canonical_changeset
→ ReadIndex::update_for(id, committed_at, file_changes)   # pure fn → Vec<upsert>
→ vcs_create_changeset_tx{ changeset{id, repo, parents_json, author, message,
     committed_at, manifest_hash, idem_key},
     file_changes[], object_links = every manifest blob_hash,
     index_upserts }                                       # ONE transaction
→ decode sentinel (SD4.5) → Ok(id)
```

Eager `path_touch` rows ride the same tx — no readable partial state (BR4.7). Idempotent races resolve in-tx (`insert-if-absent` + `vcs_find_changeset_by_idem`); `Ok(existing_id)` is byte-identical by construction.

## SD4.5 — Sentinel decode + error channel (SR4.2, TD4.9)

```
match facade_err.as_str():
  "vcs.not_found:parent"           → ParentNotFound
  "vcs.not_found:object"           → ObjectNotFound
  "vcs.invalid_input:parents"      → InvalidInput
  "vcs.invalid_input:file_change"  → InvalidInput
  _                                → Internal(err)     # opaque carry
```

Whole-string equality — `match`, not `contains`/`starts_with` (TD4.8 pins a non-match arm: `"vcs.not_found:parentXYZ"` → `Internal`). `ObjectNotFound` MAY probe `vcs_get_object` on the request's deduped candidate hashes post-sentinel to name the missing hash — pinned path only; bare variant is acceptable. `Database::is_skipped()` checked eagerly at every public entry → `DbNotConfigured`. `Internal` carries the raw string for host diagnostics but u7 maps variants→codes — it never self-stringifies to wire data. `From<String> => Internal` is the ONLY facade decode impl besides the sentinel match — `Stream`-style misuse carve-outs don't exist here (no streams consumed).

## SD4.6 — Ref pipeline (SR4.3/SR4.4)

Validator order: regex shape `^[A-Za-z0-9][A-Za-z0-9._/-]{0,254}$` → reject list (`..`, `//`, leading/trailing `/` or `.`, `@{`, `HEAD`, `refs/` prefix) → id-shape (`ChangesetId::parse` success OR pure 40/64-hex) — each `InvalidInput`, before any DB call.

CAS arm table (`vcs_cas_ref_update` outcome → `RefError`/result):

| expected_old | new | outcome | result |
|---|---|---|---|
| any | any | `Applied(row)` | `Ok(Ref)` |
| any | any | `Conflict{actual:Some}` | `RefConflict{actual}` |
| `None` | `None` | `Conflict{actual:None}` | `Ok(Ref{changeset_id:None})` — assert-absent noop |
| `None` | `Some` | `Conflict{actual:None}` | `RefNotFound` — create-if-absent raced a delete (PG/SQLite arm; see R14b for the MySQL `Err` divergence) |
| `Some`/`delete` arms | — | `Conflict{actual:None}` | `RefNotFound` — absent-delete/move → not_found (AC5.2.2) |

Precisely: `Conflict{actual:None}` ⇒ (`expected_old==None && new==None`) ⇒ `Ok(assert-absent noop)`; else `RefNotFound` — the post-CAS observation carries absent-or-NULL-head; NULL heads are unreachable through this facade (documented conflation, safe). `new=Some(id)` verifies target existence via `vcs_get_changeset` BEFORE the CAS → `TargetNotFound` (checked per SR4.4 — the facade enforces no FK on `vcs_refs.head`).

Resolution (log `from`, ref ops): `vcs_list_refs` pages scanned to match-or-exhaustion — no early-stop (PG locale collation isn't bytewise). Precedence: `cs_<26>` → changeset; 40/64-hex → `resolve_git_oid`; else ref scan. `delete` = administrative `vcs_delete_ref` → `false` ⇒ `RefNotFound`. `list` is a keyset passthrough; cursor `{"k":last_ref_name}` base64-wrapped like every paged read.

## SD4.7 — Cursor codec (SR4.7)

One private helper per read: `encode<T:Serialize> -> String` (base64 of compact JSON), `decode(&str) -> Result<T>` — `base64::engine::general_purpose::URL_SAFE_NO_PAD` (url-safe so cursors survive transport untouched). Malformed base64 OR malformed JSON OR wrong shape → `CursorMalformed` (→ `vcs.invalid_input` at u7). Shapes: log `{"n":int}`; keyset `{"k":last_index_key}`; diff `{"k":last_emitted_path}`; blame `{"k":shard_key,"off":intra_shard_offset}`; refs `{"k":last_ref_name}`. Decode allocates bounded (reject payloads >1KB pre-parse — cursor text is attacker-supplied length).

## SD4.8 — Bounded walks (SR4.8 — caps are constructor-injected)

`ReadIndex::new(db, objects, caps: ReadIndexCaps)`; `ReadIndexCaps { seen_max: 65536, mark_max: 250_000, diff_candidates_max: 250_000, blame_touches_max: 65_536 }` with `Default` pinned to the SR4.8 values — tests inject small values to exercise `FrontierTooLarge`. **Every read entry existence-checks changeset inputs via `vcs_get_changeset` before walking** — absent → `NotFound` (or `PathNotFound` for file_at) — never ambiguous empty, never `BaseNotFound` for a missing input (facade returns `[]` parents for absent ids — without the check a missing `a`/`b` silently walks to "no common ancestor"). Walk implementations:

- `log`: `BinaryHeap<(String, String)>` max-heap keyed `(committed_at, id)` — heap keys are raw `String`s, not `ChangesetId` (the wire type carries no `Ord`; key on row-id strings verbatim); `seen: HashSet` = pushed∪emitted∪frontier; skip-n replay (emit pops after first `n`); page `limit` clamp 1..=500; `next_cursor {"n"}` iff frontier non-empty. Depth ceiling registered (SR4.11).
- `merge_base`: sort inputs `(lo,hi)`; probe `{lo}\x01{hi}`; mark lo's ancestors FIFO-BFS (incl. lo itself) capped `mark_max`; level-BFS from `hi`; first level with marked nodes → candidates → tie-break `(committed_at, id)`; write cache row. None → `BaseNotFound` (`vcs.no_common_ancestor`).
- `diff`: candidate set = `paths` filter OR union of both sides' `path_touch`-covered paths between merge_base and each tip (full `tree()` scans when no base); visited capped `diff_candidates_max`; entries emit in **path order** (BTreeSet — the diff cursor's resume contract).
- `blame`: cache probe `{at}\x01{hash32}\x01{chunk:04}`; miss → collect path + `old_path`-chain touches capped `blame_touches_max`; order by graph position (ancestor distance via walk); successive `ObjectStore::get(hash)` → `tokio::fs::File` read to bytes, split on `b'\n'` into `&[u8]` lines (lossless for non-UTF-8) + `text_diff`; attribute lines to introducing changeset (oldest-toucher fallback); write shards ≤32KB payload. `ObjectStoreError` mapping: `NotFound` (blob vanished — unreachable via manifest-covered links, defensive) → `Internal`; `DbNotConfigured` → `DbNotConfigured`; else → `Internal`.
- `tree`/`file_at`: probe `tree_snap {cs}`; miss → first-parent fold `tree(parent[0])+delta(cs)` to snap/root; `tree_entry` chunks first, `tree_snap` LAST in a separate upsert (crash-resume); scan `{cs}\x01` inclusive (exact `path_prefix` hit must not be skipped by `>`); `path_prefix` filters on the raw path suffix of `index_key` — `tree_entry` payloads are `{mode,blob}` and carry no path (DS-04 scoping).

## SD4.9 — Derived-index key grammar (SR4.6 — authoritative, supersedes FD where noted)

```
path_touch   {sha256(path)[0:32]}\x01{committed_at}\x01{cs}          (83 ch)
path_touch   {sha256(old)[0:32]}\x01{committed_at}\x01{cs}\x01r      (85 ch — rename old-path row)
tree_snap    {cs}
tree_entry   {cs}\x01{path}                                          (≤542 ch)
merge_base   {lo}\x01{hi}
blame        {cs}\x01{sha256(path)[0:32]}\x01{chunk:04}
```

`\x01` separator (PG text forbids `\0`); `\x01r` discriminator on the rename row (AR-01 — kills the move-and-recreate LWW merge while keeping it inside the `{hash32}\x01` scan block). `path_touch` payload carries `{path, kind, changeset_id, committed_at, old_path?, blob?, mode}`; the rename old-path row's payload `path` = OLD path, `old_path` = OLD path, `kind:"rename"` (rename-chain self-link for blame). Blame shard payload carries `path` (SR4.10 — hashed-key reads filter `payload.path==arg`; tree reads filter the key suffix instead). Payload caps: blame shard ≤32KB; all kinds <64KB MySQL TEXT ceiling.

## SD4.10 — Observability (SR4.12)

`tracing::warn!` on typed-error returns (sentinel decodes, `FrontierTooLarge`, `CursorMalformed`, `TargetNotFound`/`RefConflict` conflict returns); `tracing::info!` on index-write counts (`create` → N upserts; `materialize` → entries+snap; `blame` → shards) — ids/hashes/counts only, never path/message/author payload. Mirrors `object_store/gc.rs:65-96` precedent.

## SD4.11 — Residual-risk register (SR4.11 + AR nits — authoritative here)

| # | Risk | Disposition |
|---|------|-------------|
| R1 | merge_base may pick a dominated ancestor on multi-branch topologies | Accepted; deterministic minimal-BFS-distance, contract-adequate; u5 consumes knowingly |
| R2 | Skip-replay log cursor O(n+limit) reads/page → O(pages²·limit) full page-out | Accepted; PK probes cheap, deep paging rare |
| R3 | `lo` seed membership — merge_base marks lo AND ancestors | Correct per AC4.2.2 (linear/ff returns lo) |
| R4 | tz-offset loss on import — committed_at normalizes to UTC | u6 handoff note (AC7.4.1 foreclosed at schema) |
| R5 | Ref-resolution scan O(#refs) per call — the only residual with neither cap nor amortization | Accepted; u7 rate-limits; no early-stop possible |
| R6 | Blame O(#touches) memory + object opens inside the 65,536 cap | Accepted; cap is the tripwire |
| R7 | Log page-out depth ceiling — `seen` cap forecloses >~32-64k visited nodes mid-cursor-chain | Accepted; u7 documents the ceiling |
| R8 | 128-bit id-prefix collision reselects other content's row | Cryptographically negligible; idem_key's 256-bit digest distinguishes |
| R9 | 128-bit path-hash collision merges `path_touch`/`blame` keys (AR-07) | Same accepted class; `payload.path==arg` drops loser's history rather than corrupting |
| R10 | Blame/diff read ObjectStore bytes | Read-path only (C6 edge); write path never sees bytes |
| R11 | Cold `tree()`/`file_at()` fold O(history) on snap-less repos | Accepted; converges after first snapshot |
| R12 | GC-safety invariant: index payloads must reference only manifest-covered blob hashes | Enforced by construction today (BR4.6 links + RESTRICT + immutability); flagged for future kinds |
| R13 | `utf8mb4_bin` behavior unverified in CI until `dialect_vcs` lands | Migrate-only CI coverage recorded (TD4.8); local/docker MySQL run proves it |
| R14 | PG/SQLite `actual` observed post-CAS by separate SELECT — racing flip possible | Accepted; returned value is correct post-state (MySQL in-tx atomic) |
| R15 | `Conflict{actual:None}` conflates absent-row with NULL-head | Unreachable through facade (NULL heads never written); documented |
| R16 | `text_diff` quadratic guard — inputs beyond the line cap return a whole-range replace hunk | Accepted; deterministic fallback keeps blame/diff correct-but-coarse |
| R17 | MySQL 8.0 strictness — `INSERT IGNORE` downgrade of bad DATETIME | Prevented by SR4.1 validation (normalized UTC second-precision; fractions rejected before the tx) |
| R18 | **MySQL CAS create-race divergence**: `expected_old=None,new=Some` racing a delete returns raw `Err("vcs cas_ref create: …")` → `Internal`, NOT `Conflict{actual:None}` → `RefNotFound` (PG/SQLite behavior; vcs.rs:1874-1890) | Accepted — rare + retry-safe either way; the `vcs cas_ref create:` prefix is deliberately NOT decoded (a fifth literal would contradict SR4.2's four-only rule); u7 clients retry |
| R19 | `VcsFileMode` wire type covers 100644/100755/120000 only — `16384` (dir marker) is an unreachable superset in the validator | Accepted; u7-facing docs must not imply clients send it |

## SD4.12 — Corrections register (supersedes stale upstream artifact lines — fingerprints bound, do not re-edit)

- rules.md BR4.4 mode set `41471` → **40960** (`0o120000`); entities.md:26 example `33177`/`41471` superseded (SR4.1).
- entities.md `path_touch` key 83-char shape → rename old-path row is **85 chars** with `\x01r` discriminator (AR-01 → SD4.9).
- WF7.1/entities "key order IS chronological" → **second-precision**; same-second touches order by changeset id (N-11 → SR4.7).
- entities.md ref-row NULL-head wording → heads are `NOT NULL`-written through the facade; `actual:None` ≡ absent (N-06 conflation documented in SD4.6).
- entities.md CHAR(29) note → sqlite column is `TEXT` (N-07; cosmetic — id is 29 chars).
- rules.md `(actual=null ⟺ absent)` parenthetical → `RefConflict.actual: String`, not Option (N-08 → SD4.6 table).
- entities.md blame payload `BlameEntry[]` → shard payload carries `path` field for the hashed-read guard (N-13 → SD4.9).
- WF5.2 "path_prefix filters payload path" → filters the **`index_key` path suffix** (DS-04; `tree_entry` payloads carry no path).
- entities.md `Ref`/wire-type conflation → domain `Ref` is a local shape; the wire DTO is `VcsRefPublic` (AR-05 → TD4.1).
