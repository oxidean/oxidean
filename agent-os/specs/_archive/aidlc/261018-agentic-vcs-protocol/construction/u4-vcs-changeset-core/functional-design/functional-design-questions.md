# Functional Design Questions — u4-vcs-changeset-core

Unit: `u4-vcs-changeset-core` — ChangesetStore, RefRegistry, ReadIndex inside `oxidean-vcs`'s `server` feature, consuming the U2 facade (`crates/oxidean-db/src/vcs.rs`) only.

## Q1 — Changeset identity

**Decision:** deterministic, content-derived. `id = "cs_" + ULID_UPPER(sha256(canonical_input)[0:16])` — the 16-byte digest prefix is packed as a **canonical uppercase ULID body** (128 bits, MSB-first, standard Crockford packing → 26 chars, `0-9 A-H J-K M-N P-T V-Z`). This satisfies `is_changeset_id` (vcs_types.rs — uppercase-only alphabet) so every wire DTO decodes it.

Rationale vs a random ULID: content-addressing makes concurrent identical creates collide to **one** row (AC2.2.2), makes resubmit return the same id (AC2.2.1), and makes U6 git-import conversions converge to identical ids for identical content (AC7.4.2). Cost: id order is meaningless — every read path orders by graph/`committed_at`, never by id.

**Byte-exact canonical recipe** (single function `canonical_changeset()` in `changeset_store`; U5/U6/U7 MUST call it — prose re-implementation is a defect):

- `manifest_json` = `serde_json::to_string` (compact, no spaces) of `{"changes":[{...}]}`; changes sorted by `path` (byte order), then `old_path` (None first); per-entry object in fixed key order `{"path":…,"kind":…,"mode":…,"old_path":…,"blob":…}` with **absent** optional fields OMITTED (never `null`); `kind` ∈ `"add"|"modify"|"delete"|"rename"`; `mode` = i64 octal value (e.g. `33188`, not `"100644"`); `blob` = bare 64-hex (no `sha256:` prefix — wire-only).
- `manifest_hash` = `sha256(manifest_json)` hex.
- `parents_json` = compact JSON array of parent ids in given order, deduped preserving first occurrence.
- `canonical_input` = `serde_json::to_string` of `{"v":1,"repo":<repo_id>,"parents":<parents_json array>,"author":"<canonical author>","message":"<verbatim>","manifest_hash":"<hex>"}`.
- `idem_key` = `sha256(canonical_input)` full 64-hex (NOT the 16-byte prefix — see Q9).
- **Deviation recorded:** `repo` is included beyond FR2.2's "exactly {parents,author,message,manifest}" — REQUIRED because `vcs_changesets.id` is a global PK while reads are repo-scoped: two repos creating identical content must not collide into one row the second repo can never read.

## Q2 — Author + timestamps

- `author` stored and hashed verbatim as the canonical `"Name <email>"` string (FR2.2, AC7.4.2). Composition: `{name} <{email}>`; name/email must not contain `<`, `>`, `\n`, `\r`, or control bytes (<0x20, 0x7f); name trimmed; author total ≤255 chars (MySQL `author VARCHAR(255)`); violations → `InvalidInput`.
- `message` ≤ 65535 bytes and `parents` ≤ 64 entries — both MySQL `TEXT` columns written under `INSERT IGNORE` **silently truncate** over-limit values (warning swallowed) → stored row would diverge from the hashed input; bounds enforced → `InvalidInput` (R-04).
- `committed_at` = caller-supplied or `Utc::now()`. **Wire/store format is exactly `%Y-%m-%dT%H:%M:%SZ`** (`chrono::SecondsFormat::Secs`, UTC): MySQL `STR_TO_DATE('%Y-%m-%d %H:%i:%s')` under `INSERT IGNORE` silently truncates fractions and coerces git offset forms to zero-dates — so offsets are normalized to UTC, fractional seconds are REJECTED (`InvalidInput`), and malformed values → `InvalidInput` before the facade call. Range pinned to MySQL `DATETIME` (1000-01-01..9999-12-31) → out-of-range → `InvalidInput`, not a mid-tx `Internal`.
  - **u6 handoff note:** UTC normalization drops git tz offsets — AC7.4.1's byte-identical timestamp round-trip is unachievable at this schema; u6 owns that AC and must record it (or an upstream amendment widens the column).
