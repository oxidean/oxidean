# Functional Design Questions — u2-vcs-persistence

**Unit:** u2-vcs-persistence | **Kind:** library | **Boundary:** sole dialect-SQL site (`oxidean-db` only)

## Q1 — Migration numbering + dialect parity
**Decision:** Migrations start at `0053_vcs_schema.sql` in all three dialect dirs (postgres/mysql/sqlite), identical logical schema, dialect-idiomatic types (TIMESTAMPTZ / DATETIME / TEXT per precedent). One additional migration alters `repository_activity` (drop push_type CHECK, nullify ref/before/after/commits_count) — folded into the same 0053 file to keep one VCS migration per dialect.
[Answer]: A — single `0053_vcs_schema.sql` per dialect covering 8 new tables + repository_activity alteration.

## Q2 — `skipped()`/not_configured surface
**Decision:** Every façade method calls `self.require_pool()`; when the DB is skipped the error propagates as `db.not_configured` at the VCS layer (same mapping the platform uses). Façade is `Database`-by-value (Clone) — no new state.
[Answer]: A — require_pool + db.not_configured mapping, Database-by-value.

## Q3 — `insert_repo_activity` nullability (C4 frozen)
**Decision:** Signature changes to `ref_name: Option<&str>, before_oid: Option<&str>, after_oid: Option<&str>, commits_count: Option<i64>`; DDL drops the `push_type IN (...)` CHECK and makes the four columns NULL. All existing call sites pass `Some(...)` — a compile-time sweep, not a behavior change. `push_type` carries the `VCS_ACTIVITY_KINDS` constants.
[Answer]: A — nullable columns + Option<> signature per C4.

## Q4 — Repo row cascade
**Decision:** All 8 new tables reference `repositories(id) ON DELETE CASCADE` — deleting a repo removes its flags/links/changesets/refs/index/import rows. `repository_activity` already cascades.
[Answer]: A — ON DELETE CASCADE everywhere.

## Q5 — Idempotency + CAS surfaces
**Decision:** `changeset.find_by_idempotency(repo_id, idem_key)`; `ref.cas_update(repo, name, expected_old, new)` in one statement (`UPDATE ... WHERE expected_old` returning rows-affected → `vcs.ref_conflict` on 0). ObjectEntry CAS row is global (hash PK); ObjectLink is per-repo attribution.
[Answer]: A — per C5 signatures.

## Q6 — Facade file layout
**Decision:** One module `crates/oxidean-db/src/vcs.rs` (row types + façade impl) re-exported from lib.rs; follows the one-module-per-domain convention (repo_activity.rs, lfs.rs).
[Answer]: A — single `vcs.rs` module.
