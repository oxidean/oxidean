# Oxidean databases

Operators choose **SQLite**, **PostgreSQL**, or **MySQL** for application data via `DATABASE_URL` (and optional `OXIDEAN_DB_DIALECT`). Canonical reference for Phase 2 multi-DB support.

Local Compose defaults use throwaway credentials `oxidean` / `oxidean`. **Do not** use those in production — set real credentials and do not expose an instance without auth.

## Supported dialects

| Dialect | Sample `DATABASE_URL` | Bring-up |
|---------|----------------------|----------|
| PostgreSQL (default) | `postgres://oxidean:oxidean@postgres:5432/oxidean` | `make up` then `make smoke` |
| MySQL | `mysql://oxidean:oxidean@mysql:3306/oxidean` | `make up-mysql` then `make smoke-mysql` |
| SQLite | `sqlite:./var/oxidean.db` | `make up-sqlite` then `make smoke-sqlite` |

Print sample URLs anytime with `make help`.

## How the dialect is chosen

- Inferred from the `DATABASE_URL` scheme: `postgres://` / `postgresql://`, `mysql://`, `sqlite:` (covers `sqlite://` and `sqlite::memory:`).
- Optional `OXIDEAN_DB_DIALECT=postgres|mysql|sqlite` must **agree** with the URL scheme, or the API exits at boot with `does not match DATABASE_URL scheme`.
- `mariadb://` is **not** supported.

## Migrations

- One hand-synced `.sql` set per dialect under `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/`.
- Applied by the sqlx migrator with `_sqlx_migrations` bookkeeping.
- `OXIDEAN_AUTO_MIGRATE` defaults to `true` in Compose/dev. Set `false` for prod-like deploys and run `make db-migrate`.

## SQLite specifics

- Default file: `./var/oxidean.db` (`var/` is gitignored runtime state).
- Parent directories are created automatically at connect.
- Every connection sets `journal_mode=WAL` and `foreign_keys=ON`.
- Compose bind-mounts host `./var` to `/app/var`; there is no SQLite database container.
- Treat SQLite as single-writer. Deeper tuning and multi-replica guidance are out of scope for now.

## Diagnostic probe

Proof entity table `instances` columns: `id`, `dialect`, `probe_count`, `probed_at`.

RPC `system.db_probe` upserts and reads back that row (unauthenticated in this phase; admin-only gating arrives with the system dashboard later):

```bash
curl -fsS \
  -H 'Content-Type: application/json' \
  -H 'Oxidean-RPC-Version: 0.1.0' \
  -d '{"procedure":"system.db_probe","input":{}}' \
  http://localhost/api/rpc
```

## Switching dialect on an empty database

Phase 2 supports switching only when the **target** database is empty. No data is copied between dialects.

1. Stop the stack (`make down` / matching down target).
2. Point `DATABASE_URL` at the new empty target (and optionally set agreeing `OXIDEAN_DB_DIALECT`).
3. Run `make db-switch-dialect` — refuses a non-empty target. Override with `make db-switch-dialect ARGS=--force-empty` only when you intentionally wipe.
4. Persist the URL in `.env`.
5. Bring the stack up with the matching `make up` / `make up-mysql` / `make up-sqlite`.

Migrating a populated database across dialects is **not** supported.

## Security note

Documented `oxidean`/`oxidean` passwords are **local-only**. Set real credentials and a non-public bind before exposing an instance.
