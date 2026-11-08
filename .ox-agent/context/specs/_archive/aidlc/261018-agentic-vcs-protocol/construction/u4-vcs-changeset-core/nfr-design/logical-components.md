# Logical Components — u4-vcs-changeset-core

## LC4.1 — `oxidean-vcs` crate layout (all `#[cfg(feature = "server")]`)

```
src/lib.rs              — doc header + pub mod {object_store, changeset_store, ref_registry, read_index, text_diff}
src/changeset_store.rs  — ChangesetStore, CreateChangesetInput, ChangesetRecord, ChangesetError,
                          canonical_changeset() (the single impl — U5/U6/U7 import it), sentinel decode
src/ref_registry.rs     — RefRegistry, Ref (domain shape), RefError
src/read_index.rs       — ReadIndex, ReadIndexCaps, LogEntry/DiffEntry/TreeEntry/FileEntry/PathTouch/BlameLine, IndexError
src/text_diff.rs        — pub fn diff_lines(old: &[&[u8]], new: &[&[u8]]) -> Vec<LineHunk>
                          + LineHunk {old_start, old_len, new_start, new_len}
Cargo.toml              — server feature gains dep:{serde,serde_json,base64,hex,oxidean-core} (TD4.1)
crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql (LC4.7)
crates/oxidean-db/tests/dialect_vcs.rs                                          (LC4.7)
```

`text_diff` is IO-free: it takes byte→line vectors supplied by the caller (`read_index` blame/diff feed `ObjectStore::get` bytes; u5 feeds merge bases) — the diff core never touches the object store or DB.

## LC4.2 — `ChangesetStore`

- `new(db: Database) -> Self`.
- `create(input: CreateChangesetInput) -> Result<ChangesetId, ChangesetError>` — SD4.2/SD4.4 pipeline; `input.committed_at: Option<String>` is the clock seam (None → `Utc::now()` → `SecondsFormat::Secs`-normalized).
- `get(repo_id, id) -> Result<ChangesetRecord>` — `cs_` shape check → `InvalidInput`; `vcs_get_changeset` miss → `ChangesetNotFound`; hit → row + `vcs_list_file_changes` → `ChangesetRecord`. Git OIDs are NOT accepted here — u7 dispatches to `resolve_git_oid`.
- `lineage(repo_id, id) -> Result<Vec<ChangesetId>>` — existence check → decode `parents` JSON (malformed stored JSON → `Internal`).
- `resolve_git_oid(repo_id, oid) -> Result<Option<ChangesetId>>` — 40/64-hex shape → `InvalidInput`; `vcs_lookup_import_link`; never triggers lazy import.
- `canonical_changeset()` — `pub`, deterministic; returns `{manifest_json, manifest_hash, canonical_input, idem_key, id}` (SD4.3); ULID packing is hand-rolled (no `ulid` dep) with a `ChangesetId::parse` round-trip unit test.

## LC4.3 — `RefRegistry`

- `new(db: Database) -> Self`.
- `cas(repo_id, name, expected_old: Option<ChangesetId>, new: Option<ChangesetId>) -> Result<Ref, RefError>` — SD4.6 validator order + arm table.
- `list(repo_id, cursor: Option<&str>, limit: usize) -> Result<(Vec<Ref>, Option<String>)>` — `vcs_list_refs` passthrough; `{"k":name}` cursor; limit clamp 1..=500.
- `delete(repo_id, name) -> Result<(), RefError>` — `vcs_delete_ref` bool.
- `pub(crate) fn resolve(repo_id, name) -> Result<ChangesetId, RefError>` — crate-internal: `vcs_list_refs` match-or-exhaustion pages (SD4.6); consumed by `ReadIndex::log`'s `from` resolution and future u7 entry points.

## LC4.4 — `ReadIndex`

