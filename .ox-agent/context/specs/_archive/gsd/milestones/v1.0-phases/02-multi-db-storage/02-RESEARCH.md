# Phase 2: Multi-DB Storage — Research

**Researched:** 2026-09-09
**Objective:** Answer "What do I need to know to PLAN this phase well?" for operator-selectable SQLite/PostgreSQL/MySQL storage (PLAT-07, PLAT-08; PLAT-09 CI adjacency).

---

## User Constraints

*(Copied verbatim from `02-CONTEXT.md` — locked; plan HOW to satisfy these, not whether.)*

### Domain
- Operators can choose SQLite, PostgreSQL, or MySQL for app data with working migrations and core write/read flows on all three.
- Requirements: PLAT-07, PLAT-08.
- Success criteria: (1) operator can configure instance dialect via config/env, (2) migrations apply cleanly on each dialect, (3) a core app write/read flow succeeds against each dialect in local Compose.
- Carried forward from Phase 1 (not re-opened): D-06 default Compose is Postgres; D-07 MySQL via profile, SQLite via env/file (no container); D-08 `oxidean-db` is the uniform adapter boundary.

### Decisions (D-01 … D-20)
- **D-01:** Prefer inferring dialect from `DATABASE_URL` scheme (`postgres://`/`postgresql://`, `mysql://`, `sqlite:`/`sqlite://`); optional `OXIDEAN_DB_DIALECT=postgres|mysql|sqlite` must agree with the URL when set.
- **D-02:** Dialect/URL mismatch fails fast at startup with a clear operator-facing error.
- **D-03:** Dialect switch is supported in Phase 2 only for an **empty** target DB, with a documented recipe (and Make helper — D-14).
- **D-04:** Canonical operator docs: `.env.example` + README + `make` help targets that print sample URLs + `docs/database.md`.
- **D-05:** Shared **logical** migrations adapted per dialect via a **thin** adapter (types / autoincrement / quoting) — not a full migration DSL.
- **D-06:** Materialize/adapt into **sqlx-compatible** per-dialect migration sets; use sqlx migrator bookkeeping (`_sqlx_migrations`).
- **D-07:** `OXIDEAN_AUTO_MIGRATE=true` by default in Compose/dev (`.env.example`); when false (prod-like), require explicit `make db-migrate` / CLI.
- **D-08:** Keep Phase 2 schema trivial so portability stays easy.
- **D-09:** Proof entity is an early **product-ish** table (e.g. `instances` or `settings`) that later phases may keep — not a disposable `smoke_kv`-only table.
- **D-10:** Schema includes a **dialect/version stamp** so operators can see which dialect the write hit.
- **D-11:** Keep as a **supported diagnostic** long-term.
- **D-12:** Shared Rust diagnostic write/read in `oxidean-db`/core; expose via minimal RPC (e.g. `system.db_probe`) so Compose smoke, CI, and a future system-admin diagnostics menu share one path — no Phase 2 diagnostics UI.
- **D-13:** Phase 2 proves the flow via cargo integration tests + Compose/Make smoke calling that diagnostic (RPC and/or Rust path).
- **D-14:** Explicit Make targets: `make up`, `make up-mysql`, `make up-sqlite` (or equivalent).
- **D-15:** CI proves **all three** dialects in a **matrix** in Phase 2.
- **D-16:** Empty-DB dialect switch: `make db-switch-dialect` (refuse unless empty / `--force-empty`) plus steps in `docs/database.md`.
- **D-17:** Default SQLite file path: `./var/oxidean.db` (runtime-state style; gitignore `var/`).
- **D-18:** API creates missing parent directories on startup when dialect is SQLite.
- **D-19:** `make up-sqlite`: no DB container; api+web(+Traefik) with SQLite file **bind-mounted** from host `./var` (not `./data`).
- **D-20:** Sensible SQLite defaults only (`WAL`, `foreign_keys=ON`) documented; no deep tuning in Phase 2.

### Claude's Discretion
- Exact table name (`instances` vs `settings`) and column names within D-09–D-10.
- Exact RPC procedure name/shape under `system.*` as long as D-12 holds.
- Thin logical→dialect adaptation mechanism (templates vs small Rust rewriter) as long as D-05–D-06 hold.
- Whether smoke hits RPC through Traefik or calls a small CLI sharing the same Rust diagnostic.
- sqlx feature-flag layout and Cargo workspace wiring for three dialects.
- Auth gate for `system.db_probe` may remain open/local in Phase 2; admin-only enforcement lands with system dashboard.

