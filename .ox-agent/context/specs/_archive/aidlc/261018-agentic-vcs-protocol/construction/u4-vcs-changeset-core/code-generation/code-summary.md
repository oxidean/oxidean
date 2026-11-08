# Code Summary — u4-vcs-changeset-core

## Landed

| File | Change |
|------|--------|
| `crates/oxidean-vcs/Cargo.toml` | `server` feature gains `dep:serde`, `dep:serde_json`, `dep:base64 = "0.23"`, `dep:hex = "0.4"`, `dep:oxidean-core` (promoted from dev-dep); `client` still dep-free |
| `crates/oxidean-vcs/src/lib.rs` | `#[cfg(feature = "server")] pub mod {changeset_store, ref_registry, read_index, text_diff};` |
| `crates/oxidean-vcs/src/changeset_store.rs` (1398 ln) | `ChangesetStore` (create/get/lineage/idempotent-resubmit/`resolve_git_oid`), `CreateChangesetInput`, `FileChangeInput`, `ChangesetRecord`, closed `ChangesetError`, `canonical_changeset()` — the single impl (U5–U7 import), private `is_changeset_id` wrapping `ChangesetId::parse`, four-literal whole-string sentinel decode |
| `crates/oxidean-vcs/src/ref_registry.rs` (651 ln) | `RefRegistry` (create/move/delete/list/`resolve` `pub(crate)`), `Ref`, closed `RefError`; CAS via `vcs_cas_ref_update` only — no pre-read TOCTOU; ref-name validator (regex + `..`/`//`/`@{`/`HEAD`/`refs/`/id-shape/hex-shape rejections) |
| `crates/oxidean-vcs/src/read_index.rs` (2819 ln) | `ReadIndex` (log/merge_base/tree/file_at/path_history/diff/blame/refs listing), `ReadIndexCaps` injectable (`seen_max`/`mark_max`/`diff_candidates_max`/`blame_touches_max`), `LogEntry`/`DiffEntry`/`TreeEntry`/`FileEntry`/`PathTouch`/`BlameLine`, closed `IndexError`, static `pub(crate) update_for -> Vec<PathTouchRow>` (owned rows; ChangesetStore borrows at tx site) |
| `crates/oxidean-vcs/src/text_diff.rs` (233 ln) | `diff_lines(old: &[&[u8]], new: &[&[u8]]) -> Vec<LineHunk>` — Myers O(ND) hand-rolled, byte-slice lines (lossless non-UTF-8), `LINE_CAP = 200_000` whole-range fallback |
| `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0054_vcs_key_collation.sql` | MySQL: four `MODIFY COLUMN` re-declarations (`index_key VARCHAR(560)`, `refs.name`, `file_changes.path` NOT NULL, `file_changes.old_path NULL` — all `utf8mb4/utf8mb4_bin`); pg/sqlite: comment-only parity files |
| `crates/oxidean-db/tests/dialect_vcs.rs` | `DATABASE_URL`-gated skip-pass + `SERIAL` mutex: migrate → `vcs_create_changeset_tx` round-trip → CAS arms → exact-sentinel text → case-variant coexistence + `information_schema` collation asserts; per-test unique ids (global `vcs_changesets.id` PK) |
| `crates/oxidean-db/src/vcs.rs` | **Deviation (required)**: Postgres arm of `vcs_list_file_changes` gains `mode::bigint AS mode` — PG `INT4` vs `VcsFileChangeRow.mode i64` broke sqlx strict decode on real Postgres (`ChangesetStore::get` path). 4 lines, existing query only, no new facade method |
| `.github/workflows/ci.yml` | `db-dialects` `-E` filter → `'binary_id(oxidean-db::dialect_probe) or binary_id(oxidean-db::dialect_vcs)'`; :814-815 comment refreshed |
| `docs/TESTING.md` | db-dialects row updated to the qualified `or` filter form |

## Pinned decisions implemented

