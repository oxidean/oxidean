# Rules — u2-vcs-persistence

```yaml
business_rules:
  - id: BR2.1
    name: sole-dialect-sql-site
    rule: "All VCS DDL/DML lives in crates/oxidean-db; no SQL outside it. One module vcs.rs owns row types + façade impl; lib.rs re-exports. Sanctioned exception: RepoActivityRow consumer de-wraps in oxidean-api (activity.rs/feeds.rs/users.rs) — mechanical Option de-wrap only, no logic."
    enforcement: "code review + module boundary"
    source: C5 + u1 ownership split

  - id: BR2.2
    name: skipped-db-fail-closed
    rule: "Every façade method resolves self.require_pool()?; skipped Database surfaces db.not_configured at the VCS layer — never partial write or silent default."
    enforcement: "unit test: skipped() façade call errors out"
    source: C5 + Database contract

  - id: BR2.3
    name: migration-dialect-parity
    rule: "Identical logical schema in postgres/mysql/sqlite migration 0053_vcs_schema.sql; filename parity (migrate.rs:51-69 enforces); per-dialect types: ids CHAR(36)/TEXT, hashes CHAR(64)/TEXT, bounded VARCHAR on mysql PK cols (3072B budget), TEXT-JSON for parents/payload, TIMESTAMPTZ/DATETIME(6)/TEXT timestamps."
    enforcement: "CI migration apply on three dialects"
    source: house migration convention

  - id: BR2.4
    name: repo-cascade-delete
    rule: "Every VCS table FKs repositories(id) ON DELETE CASCADE — deleting a repo removes flags, links, changesets, file_changes, refs, derived_index, import_links. No orphans."
    enforcement: "DDL + test: delete repo → counts drop to 0"
    source: components cascade pin

  - id: BR2.5
    name: activity-nullability-c4
    rule: "repository_activity loses its push_type CHECK (live CHECK covers 6 kinds incl. branch_rename — drop the constraint, not a literal); ref_name/before_oid/after_oid/commits_count → NULL; insert signature → Option<>. READ SIDE: RepoActivityRow fields → Option, map_activity! → try_get::<Option<..>>, consumers de-wrap (activity.rs row_to_item, feeds.rs ~8 sites with display fallbacks, users.rs pass-through). Dialect recipes: pg DROP CONSTRAINT + ALTER COLUMN DROP NOT NULL; mysql DROP CHECK + MODIFY COLUMN NULL; sqlite full table rebuild per 0025 (new table→copy→drop→rename→recreate both indexes)."
    enforcement: "compile sweep + activity tests stay green + new test: ref-free insert succeeds and list_activity decodes it"
    source: C4 + contribution blockers 1-2

  - id: BR2.6
    name: changeset-create-atomicity
    rule: "create_tx writes changeset + file_changes + object_links(insert-if-absent) + index_upserts in ONE transaction (pool begin/execute/commit idiom); find_by_idempotency lookup must be inside the tx or retried-after-conflict — never a pre-BEGIN gap; non-delete file_changes must carry blob_hash (assert before insert)."
    enforcement: "test: injected failure mid-tx leaves zero rows; re-referenced blob does not collide"
    source: C5 + AC3.5.2 + contribution 4+10

  - id: BR2.7
    name: ref-cas-atomic
    rule: "cas_update returns CasOutcome{Applied(RefRow)|Conflict{actual:Option<String>}} (Result<_,String> cannot carry `actual` — C1 data.actual). Single statement with null-safe equality (pg/sqlite IS NOT DISTINCT FROM; mysql <=>); mysql: rows_affected() then SELECT actual. expected_old NULL+absent ⇒ create path (pg/sqlite INSERT..ON CONFLICT..DO UPDATE..WHERE; mysql UPDATE→INSERT in tx). delete = row DELETE — absent refs, not tombstones (AC5.2.1)."
    enforcement: "test: concurrent CAS → one Applied one Conflict{actual}"
    source: C5 + AC5.1.3 + contribution 8

  - id: BR2.8
    name: idempotent-changeset-create
    rule: "Idempotency via insert-if-absent-on-idem_key or unique-race → re-SELECT (star_repository pattern, stars.rs:16-44). NEVER error-string matching — violation text differs per dialect. Byte-identical resubmit → same id; concurrent identical → one row, both return it."
    enforcement: "test: resubmit → same id, one row; concurrent pair → one row"
    source: C5 + AC2.2.x + contribution 10

  - id: BR2.9
    name: derived-index-merge-safe
    rule: "derived_index upsert_rows uses ON CONFLICT (repo_id,kind,index_key) merge — concurrent same-parent children must not collide (AC2.3.1). Column is index_key, not key (mysql-reserved)."
    enforcement: "test: two same-parent upserts both visible"
    source: C5 + AC2.3.1 + contribution 3

  - id: BR2.10
    name: no-wire-leak
    rule: "Row types stay in oxidean-db; wire DTOs in oxidean-core vcs_types. Façade returns row types/CasOutcome — no serde wire shape crosses this boundary."
    enforcement: "pub-within-db row structs; API maps to wire types"
    source: ADR-002 + U1 boundary
```

## Error mapping

| Façade result | Surfaced as |
|---|---|
| require_pool() fails (skipped) | `db.not_configured` |
| cas_update → CasOutcome::Conflict{actual} | `vcs.ref_conflict{actual}` (API maps the enum — never a String err) |
| FK violation (missing parent/changeset/object) | `vcs.not_found` |
| idem unique-race | resolved internally → existing id (never surfaces) |