### Deferred (do not build in Phase 2)
- System dashboard / diagnostics UI (capability only, no UI).
- Auth-gating `system.db_probe` to system-admin (later, with real roles).
- Non-empty dialect migration / data portability between dialects.
- Deep SQLite tuning, multi-replica guidance.
- PLAT-09 full resolution (belongs to Phase 22; Phase 2 only needs the matrix CI groundwork per D-15).

---

## Standard Stack

- **sqlx 0.8** (already pinned in `crates/oxidean-db/Cargo.toml`) — add `mysql` and `sqlite` features alongside the existing `postgres`. [VERIFIED: crates/oxidean-db/Cargo.toml]
- Pin exact dependency set with `default-features = false` and an explicit feature list, mirroring how the codebase already avoids unused defaults elsewhere:
  ```toml
  sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio", "tls-rustls",
    "postgres", "mysql", "sqlite",
    "migrate", "chrono",
  ] }
  ```
  [CITED: https://docs.rs/crate/sqlx/0.8.6 feature table] — `default` pulls in `any, macros, migrate, json`; none of `any` or `macros` are needed here (see Don't Hand-Roll / Pitfalls). `tls-rustls` avoids adding an OpenSSL/native-tls system dependency to the `debian:bookworm-slim`/`rust:1-bookworm` Docker images already in use. [ASSUMED: rustls is the simpler default for this Debian-based image; native-tls would also work but adds an apt dependency]
- **No `sqlx::Any`/`AnyPool`.** Use three concrete pool types (`PgPool`, `MySqlPool`, `SqlitePool`) behind a small internal enum. `Any` exists but its own docs say "SEE DOCUMENTATION BEFORE USE" — it doesn't support the `query!`/`query_as!` compile-time macros, requires calling `sqlx::any::install_default_drivers()` once at startup, and still needs dialect-specific SQL text for anything beyond trivial queries (placeholder syntax differs: `$1` vs `?` vs `?1`). Since D-05 already commits to a **thin per-dialect adapter** (not one dialect-agnostic driver), a concrete-pool enum is the simpler, more explicit match for the locked decisions. [CITED: https://docs.rs/sqlx/latest/sqlx/any/index.html]
- **No `sqlx::query!`/`query_as!` macros.** These require either a live DB or an offline `.sqlx` query cache at *build* time, tied to one schema/dialect — incompatible with "one binary, three dialects chosen at runtime." The current code already avoids them (`sqlx::query("SELECT 1").execute(pool)` in `ping()`), so Phase 2 should continue using plain `sqlx::query()` / `sqlx::query_as::<_, T>()` runtime calls. [VERIFIED: crates/oxidean-db/src/lib.rs current `ping()` impl]
- **`sqlx::migrate!` macro is fine and unrelated to the above.** It only embeds `.sql` files as compiled-in byte arrays at build time (no DB connection needed to build); the DB connection happens later, at `Migrator::run(&pool)`. Safe for Docker multi-stage builds with no DB reachable during `cargo build`. [CITED: https://docs.rs/sqlx/latest/sqlx/macro.migrate.html]
- **sqlite feature is bundled** (`sqlx-sqlite` vendors/statically links libsqlite3 by default), which needs a C compiler at build time. `rust:1-bookworm` (current builder image) and `ubuntu-latest` GitHub runners both ship a C toolchain by default, so no Dockerfile/CI change is required for this. [ASSUMED: verified indirectly — official rust Docker images and GH ubuntu-latest runners include build-essential/gcc; not independently re-verified in this environment]

---

## Architecture Patterns

### 1. Dialect resolution (D-01/D-02)

Resolve dialect **before** connecting, from the `DATABASE_URL` scheme, by prefix match (not a strict `url::Url::parse`, since `sqlite:./var/oxidean.db` and `sqlite::memory:` are not standard RFC-3986 URLs but are valid sqlx SQLite URLs):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect { Postgres, MySql, Sqlite }

impl Dialect {
    pub fn from_url(url: &str) -> Result<Self, String> {
        if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(Dialect::Postgres)
        } else if url.starts_with("mysql://") {
            Ok(Dialect::MySql)
        } else if url.starts_with("sqlite:") {
            Ok(Dialect::Sqlite)
        } else {
            Err(format!("unrecognized DATABASE_URL scheme: {url}"))
        }
    }

    pub fn as_env_str(self) -> &'static str {
        match self {
            Dialect::Postgres => "postgres",
            Dialect::MySql => "mysql",
            Dialect::Sqlite => "sqlite",
        }
    }
}