- `committed_at` is NOT part of `canonical_input` (AC2.2.3) — server clock never affects the id.
- Test seam: `committed_at: Option<String>` is the clock injection point; `Utc::now()` inline is accepted production behavior.

## Q3 — ReadIndex kinds + `index_key` grammar

Separator: `'\x01'` (SOH) — NOT `\0` (Postgres TEXT rejects 0x00). Legal in all dialects; sorts below every legal path char (paths ban control bytes — BR4.4).

| kind | index_key | payload (JSON) | writes |
|------|-----------|----------------|--------|
| `path_touch` | `{sha256(path)[0:32hex]}\x01{committed_at}\x01{cs}` | `{path,kind,changeset_id,committed_at,old_path?,blob?,mode}` | one per file change (+ one keyed by `sha256(old_path)` for renames) inside create tx |
| `tree_snap` | `{cs}` | `{written_at}` marker | lazy, on materialize |
| `tree_entry` | `{cs}\x01{path}` | `{mode,blob}` | lazy, on materialize |
| `merge_base` | `{lo_cs}\x01{hi_cs}` (sorted pair) | `{base}` | lazy, on query |
| `blame` | `{cs}\x01{sha256(path)[0:32]}\x01{chunk:04}` | `[{changeset_id,author,content}…]` ≤32KB/row | lazy, on query |

Design notes:

- `path_touch` hashes the path (fixed 83-char key: 32+1+20+1+29 — immune to MySQL `VARCHAR(255)` and collation) and embeds `committed_at` so `index_key` order IS chronological within a path — the wire request has no anchor, so keyset order = answer order, globally consistent. `committed_at` in payload kills the N+1 for the read shape. Reads always filter `payload.path == arg` (the hash is the index mechanism; payload equality is the semantic guarantee). Renames emit a second row hashed on `old_path` so `path_history(old_path)` sees the rename — that row's payload `path` field carries the OLD path (the queried path), `old_path` carries the same value, and `kind:"rename"` marks the event.
- `tree_entry` keeps the raw path — tree pagination and `path_prefix` queries need path ordering. Needs `index_key` ≥ 29+1+512=542 chars → the 0054 migration widens to `VARCHAR(560)` (PK bytes (36+64+560)×4 = 2640 < 3072) AND applies `COLLATE utf8mb4_bin` to `index_key`, `vcs_refs.name`, `vcs_file_changes.path` — default `utf8mb4_*_ci` collation makes case-variant paths/refs PK-collide (silent row loss). Migration ships in **all three** dialect dirs (`migration_parity` test enforces filename parity): real ALTERs in mysql/, no-op comment files in postgres/ (bytewise equality) and sqlite/ (BINARY default).
- `blame` shards by `{cs}\x01{pathhash}\x01{chunk}` (≤32KB payload/row — single-row whole-file blame exceeds MySQL `TEXT` 64KB; hashed path keeps keys ≤68 chars).
- Key length bound: every kind ≤ 560 chars with paths at the full 512-char schema width — **no artificial path cap**.
- Call shape (facade): probes = `index_key=Some(k), cursor=None`; scans = `index_key=None, cursor=Some(start_key)`, read until `index_key` leaves the wanted prefix — `vcs_query_derived_index` has exact-equality + `index_key > cursor` only, no LIKE/prefix predicate.
- Upserts are last-writer-wins payload-replace on `(repo,kind,index_key)` — every key above is unique per logical row by construction, so concurrent same-parent siblings never collide (AC2.3.1).

## Q4 — `vcs.log` walk + cursor; `merge_base` pick rule

