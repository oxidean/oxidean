# Tech-Stack Decisions — u2-vcs-persistence

## TD2.1 — sqlx + per-dialect migrations (existing)

Extend the house pattern: `sqlx::migrate!` embedded per-dialect dirs (`migrate.rs:5-12`), `Pool`/`Dialect` abstraction (`pool.rs`, `dialect.rs`). No ORM, no new query layer. Migration `0053_vcs_schema.sql` ×3; filename parity enforced by `migrate.rs:51-69` test.

## TD2.2 — `Database`-by-value façade on `impl Database`

Façade methods live on `impl Database` inside `src/vcs.rs` (crate-visible `require_pool`, `pool()`, `dialect()`); row types `pub` in the same module, re-exported in lib.rs. Matches repo_activity.rs / lfs.rs / stars.rs conventions exactly.

## TD2.3 — Result<T, String> + typed outcome enums where data must travel

House error channel is `Result<_, String>`; where the contract needs structured data (`vcs.ref_conflict{actual}`), return a typed enum (`CasOutcome`) instead of smuggling in strings — API maps it to AppError data (C1).

## TD2.4 — Per-dialect SQL idioms (pinned)

- Upsert: pg/sqlite `INSERT..ON CONFLICT..DO UPDATE/NOTHING`, mysql `INSERT IGNORE` / `ON DUPLICATE KEY UPDATE`.
- Null-safe equality: pg/sqlite `IS NOT DISTINCT FROM`, mysql `<=>`.
- Insert-returning: pg/sqlite `RETURNING`, mysql `rows_affected()` + re-SELECT.
- sqlite table rebuild for repository_activity (0025 precedent); pg DROP CONSTRAINT+ALTER COLUMN; mysql DROP CHECK+MODIFY COLUMN.

## TD2.5 — Storage formats

JSON-in-TEXT for `parents`/`payload` (0047 perms_json precedent) — never TEXT[]/JSONB (zero precedent). Bare hex for hash/git_oid storage (`sha256:` is wire-only). Bare ref names (no refs/heads/ prefix). `index_key` (mysql-reserved `key` avoided).

## TD2.6 — No new dependencies

`oxidean-db` dependency set unchanged (sqlx, serde, serde_json already present). The VCS unit lands schema + façade only; U3+ own behavior.

## TD2.7 — Test dialect

Unit tests run on the sqlite pool (house convention — migration applies identically); CI covers all three via migration apply + dialect matrix.