pub fn resolve_dialect(url: &str) -> Result<Dialect, String> {
    let from_url = Dialect::from_url(url)?;
    if let Ok(declared) = std::env::var("OXIDEAN_DB_DIALECT") {
        let declared = match declared.as_str() {
            "postgres" => Dialect::Postgres,
            "mysql" => Dialect::MySql,
            "sqlite" => Dialect::Sqlite,
            other => return Err(format!("unknown OXIDEAN_DB_DIALECT: {other}")),
        };
        if declared != from_url {
            return Err(format!(
                "OXIDEAN_DB_DIALECT={} does not match DATABASE_URL scheme (detected {})",
                declared.as_env_str(), from_url.as_env_str()
            ));
        }
    }
    Ok(from_url)
}
```

This should run in `main.rs` before `Database::from_env()`, and a mismatch/parse error should `eprintln!` + `process::exit(1)` (matching the existing fail-fast pattern already used for CORS config errors in `main.rs`). [VERIFIED: crates/oxidean-api/src/main.rs current CORS error-exit pattern — reuse the same style for D-02]

### 2. Concrete-pool enum, not a trait object (D-08 "keep schema trivial")

```rust
pub enum DbPool {
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
}

pub struct Database {
    pool: Option<DbPool>,
    dialect: Option<Dialect>,
}
```

`ping()`, `db_probe_write_read()`, and future queries match on the enum and issue dialect-appropriate SQL text. This keeps `oxidean-db` as the single dialect-branch point (D-08 carried forward: "API must not dialect-branch ad hoc") while staying honest that the three dialects are *not* SQL-identical.

### 3. SQLite connect specifics (D-17/D-18/D-20)

```rust
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::str::FromStr;

async fn connect_sqlite(url: &str) -> Result<sqlx::SqlitePool, String> {
    // sqlx does NOT create parent directories — only the file itself
    // (and only when create_if_missing(true) is set). D-18 requires this step.
    if let Some(path) = url.strip_prefix("sqlite:").map(|p| p.trim_start_matches("//")) {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
    }

    let opts = SqliteConnectOptions::from_str(url)
        .map_err(|e| e.to_string())?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .connect_with(opts)
        .await
        .map_err(|e| e.to_string())
}
```

`.foreign_keys(true)` on `SqliteConnectOptions` is the sqlx-native way to set `PRAGMA foreign_keys=ON` per-connection (SQLite requires this pragma on every connection, it is not persisted in the file). `.journal_mode(Wal)` sets `PRAGMA journal_mode=WAL` (persisted in the file after first set). [CITED: https://docs.rs/sqlx/latest/sqlx/sqlite/struct.SqliteConnectOptions.html]

D-17's default path `./var/oxidean.db` should be read from `DATABASE_URL` like any other dialect (i.e., `.env.example`'s SQLite line becomes `DATABASE_URL=sqlite:./var/oxidean.db`), not hardcoded — the default only needs to *appear* as the example/Make-generated URL.

### 4. Migrations: parallel per-dialect directories, sqlx migrator bookkeeping (D-05/D-06)

Given D-08 ("keep Phase 2 schema trivial") and a single-table proof entity, a full templating/codegen system is overkill. The **prescriptive** thin-adapter mechanism for Phase 2:

```
crates/oxidean-db/migrations/
  postgres/0001_init.sql
  mysql/0001_init.sql
  sqlite/0001_init.sql
```

One hand-maintained `.sql` file per dialect per logical migration step, kept in lockstep by convention (same filename prefix `0001_`, same column set/order, comment header noting the "logical" migration they implement). A single `cargo test` asserts structural parity (see Validation Architecture) so drift is caught mechanically rather than trusted to memory. This is the documented **Claude's Discretion** choice among "templates vs small Rust rewriter" — hand-synced parallel SQL is simplest given a one-table schema, and avoids building throwaway codegen for Phase 2. Revisit only if the schema grows non-trivially in a later phase.

Each dialect's migrations are embedded and run via `sqlx::migrate!`, one macro invocation per dialect (each produces its own `'static Migrator`, this is normal/expected sqlx usage, not a hack):