- **Canonical id** (SD4.2): `cs_`+26 uppercase Crockford from first 128 bits of `SHA-256(repo_id‖parents-sorted‖manifest_json‖author‖committed_at‖message)`; `manifest_json` is hash material only — `manifest_hash` stored; `idem_key` = full 256-bit digest; hand-packed Crockford (no ulid dep). `repo` in input — the global id PK would otherwise cross-collide repos.
- **One tx** (SD4.3): all input validation precedes any DB call; `vcs_create_changeset_tx` commits changeset+file_changes+object_links+derived upserts atomically; idempotent resubmit probes `vcs_find_changeset_by_idem` and returns the existing row without writing.
- **Sentinels** (SD4.5): four literals, whole-string exact; everything else → `Internal`; MySQL CAS create-race raw `Err` → `Internal` (R18, not a fifth sentinel).
- **CAS arms** (SD4.6): `expected_old=None,new=None` = assert-absent noop → `Ok`; absent delete/move → `RefNotFound`; `actual:Some` → `RefConflict{actual}`; `actual:None` → `RefNotFound`; `Applied` → row.
- **Key grammar** (SD4.9): `path_touch` `{hash32}\x01{ts}\x01{cs}` + rename `\x01r` discriminator rows; `tree_entry {cs}\x01{path}`; `tree_snap {cs}`; `merge_base {lo}\x01{hi}`; blame `{at}\x01{hash32}\x01{chunk:04}`; payload field `changeset_id`; manifest-wide `old_path` uniqueness enforced at validation.
- **Walks** (SD4.8): `log` `BinaryHeap<(String,String)>` raw `(committed_at,id)` keys + skip-`n` replay; `merge_base` mark-BFS capped + level-walk + cache row; `diff` `BTreeSet` path-ordered candidates; `blame` ancestry-ordered touches, `ObjectStore::get → File → bytes → split b'\n'`, shards ≤32KB; `tree`/`file_at` first-parent fold — `tree_entry` chunks before `tree_snap` (crash-resume); `vcs_get_changeset` existence check before every walk.
- **Cursors** (SD4.10): base64url-nopad compact JSON ≤1KB; all five shapes; malformed → `CursorMalformed`.
- **Observability** (SR4.12): every typed-error construction warns once via a `warned()` helper (`error = %e` carries the typed error; ids/hashes/counts as fields, never path/message/author payloads); `tracing::info!` on index-write counts.
- **Error model** (post-review hardening): `InvalidInput(String)` and `Internal(String)` carry `field: reason` text / raw detail — `Internal` keeps the opaque-to-wire contract while hosts get real diagnostics; sentinel decode stays whole-string exact.
- **Manifest fold semantics**: `materialize` + `file_state_at` apply each file_change entry's removals-before-write sequentially in manifest order — chained renames `{A→B, B→C}` move `B` away again (no phantom), and last-toucher-wins for `file_at`.
- **tree prefix**: literal `"/"` rejected (`prefix: use empty for the root listing`) rather than silently coercing to root.
- **Blame shards**: incremental size accounting (envelope + per-entry JSON) replaces per-line whole-chunk re-serialization; `write_blame_shards` propagates serialize errors instead of `unwrap_or_default`.
- **Fingerprint-bound deferrals**: `versioning.toml depends_on` + `Makefile db-matrix` NOT in this commit (build-and-test stage, post-unit-close).

## Deviations

- `crates/oxidean-db/src/vcs.rs` — the `mode::bigint` cast (table above). Found by the live-Postgres dialect leg; without it `vcs_list_file_changes` errors on every real-PG call. Path is bound to u2's source manifest — accepted change-notice re-baseline.
- `dialect_vcs.rs` uses per-test constant ids (`CS_RT`/`CS_RT2`/`CS_CV`) because `vcs_changesets.id` is a global PK — shared ids would `ON CONFLICT`-suppress across tests.
- Repo-seeding helper auto-inserts object rows via `insert_if_absent` so blame tests get real blob sizes.

## Verification (all executed on this tree)

- `cargo nextest run -p oxidean-vcs --features server` — **69/69 pass**
- `cargo test -p oxidean-vcs --features server` — 69/69 pass (fallback parity)
- `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings` — clean
- `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server` — clean
- `cargo nextest run -p oxidean-db` — **74/74 pass** (incl. `migration_parity` + 3 dialect_vcs skip-pass)
- `cargo check -p oxidean-vcs` (client) — clean; `--all-features` — clean
- `cargo fmt --check` — clean
- **Live dialect legs (Docker)**: Postgres 16 `dialect_vcs` 3/3, MySQL 8.4 `dialect_vcs` 3/3 (incl. `information_schema` utf8mb4_bin + VARCHAR(560) asserts), SQLite 3/3; `dialect_probe` unregressed on both real engines.
- `Cargo.lock` delta: +4 optional deps on `oxidean-vcs` only (base64/hex/serde/serde_json — all lockfile-resident).

## Test counts

69 crate tests: 8 `text_diff` golden/coarse vectors; `changeset_store` canonical-id vectors + create/idempotent/concurrent/missing-parent/object/sentinel/fail-close/no-echo arms; `ref_registry` validation arms + CAS all-outcomes + list/resolve; `read_index` 16 tests (log pagination+from-resolution, merge-base linear/diamond/disjoint/cached/caps, tree materialize+prefix+crash-resume, file_at, path_history+rename+payload-guard, diff order/filter/reverse/caps, blame attribution/rename/binary/shard/caps, skipped-db). 3 `dialect_vcs` integration tests × 3 engines.
