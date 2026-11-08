# Security Design — u2-vcs-persistence

## SD2.1 — Query-construction boundaries

Runtime `sqlx::query*`/`query_scalar` + `.bind()` for every value (zero macro call sites in this crate — TD-verified). Assembled SQL only from compile-time constant fragments via `sqlx::AssertSqlSafe` (~80 sites precedent; `repo_activity.rs:193`): dialect-variant statement text and pagination tails. User data (owner/name/path/ref/idem_key/git_oid) never concatenated — always bound.

## SD2.2 — Fail-closed path

```
skipped Database ──require_pool()──> "database not configured" ──api──> db.not_configured
```
Every façade method starts `let p = self.require_pool()?;` — no statement executes on a skipped pool. Test: skipped-DB call on each façade family returns Err, zero queries issued.

## SD2.3 — Tenant isolation

All repo-scoped SELECT/INSERT/UPDATE/DELETE filter `repo_id = ?` bound from the method arg (never interpolated). `vcs_objects` is deliberately global (no repo_id) — cross-repo reads of object metadata are BY DESIGN (shared CAS); byte access is gated above at u3/u7.

## SD2.4 — Atomicity + race design

- create_tx: single `p.begin()` tx — idem lookup inside → insert-if-absent changeset → file_changes → object_links IF-ABSENT → index upserts → commit. Rollback on any error.
- Idempotent-race: `INSERT-IF-ABSENT` + `rows_affected()==0` → re-SELECT (star_repository idiom — never error-text matching).
- CAS: single statement, null-safe equality; `CasOutcome` enum travels typed — `actual` reaches `vcs.ref_conflict.data` without stringification.
- import_link: insert-if-absent (crash-retry must not violate PK).

## SD2.5 — FK action map (corrected per review R-01)

| FK | ON DELETE |
|---|---|
| every `repo_id` → repositories(id) | CASCADE |
| file_changes.changeset_id | CASCADE |
| refs.changeset_id | CASCADE |
| derived_index.covering_changeset | CASCADE |
| import_links.changeset_id | CASCADE |
| object_links.hash → vcs_objects | RESTRICT / NO ACTION (protective assertion — a linked object can't be deleted) |
| file_changes.blob_hash → vcs_objects | RESTRICT / NO ACTION (manifest history survives object GC) |
| repo_surface_flags.enabled_by → users | SET NULL |

## SD2.6 — Bounds at this layer

- `limit` clamped in-crate on paginated methods (`clamp(1,500)` precedent) — ref.list, derived_index.query, import_link.cursor_page.
- Path/key/name lengths capped by column types (VARCHAR bounds = DB-enforced cap).
- `vcs_objects` insert asserted on hash format upstream (u3) — façade trusts hex column shape.

## SD2.7 — Migration-order safety

0053 applies once (sqlx bookkeeping). sqlite rebuild runs inside `PRAGMA foreign_keys=OFF/ON` with row-count parity check in test; object tables created empty — no data migration needed.