```rust
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::Error> {
    match pool {
        DbPool::Postgres(p) => sqlx::migrate!("migrations/postgres").run(p).await?,
        DbPool::MySql(p) => sqlx::migrate!("migrations/mysql").run(p).await?,
        DbPool::Sqlite(p) => sqlx::migrate!("migrations/sqlite").run(p).await?,
    }
    Ok(())
}
```

Each dialect gets its own `_sqlx_migrations` bookkeeping table (D-06) — sqlx creates this automatically per target DB on first `Migrator::run`. [CITED: https://docs.rs/sqlx/latest/sqlx/migrate/struct.Migrator.html]

`sqlx::migrate!(path)` resolves `path` relative to `CARGO_MANIFEST_DIR` of the crate it's invoked in (i.e., `crates/oxidean-db/`), so `"migrations/postgres"` is correct if the macro is called from within `oxidean-db`.

### 5. Auto-migrate flag (D-07)

```rust
let auto_migrate = std::env::var("OXIDEAN_AUTO_MIGRATE")
    .map(|v| v == "true" || v == "1")
    .unwrap_or(true); // default true in Compose/dev per D-07; operators flip to false for prod-like
if auto_migrate {
    run_migrations(&pool).await?;
}
```

Called from `oxidean-api` startup, after `Database::from_env()` succeeds and before the server starts listening — a migrate failure should be fail-fast (`process::exit(1)`), same pattern as bind failure in current `main.rs`.

For `OXIDEAN_AUTO_MIGRATE=false`, an explicit `make db-migrate` target must exist and run the *same* `run_migrations` path — the cleanest way is a tiny binary in `oxidean-db` (e.g. `crates/oxidean-db/src/bin/migrate.rs`) that does `Database::from_env()` + `run_migrations()` and exits, wired as `cargo run -p oxidean-db --bin migrate`. This reuses D-12's "share one path" philosophy (no separate migration logic duplicated between server startup and CLI).

### 6. Proof entity + `system.db_probe` RPC (D-09–D-13)

Recommended table (Claude's discretion on exact name; `instances` fits "product-ish, later phases may keep" better than `settings`, which implies key/value):

```sql
-- logical shape (see per-dialect migrations for exact types)
CREATE TABLE instances (
  id           <int-pk>              -- singleton row, id = 1
  dialect      <text> NOT NULL,      -- 'postgres' | 'mysql' | 'sqlite' (D-10 stamp)
  probe_count  <int> NOT NULL DEFAULT 0,
  probed_at    <timestamp> NOT NULL
);
```

Per-dialect column types (the "thin adapter" surface — types/autoincrement/quoting per D-05):

| Concern | Postgres | MySQL | SQLite |
|---|---|---|---|
| PK | `id INTEGER PRIMARY KEY` (no autoincrement needed — always inserting literal `1`) | `id INT PRIMARY KEY` | `id INTEGER PRIMARY KEY` |
| Timestamp type | `TIMESTAMPTZ` | `TIMESTAMP` | `TEXT` (SQLite has no native temporal type; ISO-8601 text is the sqlx/community convention) |
| now() function | `now()` | `NOW()` | `CURRENT_TIMESTAMP` |
| Upsert | `INSERT ... ON CONFLICT (id) DO UPDATE SET ...` | `INSERT ... ON DUPLICATE KEY UPDATE ...` | `INSERT ... ON CONFLICT(id) DO UPDATE SET ...` |
| Placeholder | `$1, $2, ...` | `?, ?, ...` | `?1, ?2, ...` or `?` |

`system.db_probe` RPC handler (add to `rpc.rs` `dispatch()` alongside `system.health`/`system.echo`):

```rust
"system.db_probe" => {
    match db.probe().await {
        Ok(result) => RpcResponse::ok(result), // { dialect, probe_count, probed_at }
        Err(e) => RpcResponse::err(AppError::new("db.probe_failed", e)),
    }
}
```

`Database::probe()` lives in `oxidean-db`, does the dialect-branched upsert-then-read, and is the single shared path for: cargo integration tests, Compose smoke script, and (future, deferred) system-admin diagnostics UI — satisfying D-12 directly. No auth gate in Phase 2 (explicit discretion).

### 7. Compose topology (D-14/D-19, carried D-06/D-07)

- `docker-compose.yml` (default): unchanged, Postgres service + api depends_on it. Already correct. [VERIFIED: docker-compose.yml]
- `docker-compose.mysql.yml` (profile overlay): unchanged in shape; already correct. [VERIFIED: docker-compose.mysql.yml]
- **New** `docker-compose.sqlite.yml` overlay for `make up-sqlite` (D-19): no new DB service; overrides `api.environment.DATABASE_URL` to `sqlite:/app/var/oxidean.db` (container-internal path) and adds a bind mount `./var:/app/var`, and **removes** the Postgres `depends_on`. Compose overlays can't easily *remove* a `depends_on` key from the base file — the practical pattern is to give the `api` service in the base file a `depends_on` that is itself overridable, or restructure so the base `docker-compose.yml`'s Postgres dependency is expressed via a Compose profile too (e.g. tag `postgres` service with a default profile using the `profiles: ["postgres"]` + `COMPOSE_PROFILES=postgres` default-on env trick, or simply document `make up-sqlite` as `docker compose -f docker-compose.yml -f docker-compose.sqlite.yml up --build --scale postgres=0` alternative). **Prescriptive recommendation:** give the `postgres` service in base `docker-compose.yml` `profiles: ["postgres"]` and set `COMPOSE_PROFILES=postgres` as the *default* Make-injected env for `make up`/`make up-mysql`; `make up-sqlite` simply doesn't set that env, so Postgres never starts and `api.depends_on.postgres` needs to be dropped for the sqlite override — Compose overlays **can** override `depends_on` by redeclaring the `api` service's `depends_on` list entirely in the sqlite overlay (last-value-wins per key for scalars, but `depends_on` list needs full redeclaration, not a merge) or converting `api.depends_on` in the base file to only include what's essential when profile is active. Recommend testing `docker compose config` output for whichever final approach is chosen (this is a real compose-file design decision with more than one valid encoding — flag as a planning-time detail to nail down rather than a solved algorithm here).
- Mount path: `./var:/app/var` on host side, matching D-17 (`./var/oxidean.db`, not `./data`). Update `.gitignore` to include `var/` (currently only has `data/`). [VERIFIED: .gitignore current content lacks `var/`]
- `make up-sqlite` help text + target added to `Makefile` alongside existing `up`/`up-mysql`-style entries (only `up`/`down`/`logs`/`smoke` currently exist — `up-mysql`/`up-sqlite` don't exist yet and must be added per D-14). [VERIFIED: Makefile current targets]

### 8. CI matrix (D-15, PLAT-09 adjacency)

Extend `.github/workflows/ci.yml` with **one job, real `strategy.matrix`**, not three hand-copied jobs — this is the literal reading of D-15 ("in a matrix"):

```yaml
  db-matrix:
    name: db-${{ matrix.dialect }}
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - dialect: postgres
            database_url: postgres://oxidean:oxidean@localhost:5432/oxidean
          - dialect: mysql
            database_url: mysql://oxidean:oxidean@127.0.0.1:3306/oxidean
          - dialect: sqlite
            database_url: sqlite:./var/oxidean.db
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER: oxidean
          POSTGRES_PASSWORD: oxidean
          POSTGRES_DB: oxidean
        ports: ["5432:5432"]
        options: >-
          --health-cmd="pg_isready -U oxidean" --health-interval=5s --health-timeout=5s --health-retries=10
      mysql:
        image: mysql:8.4
        env:
          MYSQL_USER: oxidean
          MYSQL_PASSWORD: oxidean
          MYSQL_DATABASE: oxidean
          MYSQL_ROOT_PASSWORD: oxidean
        ports: ["3306:3306"]
        options: >-
          --health-cmd="mysqladmin ping -h 127.0.0.1 -uroot -poxidean" --health-interval=5s --health-timeout=5s --health-retries=20
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: run migrations + probe test
        env:
          DATABASE_URL: ${{ matrix.database_url }}
          OXIDEAN_DB_DIALECT: ${{ matrix.dialect }}
        run: cargo test -p oxidean-db --test dialect_probe -- --nocapture
```

Both `postgres` and `mysql` services boot on every matrix leg (including the `sqlite` leg, where they're simply unused) — GitHub Actions `services:` cannot be conditioned on `matrix.*` values, so the pragmatic tradeoff is a few extra seconds of unused service boot time on the sqlite leg rather than three separate job blocks. [ASSUMED: this GH Actions `services:` limitation — services keys don't support skip conditions, only image/env can use expressions, but the whole block can't be omitted per-matrix-leg — based on general GH Actions documentation knowledge, not independently re-verified in this session] If this tradeoff is unwanted, the alternative is three explicit jobs (`db-postgres`, `db-mysql`, `db-sqlite`) each with only their one relevant service — functionally equivalent for D-15, more YAML, no wasted service boot. **Planner should pick one of these two encodings explicitly**; both satisfy "all three dialects in CI."

Existing `compose` CI job (`docker compose config` validation only, no bring-up) stays as-is; D-15's per-dialect proof is at the `cargo test` layer, not full Compose bring-up in CI (Compose bring-up CI is Phase 22 / PLAT-03, out of scope here). [VERIFIED: .github/workflows/ci.yml current `compose` job only runs `config`, not `up`]

### 9. Empty-DB dialect switch (D-03/D-16)

`make db-switch-dialect` should:
1. Read target `DATABASE_URL` (new dialect) from env/arg.
2. Connect and check "is empty": query dialect-appropriate system catalog for user-table count (Postgres/MySQL: `information_schema.tables` filtered to the target schema/database excluding sqlx's own `_sqlx_migrations`; SQLite: `sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'`).
3. Refuse (nonzero exit, clear message) unless count is 0, or `--force-empty` is passed.
4. Run `run_migrations()` against the new URL/dialect.
5. Print the operator's next step (update `.env`/`DATABASE_URL` permanently).

This is a small script or Rust CLI subcommand — either shells out via the same `migrate` binary from §5 with an `--assert-empty` flag, or a separate `scripts/db-switch-dialect.sh` that calls it. Document the manual recipe in `docs/database.md` regardless (D-16 requires both the Make target and doc steps).

---

## Don't Hand-Roll

- **A generic multi-dialect query builder / ORM.** Not needed for a one-table proof schema; sqlx's runtime `query()`/`query_as()` plus three short hand-written SQL statements per operation is simpler and matches D-05's "thin adapter, not full DSL."
- **A custom migration runner.** Use `sqlx::migrate!` + `Migrator::run` per dialect (D-06 explicitly requires "sqlx migrator bookkeeping").
- **URL parsing via the `url` crate for dialect detection.** SQLite URLs (`sqlite:./var/oxidean.db`, `sqlite::memory:`) aren't standard RFC-3986 URLs and can fail strict parsers; simple prefix matching (as sqlx itself effectively does internally) is sufficient and more robust here.
- **`sqlx::Any`/`AnyPool` as "the" abstraction.** Tempting for "one pool type," but it forces giving up compile-time-adjacent ergonomics and still needs per-dialect SQL text for anything beyond `SELECT 1`; a concrete-pool enum is more honest about where dialect differences live (see Architecture Patterns §2).
- **`sqlx::query!`/`query_as!` compile-time macros anywhere in this phase.** They assume one schema/one DB reachable at build time; multi-dialect-at-runtime is fundamentally incompatible with that model. This is the single highest-risk "looks idiomatic but breaks the build" trap for this phase.
- **A custom SQLite directory-creation library.** `std::fs::create_dir_all` on the parent of the parsed path is enough (D-18); sqlx's `create_if_missing(true)` only creates the file, never parent directories. [CITED: sqlx SqliteConnectOptions docs — create_if_missing behavior]
- **Hand-rolled connection pooling/retry logic.** `PgPoolOptions`/`MySqlPoolOptions`/`SqlitePoolOptions` with sane `max_connections` already covers Phase 2's needs; D-20 explicitly defers deep tuning.

---

## Common Pitfalls

1. **Using `query!`/`query_as!` macros breaks multi-dialect builds.** They need `DATABASE_URL` (or a `.sqlx` offline cache) at `cargo build` time tied to one schema. A CI machine building against Postgres would fail to compile MySQL-specific code paths checked the same way. Stay on runtime `query()`/`query_as::<_, T>()` for all three dialects (already the codebase's existing convention in `ping()`).
2. **SQLite parent directory must exist before connecting**, even with `create_if_missing(true)` — that flag only creates the *file*. Forgetting this breaks D-18 and `make up-sqlite` on a fresh checkout where `./var/` doesn't exist yet.
3. **`PRAGMA foreign_keys=ON` is per-connection, not persisted in the SQLite file.** If any code path opens a raw `SqliteConnection` without going through the configured `SqliteConnectOptions` (e.g. a future admin tool), foreign keys silently revert to OFF. Keep exactly one connection-options constructor.
4. **Placeholder syntax differs per dialect** ($1/$2 Postgres, `?` MySQL, `?`/`?1` SQLite) — a hand-copied SQL string moved between dialect files without updating placeholders will fail at runtime, not compile time (since macros aren't used). The structural-parity test (see Validation Architecture) should also sanity-check statement counts/shapes, but exact placeholder correctness is only caught by the per-dialect integration test actually running the query.
5. **Upsert syntax differs**: `ON CONFLICT ... DO UPDATE` (Postgres/SQLite) vs `ON DUPLICATE KEY UPDATE` (MySQL) — different column-reference syntax in the `SET` clause too (`excluded.col` / `instances.col` vs `VALUES(col)`). This is the most likely spot for a copy-paste dialect bug.
6. **MySQL's `mysql://` driver in sqlx also accepts `mariadb://`** — not relevant to Oxidean's URL contract, but don't accidentally document/accept `mariadb://` as a fourth dialect; D-01 only lists three.
7. **GitHub Actions `services:` block cannot be conditioned on `matrix.*`.** Don't spend planning time trying to make the mysql/postgres service definitions "only start for their leg" — either accept both booting every leg, or split into three jobs (see §8).
8. **`docker compose` overlay `depends_on` doesn't deep-merge lists cleanly across files** in every Compose version/edge-case — verify the exact `up-sqlite` overlay behavior with `docker compose -f docker-compose.yml -f docker-compose.sqlite.yml config` before trusting it; this is flagged as a planning-time detail to verify hands-on, not a solved recipe (see Architecture Patterns §7).
9. **Compose smoke script currently only calls `system.health` and greps for `"ok":true`.** Extending it for the db_probe round-trip (D-13) needs a second curl + a grep/jq check on the `dialect` field in the response to actually prove which dialect was hit, not just that *a* DB responded.
10. **`.env.example`'s current SQLite line is a stale Phase-1 placeholder** (`DATABASE_URL=sqlite:./data/oxidean.db`, using `./data` not `./var`, with a comment saying "full SQLite proof is Phase 2") — this must be corrected as part of Phase 2, not left as-is. [VERIFIED: .env.example current content]
11. **`rust:1-bookworm` build stage compiles all three sqlx drivers into one binary** (needed since dialect is chosen at runtime) — expect a real increase in `cargo build --release` time and binary size versus Phase 1's Postgres-only build; not a correctness bug, but worth expecting so CI timing isn't mistaken for a hang.

---

## Code Examples

### `oxidean-db` Cargo.toml (target state)

```toml
[package]
name = "oxidean-db"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
oxidean-core = { path = "../oxidean-core" }
sqlx = { version = "0.8", default-features = false, features = [
  "runtime-tokio", "tls-rustls",
  "postgres", "mysql", "sqlite",
  "migrate", "chrono",
] }
```

### `system.db_probe` response shape (`oxidean-core`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbProbeResponse {
    pub dialect: String,     // "postgres" | "mysql" | "sqlite"
    pub probe_count: i64,
    pub probed_at: String,   // ISO-8601 text, dialect-normalized on the way out
}
```

### Compose smoke extension (illustrative addition to `scripts/compose-smoke.sh`)

```bash
echo "==> RPC system.db_probe"
curl -fsS \
  -H 'Content-Type: application/json' \
  -H 'Oxidean-RPC-Version: 1' \
  -d '{"procedure":"system.db_probe","input":{}}' \
  "$BASE_URL/api/rpc" | tee /tmp/oxidean-smoke-probe.json
grep -q "\"dialect\"" /tmp/oxidean-smoke-probe.json
```

---

## Validation Architecture

Phase 2's success criteria are all "works identically across three dialects," so validation must be **per-dialect**, not just per-feature. Structure for the eventual `VALIDATION.md`:

### Layer 1 — Unit tests (no DB required, run everywhere)
- `Dialect::from_url` / `resolve_dialect`: table-driven tests covering all valid schemes (`postgres://`, `postgresql://`, `mysql://`, `sqlite:`, `sqlite://`), the D-02 mismatch-fails-fast case (`OXIDEAN_DB_DIALECT` disagreeing with URL scheme), and unrecognized-scheme rejection.
- Migration structural-parity check: a test that lists files in `migrations/postgres/`, `migrations/mysql/`, `migrations/sqlite/` and asserts matching filename sets (same migration steps present in all three) — catches "added a migration to one dialect, forgot the others" without needing a live DB.

### Layer 2 — Per-dialect integration tests (require a live DB; gated by `DATABASE_URL`)
- `crates/oxidean-db/tests/dialect_probe.rs` (or similar): connects using `DATABASE_URL` from env, runs `run_migrations()`, calls `probe()` twice, asserts `probe_count` increments and `dialect` in the row matches the connected dialect.
- These tests should **skip gracefully** (not fail) when `DATABASE_URL` is unset — matching the existing `Database::skipped()` philosophy — so local `cargo test --workspace` without Docker running still passes. CI's `db-matrix` job is what actually exercises all three dialects (Layer 2 is meaningless without CI setting `DATABASE_URL` per leg).
- Explicit assertion of D-18 (SQLite parent dir creation): a test that points `DATABASE_URL` at a nested nonexistent path under a tempdir and asserts connect succeeds and the file exists afterward.

### Layer 3 — CI matrix (D-15, PLAT-09 adjacency)
- One `db-matrix` GitHub Actions job (or three discrete jobs — planner's choice, see Architecture Patterns §8) proving migrate + probe round-trip on Postgres, MySQL, and SQLite on every PR.
- This is the primary mechanized proof of ROADMAP success criteria #2 ("Migrations apply cleanly on each supported dialect").

### Layer 4 — Compose/Make smoke (D-13, ROADMAP success criterion #3)
- Extend `scripts/compose-smoke.sh` (default Postgres profile already exercised) with the `system.db_probe` RPC call + dialect assertion (see Code Examples).
- Add equivalent smoke invocations for `make up-mysql` and `make up-sqlite` overlays — either as additional CI jobs (requires Docker-in-Docker on the runner, heavier) or documented as a manual/local-only proof step if CI compose bring-up isn't in scope until Phase 22. **Planning decision needed:** whether Phase 2 CI includes live Compose bring-up smoke for all three profiles, or whether that's explicitly deferred to Phase 22 (PLAT-03/PLAT-09) and Phase 2 CI stops at Layer 3 (`cargo test` matrix) plus `docker compose config` validation for the new `up-sqlite` overlay (mirroring the existing `compose` job's `config`-only check for the mysql overlay). Given D-15's wording ("CI proves all three dialects in a matrix") is satisfiable by Layer 3 alone, and the existing `compose` CI job is explicitly config-validation-only (not bring-up) even for the already-shipped MySQL overlay, **recommend**: Layer 3 (`cargo test` matrix) is the CI proof for D-15; extend the existing `compose` job's config-validation to include the new `docker-compose.sqlite.yml` overlay; leave live three-profile Compose bring-up smoke as a local/manual `make smoke` / `make smoke-mysql` / `make smoke-sqlite` capability (D-13's "Compose/Make smoke" satisfied via Make, not necessarily via CI) rather than adding Docker-in-Docker bring-up jobs to CI in this phase.

### What "done" looks like for VALIDATION.md gates
| Success criterion | Validation layer | Command |
|---|---|---|
| Operator can configure dialect via config/env | Layer 1 | `cargo test -p oxidean-db resolve_dialect` |
| Migrations apply cleanly on each dialect | Layer 2 + 3 | CI `db-matrix` job (all 3 legs green) |
| Core write/read flow succeeds per dialect in local Compose | Layer 4 | `make smoke`, `make smoke-mysql` (new), `make smoke-sqlite` (new) — locally invokable, not required in CI for this phase |
| Dialect/URL mismatch fails fast (D-02) | Layer 1 | `cargo test -p oxidean-db resolve_dialect_mismatch` |
| SQLite parent dir auto-create (D-18) | Layer 2 | `cargo test -p oxidean-db sqlite_creates_parent_dir` |

---

## Open Items for Planning (not blockers, but decide explicitly in PLAN.md)

1. Compose `depends_on`/profile encoding for `make up-sqlite` removing the Postgres dependency cleanly (Pitfall #8) — verify with `docker compose config` during planning/execution, don't assume the naive overlay works first try.
2. CI matrix encoding: one job with `strategy.matrix` + both services always booted, vs. three discrete jobs (Architecture Patterns §8) — either satisfies D-15; pick one for consistency with the existing `ci.yml` job-per-concern style.
3. Whether live three-profile Compose bring-up smoke belongs in CI now or stays Make-only until Phase 22 (Validation Architecture Layer 4) — recommendation given above, but confirm against CI runtime/complexity budget for this phase.
4. Exact `instances` table column list beyond the D-09/D-10 minimum (this research proposes `id, dialect, probe_count, probed_at`) — fine to lock in PLAN.md.

---

*Research completed: 2026-09-09*