**Log:** max-heap frontier ordered by `(committed_at, cs_id)`. `committed_at` round-trips at SECOND precision and id tie-breaks are content hashes — same-second commits and topology-inverting orders are normal. Cursor: **opaque `{"n": emitted_count}`** (base64 JSON). Resume = re-run the heap walk from `from`, discard the first `n` pops — the heap order is total (id unique), so replay is deterministic and duplicates are impossible: dedup lives in the in-walk `seen` set (frontier ∪ emitted ∪ pushed), never serialized. Alternative (serialize frontier+seen) rejected — `seen` is O(emitted), any deep page breaches a fixed cap; re-walk trades O(n+limit) point reads/page (cheap PK probes) for a cursor that never `too_large`s. In-walk `seen` bound: nodes visited ≤ n+limit+width; cap at 65536 → `FrontierTooLarge`.

**`merge_base`:** existence-check both inputs first (`vcs_get_changeset` → `NotFound`, never `NoCommonAncestor` for absent input). **Canonical orientation** — always mark `lo`'s ancestors and BFS from `hi` (sorted key order drives the walk too, so the answer is call-order-independent and matches the `{lo}\x01{hi}` cache key). Cache probe → hit returns. Miss → paint-down: mark `lo`'s ancestors (FIFO-BFS over `vcs_changeset_parents`, mark cap 250k → `FrontierTooLarge`/`vcs.too_large`), then FIFO-BFS **levels** from `hi`; the first level containing marked nodes yields candidates → tie-break `(committed_at, id)` → write cache row; none → `NoCommonAncestor`. Honest claim: the pick is "a minimal-BFS-distance common ancestor" — deterministic and contract-adequate (single id); it may be a *dominated* ancestor when parallel branches hold deeper independent bases (documented, not claimed as the full merge-base set). BFS-by-level still beats a `(committed_at,id)` heap pop — timestamp skew can rank an ancestor above its descendant base.

## Q5 — Blame algorithm + object-byte boundary (amended)

`path_touch` keys do NOT sort chronologically by themselves — under the Q3 re-key they DO within one path (`committed_at` in key), but attribution order still comes from **graph position**: collect the path's `path_touch` rows (plus `old_path` rename-chain touches), order by ancestor distance via the log walk (topological, not timestamp-lexicographic — second-precision skew makes timestamps advisory), then diff successive blobs' lines; a line's blamer = the changeset that introduced it (fallback: oldest toucher when ancestry is ambiguous, e.g., timestamps equal + parallel branches).

