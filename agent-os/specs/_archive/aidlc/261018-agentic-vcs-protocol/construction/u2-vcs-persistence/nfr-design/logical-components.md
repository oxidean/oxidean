# Logical Components — u2-vcs-persistence

## LC2.1 — `oxidean-db::vcs` module (new file `src/vcs.rs`)

- Row structs (pub): `VcsRepoSurfaceFlagRow`, `VcsObjectRow`, `VcsObjectLinkRow`, `VcsChangesetRow`, `VcsFileChangeRow`, `VcsRefRow`, `VcsDerivedIndexRow`, `VcsImportLinkRow` + `CasOutcome{Applied(VcsRefRow)|Conflict{actual:Option<String>}}` + `VcsRepoObjectCounts{object_count:i64,storage_bytes:i64}`.
- `impl Database` block in-module (deliberate consolidation — lib.rs is ~4,941 lines; private `require_pool`/`dialect`/fields are crate-visible to the descendant module).
- ~20 façade methods per C5 + corrections (insert_if_absent on object_link/import_link, count_for_repo pair, changeset.count_for_repo).
- JSON TEXT cols decode at boundary: `parents`, `payload` stay `String` on rows; serde_json at the oxidean-vcs edge.

## LC2.2 — Migration `0053_vcs_schema.sql` ×3

- 8 CREATE TABLEs + 7 secondary indexes + repository_activity alteration (per-dialect recipe: pg DROP CONSTRAINT+ALTER×4; mysql DROP CHECK+MODIFY×4; sqlite rebuild per 0025 + index recreation).
- filename parity enforced by `migration_parity` test (migrate.rs:51-69).

## LC2.3 — `repo_activity.rs` edits (same crate)

- `RepoActivityRow` 4 fields → Option; `map_activity!` → `try_get::<Option<..>>` (i32→i64 fallback kept); `InsertRepoActivity` fields → Option; insert_activity binds Option values.

## LC2.4 — `oxidean-api` consumer de-wraps (sanctioned exception)

- `repo/activity.rs` row_to_item (already emits Option fields — de-wrap).
- `routes/feeds.rs` ~8 sites + 2 test literals → display fallbacks (""/0) for None.
- `admin/users.rs` → pass Option through (fields already Option).
- insert-side call sites get `Some(...)` — `repo/activity.rs:144,183,220,264,299` + test files.

## LC2.5 — lib.rs re-exports

`pub mod vcs;` + row/CasOutcome re-exports; `insert_repo_activity` signature updated in the existing impl block.

## Dependency edges

- vcs.rs depends on: pool.rs (Pool), dialect.rs (Dialect), repo_activity.rs (InsertRepoActivity shape only).
- Nothing in vcs.rs is consumed by other db modules — leaf module.
- Upstream: u1's `VCS_ACTIVITY_KINDS` (core constants used in docs/tests only — no code edge into db).