- `new(db: Database, objects: ObjectStore, caps: ReadIndexCaps) -> Self` — takes `ObjectStore` **by value** (it is not `Clone`; a host that also serves object routes constructs a second instance — `ObjectStore::new` is idempotent — or wraps `Arc<ObjectStore>`; `get(&self)` suffices so `&ObjectStore`/deref through `Arc` works unchanged); caps injectable (SD4.8); `Default` = SR4.8 pins.
- `pub(crate) fn update_for(id, committed_at, changes: &[FileChangeInput]) -> Vec<PathTouchRow>` — **associated fn, no `&self`** (ChangesetStore holds no ReadIndex); pure. Returns an **owned** `PathTouchRow { kind, index_key, payload, covering_changeset: String }` — `VcsDerivedIndexUpsert` fields are all `&'a str` so the freshly-allocated `format!`/`serde_json` strings cannot be returned through it (E0515); `ChangesetStore::create` forms the borrowed `VcsDerivedIndexUpsert` slice at the `vcs_create_changeset_tx` call site where the owned rows outlive the call. One `path_touch` per change + `\x01r` old-path row per rename (SD4.9); consumed ONLY by `ChangesetStore::create` (single owner). `FileChangeInput` is defined in `changeset_store` and imported by `read_index` (a crate-internal module edge — read_index → changeset_store types only, never the reverse).
- `log(repo_id, from: &str, limit, cursor) -> Result<(Vec<LogEntry>, Option<String>)>` — WF4: `from` precedence resolution → existence → heap walk + skip-n cursor.
- `tree(repo_id, cs, path_prefix, limit, cursor) -> Result<(Vec<TreeEntry>, Option<String>)>` — WF5: snap probe → materialize → `{cs}\x01` inclusive scan; `path_prefix` filters the key suffix.
- `file_at(repo_id, cs, path) -> Result<FileEntry>` — WF8.2 first-parent fold.
- `diff(repo_id, from, to, paths, limit, cursor) -> Result<(Vec<DiffEntry>, Option<String>)>` — WF6 candidates; optional patch/totals via `ObjectStore::get` + `text_diff`; path-ordered emission.
- `path_history(repo_id, path, limit, cursor) -> Result<(Vec<PathTouch>, Option<String>)>` — WF7.1 hashed scan + `payload.path==arg` filter.
- `blame(repo_id, path, at, limit, cursor) -> Result<(Vec<BlameLine>, Option<String>)>` — WF7.2 cache/shard/graph-order attribution.
- `merge_base(repo_id, a, b) -> Result<ChangesetId>` — WF8.1 oriented `(lo,hi)` mark+level BFS, cached.

## LC4.5 — `text_diff`

