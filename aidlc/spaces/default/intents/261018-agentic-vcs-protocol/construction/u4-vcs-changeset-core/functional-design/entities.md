# Entities — u4-vcs-changeset-core

Owned domain entities and their persistence shapes. Rows match U2's structs (`VcsChangesetRow`, `VcsFileChangeRow`, `VcsRefRow`, `VcsDerivedIndexRow` — vcs.rs:30–167); this file pins the logical view.

## ChangesetRecord (returned by `get`)

| field | type | notes |
|-------|------|-------|
| `id` | `cs_<26 upper-ULID>` (29 chars) | content-derived (Q1); global PK — `CHAR(29)` PG/SQLite, `VARCHAR(32)` MySQL |
| `repo_id` | uuid/char(36) | read scope |
| `parents` | `Vec<cs_id>` ordered | JSON array verbatim; `[]` = root |
| `author` | string `Name <email>` | canonical, ≤255, hashed verbatim |
| `message` | string | verbatim UTF-8 |
| `committed_at` | `%Y-%m-%dT%H:%M:%SZ` | second-precision UTC; NOT in id |
| `manifest_hash` | 64-hex | sha256 of manifest_json |
| `idem_key` | 64-hex \| **Option** | full canonical hash (column NULL-able; always set by u4) |
| `manifest` | `Vec<FileChange>` path-ordered | joined from `vcs_file_changes` |

## FileChange (manifest entry — `vcs_file_changes` row)

| field | type | rule |
|-------|------|------|
| `changeset_id` | cs_id | PK part |
| `path` | string ≤512, no `\0`/control bytes | PK part; POSIX separators |
| `kind` | `add`/`modify`/`delete`/`rename` | |
| `mode` | i64 octal (`33188`/`33177`/`41471`…) | canonical numeric form |
| `old_path` | string? | set iff `rename` |
| `blob_hash` | 64-hex? | required unless `delete`; forbidden on `delete` |

## Ref (`vcs_refs` row)

| field | type | notes |
|-------|------|-------|
| `repo_id` | char(36) | PK part |
| `name` | string ≤255 | PK part; grammar BR4.9 (id-shaped names rejected); `utf8mb4_bin` collation (0054) |
| `changeset_id` | cs_id \| NULL | column permits NULL head; CAS insert always binds `new` — NULL-head rows arise only via a legal `new=None` write, which deletes rather than nulls |
| `updated_at` | ts | dialect-managed |

## DerivedIndexRow (`vcs_derived_index` — kinds per Q3)

| kind | index_key | payload | written |
|------|-----------|---------|---------|
| `path_touch` | `{sha256(path)[0:32]}\x01{committed_at}\x01{cs}` | `{path,kind,changeset_id,committed_at,old_path?,blob?,mode}` | in create tx (BR4.12) |
| `tree_snap` | `{cs}` | `{written_at}` | materialize final call |
| `tree_entry` | `{cs}\x01{path}` | `{mode,blob}` | materialize, chunked upserts |
| `merge_base` | `{lo}\x01{hi}` | `{base}` | on first merge_base(a,b) |
| `blame` | `{cs}\x01{sha256(path)[0:32]}\x01{chunk:04}` | `BlameEntry[]` ≤32KB | on first blame(path,at) |

Every row carries `covering_changeset` (FK→changesets, CASCADE) = the changeset the row was derived from.

## Read shapes

- `LogEntry` = full `ChangesetRecord` (incl. `manifest` — wire `VcsChangesetPublic` requires it; ReadIndex calls `vcs_list_file_changes` per emitted entry (unpaginated facade call; u7's 10k manifest cap is the real bound)) + `next_cursor: Option<opaque>`.
- `TreeEntry { path, mode, blob_hash }` — paged by `index_key` within `{cs}\x01` prefix.
- `DiffEntry { path, kind, old_mode?, new_mode?, old_blob?, new_blob? }` (+ patch/`totals` joined by u7 via `text_diff`).
- `PathTouch { path, kind, changeset_id, committed_at, old_path?, blob?, mode }` — keyset order IS chronological within a path (Q3 key embeds `committed_at`).
- `BlameEntry { line_no, changeset_id, author, content }` — `author` is the canonical string; u7 re-splits to `VcsAuthor{name,email}` (lossy for names containing ` <`/… — accepted wire mapping).
- **Cursors (uniform):** log → base64 `{"n": int}`; keyset reads (tree/path_history/blame/diff) → base64 `{"k": "<last_index_key>"}` — opaque, emitted only when more rows exist; malformed → `CursorMalformed` → `vcs.invalid_input` (BR4.15).

## Error enums (typed — wire codes are u7's mapping)

```rust
ChangesetError: ParentNotFound | ObjectNotFound{hash} | ChangesetNotFound
              | InvalidInput{field,reason} | Internal | DbNotConfigured
RefError:       RefNotFound | RefConflict{actual: String} | TargetNotFound
              | InvalidInput | Internal | DbNotConfigured
IndexError:     NotFound | PathNotFound | BaseNotFound        // → no_common_ancestor
              | FrontierTooLarge                              // → too_large
              | CursorMalformed                               // → invalid_input
              | InvalidInput | Internal | DbNotConfigured
```

`DbNotConfigured` covers `Database::is_skipped()`; `Internal` covers non-sentinel DB errors, malformed stored JSON, and IO faults from `ObjectStore::get`.

## Invariants

- A changeset row is fully created or not at all (one `create_tx`); no readable partial exists.
- `tree_snap` is written LAST, in its own final upsert call after chunked `tree_entry` calls — crash mid-materialize leaves entries but no snap, so retry recomputes idempotently (same keys → same payloads under LWW).
- Index payloads are self-describing (path/cs/committed_at duplicated in payload) so reads never re-join for their own fields.
