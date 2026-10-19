# Security Requirements — u2-vcs-persistence

## SR2.1 — Injection safety (parameterized only)

All queries through sqlx binds (`query!`-style macros or `bind()`); no string-interpolated SQL. Dynamic fragments (cursor pagination, dialect variants) go through the `AssertSqlSafe` precedent (`repo_activity.rs:193`) or per-dialect literal branches — never user text. Covers: all façade methods, cursor values, ref names, idem keys.

## SR2.2 — Fail-closed on absent DB

`require_pool()?` on every method; skipped Database → `db.not_configured` before any statement (AC1.2.2). No method may default to a stub result.

## SR2.3 — Tenant isolation by repo_id

Every repo-scoped table keyed on `repo_id`; every repo-scoped query filters `WHERE repo_id = ?`. No method accepts an owner/name — resolution to repo_id happens above (u7). Cascade delete must not bleed across repos (FKs only on repositories(id)).

## SR2.4 — Write atomicity & no partial state

create_tx is all-or-nothing (BR2.6). Idempotency must use insert-if-absent/unique-race→reselect, never error-text matching (BR2.8 — prevents dialect-specific error string sniffing). CAS returns typed `CasOutcome` — `actual` never stringified into a generic error (prevents leaking stale ref state through mismatched paths and preserves C1's `data.actual`).

## SR2.5 — Data-integrity invariants at the DB boundary

- non-delete FileChange ⇒ blob_hash NOT NULL (façade assert before insert; DB-level CHECK optional).
- ObjectLink insert-if-absent (re-reference must not collide).
- `vcs_objects` is global — MUST NOT gain repo_id (shared CAS; repo-delete must not drop shared blobs — FR1.5).
- `index_key` not `key` (mysql-reserved); bounded VARCHARs within 3072B InnoDB PK budget.

## SR2.6 — Migration safety

- sqlite rebuild keeps both activity indexes + all rows (copy verified by count parity in test).
- pg/mysql ALTER recipes keep DEFAULTs (explicit NULLs bound at insert anyway).
- Migration is additive/alter-only on `repository_activity` — existing rows survive (0053 renames nothing else, drops no data column).

## SR2.7 — No credentials / secrets in rows

VCS tables store ids, hashes, paths, payloads — never tokens, credentials, or user PII beyond actor ids (same posture as repository_activity).

## SR2.8 — FK action pins

Explicit `ON DELETE` on every FK: repo-scoped tables CASCADE on repositories(id); file_changes.blob_hash / refs.changeset_id / derived_index.covering_changeset / import_links.changeset_id CASCADE or SET NULL as pinned in entities (prevents mysql RESTRICT aborting a repo cascade); repo_surface_flags.enabled_by SET NULL (user delete must not kill flags).