`pub fn diff_lines(old: &[&[u8]], new: &[&[u8]]) -> Vec<LineHunk>` — Myers O(ND) (hand-rolled, no dep): backtrace over the edit graph, coalesced to `LineHunk {old_start, old_len, new_start, new_len}` (one name — totals `{additions, deletions}` are caller-derived from hunks, superseding FD Q8's diff-return sketch). Inputs are **byte-slice lines** — callers split `ObjectStore` bytes on `b'\n'` themselves (lossless; non-UTF-8 blobs flow through with no error arm — a `&[String]` signature would force a `from_utf8_lossy` loss or an extra error variant; this seam propagates to u5). Guard (R16): `old.len() + new.len() > LINE_CAP` (pin 200k) → single whole-range replace hunk — blame stays correct-but-coarse. Golden vectors in TD4.8: empty→full, full→empty, prepend/append/middle edits, rename-relevant chains (lines shifted not edited), binary content (`\0`-bearing lines diff verbatim), `\r\n` endings kept verbatim.

## LC4.6 — Error enums (closed sets, u7 maps to wire codes)

```
ChangesetError: ParentNotFound | ObjectNotFound{hash: Option<String>} | ChangesetNotFound
              | InvalidInput(String) | Internal(String) | DbNotConfigured
RefError:       RefNotFound | RefConflict{actual: String} | TargetNotFound
              | InvalidInput(String) | Internal(String) | DbNotConfigured
IndexError:     NotFound | PathNotFound | BaseNotFound | FrontierTooLarge
              | CursorMalformed | InvalidInput(String) | Internal(String) | DbNotConfigured
```

Payload shapes pinned: `InvalidInput(String)` carries the validation reason (never the rejected value); `ObjectNotFound{hash: Option<String>}` — `Some` only via the SD4.5 post-sentinel probe; `Internal(String)` carries the raw facade/io detail for host diagnostics (opaque to the wire). `BaseNotFound` is the single variant (Q4's `NoCommonAncestor` mention aliases here — N-08). `From<String> => Internal` only for the facade surface (SD4.5 — sentinel match precedes). `RefConflict.actual` is `String` (not Option — every `None` arm already mapped).

## LC4.7 — Sanctioned deltas outside the crate

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql` — mysql/ carries:
  `ALTER TABLE vcs_derived_index MODIFY COLUMN index_key VARCHAR(560) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;`
  `ALTER TABLE vcs_refs MODIFY COLUMN name VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;`
  `ALTER TABLE vcs_file_changes MODIFY COLUMN path VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL;`
  `ALTER TABLE vcs_file_changes MODIFY COLUMN old_path VARCHAR(512) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NULL;`
  (`old_path` added at infrastructure-design — keeps the rename path-pair symmetric; a case-insensitive `old_path` beside binary `path` risks ERROR 1267 on any future predicate mixing the pair)
  postgres/ + sqlite/: comment-only files (`-- no-op: bytewise equality/BINARY collation already correct`). Filename parity per `migration_parity` + FR15.5.
- `crates/oxidean-db/tests/dialect_vcs.rs` — `DATABASE_URL`-gated skip-pass (dialect_probe.rs:10-15 idiom): assert `information_schema` collation utf8mb4_bin + case-variant insert/read round-trip. Executes in CI — the infrastructure-design stage claims the `db-dialects` filter widening to `binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)` (ci.yml:810) in the same commit.
- `crates/oxidean-vcs/Cargo.toml` — `server` feature adds `dep:serde`, `dep:serde_json`, `dep:base64` (`"0.23"` api pin), `dep:hex` (`"0.4"`), `dep:oxidean-core` (optional; `client` stays dep-free).

## LC4.8 — Consumed surfaces (unchanged)

Facade (12 methods, zero additions): `vcs_create_changeset_tx`, `vcs_get_changeset`, `vcs_changeset_parents`, `vcs_find_changeset_by_idem`, `vcs_list_file_changes`, `vcs_list_refs`, `vcs_cas_ref_update`, `vcs_delete_ref`, `vcs_upsert_derived_index_rows`, `vcs_query_derived_index`, `vcs_lookup_import_link`, `vcs_get_object` (sentinel-probe only). `ObjectStore::get` — read-only (C6 u3→u4 edge). `Database::is_skipped` — eager `DbNotConfigured`. Scan idiom: probes `index_key=Some, cursor=None`; scans `index_key=None, cursor=Some(start)` terminating on prefix-break or exhaustion.

## LC4.9 — Build/test surfaces (TD4.8)

`cargo nextest run -p oxidean-vcs --features server` + `cargo clippy -p oxidean-vcs --all-targets --all-features` — both pinned in CI/Makefile by U3 (Makefile:240/408, ci.yml `vcs-server` leg). Zero-test runs fail. Test matrix per TD4.8: deterministic-id golden vectors, idempotent resubmit, `tokio::join` concurrent identical/divergent creates, missing parent/object sentinels, all CAS arms, ref-name rejections incl. id-shapes, log skip-replay no-dup, tree materialize + crash-resume, path_history chronological + rename visibility, blame attribution + rename chain, merge_base (linear→self / divergent / no-ancestor), file_at delete-not-found, cursor malformed, `FrontierTooLarge` via injected caps, `text_diff` golden vectors, every SR4.1 validation arm, sentinel non-match → `Internal`, object-links completeness, `payload.path==arg` guard, `DbNotConfigured`. No doc examples in the server module (U3 precedent).

## Dependency edges

```
ChangesetStore ──uses──▶ ReadIndex::update_for (pure, static)   # one-way
ChangesetStore ──uses──▶ canonical_changeset (module-local pub)
RefRegistry    ──uses──▶ ChangesetId::parse (oxidean-core)      # id-shape oracle
ReadIndex      ──uses──▶ RefRegistry::resolve                   # log `from` ref arm
ReadIndex      ──uses──▶ ChangesetStore::resolve_git_oid        # log `from` 40/64-hex arm
ReadIndex      ──uses──▶ changeset_store::FileChangeInput       # types-only edge
ReadIndex      ──uses──▶ ObjectStore::get + text_diff           # blame/diff bytes
all            ──uses──▶ oxidean-db facade + Database::is_skipped
```

No back-edges: `ReadIndex` never mutates changesets; `RefRegistry` never reads the index; `ChangesetStore` never reads refs. `oxidean-vcs` gains no dependency on any host crate — `oxidean-core` is a domain-types crate, `oxidean-db` provides the port (facade).