**Boundary amendment (BR4.17):** "no object bytes" applies to the **create/write path** only. Blame and diff patches/`totals` read blob bytes via `ObjectStore::get(hash)` — the C6-granted `u3 → u4` edge. `text_diff` (shared module, consumed by u5's merge) wraps those reads: inputs are two blobs, output is line diff + `additions`/`deletions` totals.

## Q6 — Ref CAS semantics (no `vcs_get_ref` exists)

C5's ref surface is `{list, cas_update, delete}` — point reads use `vcs_list_refs` scans; there is no `vcs_get_ref`.

- `cas(repo, name, expected_old, new)`:
  - name rules → `InvalidInput` (BR4.9); `new=Some(id)` → `vcs_get_changeset` target check → `TargetNotFound`.
  - Facade `Conflict{actual}` mapping:
    - `actual=Some` → `RefConflict{actual}` (data.actual populated).
    - `actual=None` + `expected_old=Some` or `new=None` → the row is absent → `RefNotFound` (AC5.2.2's absent-delete/move requirement — **atomic**, no pre-read TOCTOU).
    - `actual=None` + `expected_old=None` + `new=None` → assert-absent matched an absent row → `Ok(Ref{name, changeset_id:None})`.
  - Non-idempotent retry (BR4.10): a retried landed CAS conflicts on the second attempt — by design; clients re-read then re-CAS.
- `list`: `vcs_list_refs` passthrough, keyset on name; fresh repo → `[]` (no implicit main).
- `delete`: administrative `vcs_delete_ref` → `false` → `RefNotFound`.
- Ref existence anywhere else (log `from`) = `vcs_list_refs` keyset pages **scanned to match-or-exhaustion** — NO `name ≥ target` early-stop: PG `ORDER BY name` under a locale collation is not bytewise (`"ab"` may precede `"a-b"`), so an early-stop can pass the row → deployment-dependent silent `NotFound`. Refs are small/bounded; scan until `name == target` (equal → head) or exhaustion → `NotFound`.
- Ref names matching an id shape (`is_changeset_id` or pure 40/64-hex) are REJECTED by BR4.9 — creatable-but-unresolvable refs are a hazard (git forbids all-hex names for the same reason).

## Q7 — Sentinel decode (exact-match only)

Four sanctioned literals (consts are intentionally private in the facade — the module docstring is the contract): `vcs.not_found:parent` → `ParentNotFound`; `vcs.not_found:object` → `ObjectNotFound`; `vcs.invalid_input:parents` → `InvalidInput`; `vcs.invalid_input:file_change` → `InvalidInput`. Exact equality on the whole error string — no substring/prefix matching. Every other DB error → `Internal` (opaque; never parsed for meaning).

## Q8 — Module surfaces (C6 conformance)

```rust
changeset_store: create(CreateChangesetInput) -> Result<ChangesetId, ChangesetError>
                 get(repo_id, id) -> Result<ChangesetRecord, ChangesetError>               // cs_ ids only; miss → ChangesetNotFound — u7 dispatches oid inputs to resolve_git_oid
                 lineage(repo_id, id) -> Result<Vec<ChangesetId>, ChangesetError>
                 resolve_git_oid(repo_id, oid) -> Result<Option<ChangesetId>, ChangesetError>
ref_registry:    cas(repo_id, name, expected_old, new) -> Result<Ref, RefError>
                 list(repo_id, cursor, limit) -> Result<Vec<Ref>, RefError>
                 delete(repo_id, name) -> Result<(), RefError>
read_index:      update_for(id, committed_at, &[FileChangeInput]) -> Vec<VcsDerivedIndexUpsert>  // pure pre-tx producer; committed_at feeds path_touch keys
                 log(from, limit, cursor) / tree(cs, path_prefix, limit, cursor)
                 diff(from, to, paths, limit, cursor) / path_history(path, limit, cursor)
                 blame(path, at, limit, cursor) / merge_base(a, b) / file_at(cs, path)
text_diff:       diff(old_blob, new_blob) -> {lines, additions, deletions}                  // shared with u5
```

Deps added under `server`: `dep:base64` (cursor codec), `dep:hex` (key hashes), `dep:serde` + `dep:serde_json` (canonical recipe + payloads — NOT currently in Cargo.toml under any feature); `sha2`/`chrono`/`tokio` already server deps; `ObjectStore` consumed read-only for blame/diff.

**Cursor forms (uniform):** log → `{"n":int}`; every keyset read (tree/path_history/blame/diff) → opaque base64 `{"k":"<last_index_key>"}` — emitted only when more rows exist; malformed decode → `CursorMalformed` → `vcs.invalid_input` (BR4.15).

## Q9 — Why `idem_key` ≠ the id's 16-byte prefix

`idem_key` = **full** `sha256(canonical_input)` hex — not the truncated prefix. Rationale: (a) the in-tx idem SELECT on `(repo_id, idem_key)` keys on a 256-bit value while the id commits only 128 bits — a 128-bit prefix collision (cryptographically negligible, documented) would make `reselect_existing_changeset` return the OTHER content's row as Ok; the idem column keeps the distinguishing key. (b) Forward-compat: a `v:2` recipe can share ids' shape while idem keys differ by canonical bytes. The in-tx idem lookup is redundant-but-harmless next to the PK insert race — kept per C5's pinned sequence.

## Open scope borders (NOT u4)

- `vcs.repo_enable` / `vcs.ref_update` / `vcs.git_import|export` / `vcs.changeset_create` as external RPC procedures — u7 owns wire/auth/quota; u4 supplies the stores.
- `content_b64` transport decode and blob-PUT resolution — u7 request handling; `CreateChangesetInput` takes `blob_hash` only.
- Merge changeset construction (`file list relative to first parent`) — u5; u4 supplies `merge_base`, `file_at`, `text_diff`.
- Checkout/working-copy — u8.
