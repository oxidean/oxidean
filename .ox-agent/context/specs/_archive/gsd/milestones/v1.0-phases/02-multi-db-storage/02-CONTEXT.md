# Phase 2: Multi-DB Storage - Context

**Gathered:** 2026-09-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Operators can choose SQLite, PostgreSQL, or MySQL for app data with working migrations and core write/read flows on all three.

**Requirements:** PLAT-07, PLAT-08

**Success criteria (from ROADMAP):**
1. Operator can configure the instance dialect to SQLite, PostgreSQL, or MySQL via config/env
2. Migrations apply cleanly on each supported dialect
3. A core app write/read flow succeeds against each dialect in a local Compose setup

**Carried forward from Phase 1 (not re-opened):**
- D-06: Default Compose stack uses Postgres
- D-07: MySQL via Compose profile; SQLite via env/file (no DB container)
- D-08: `oxidean-db` is the uniform adapter boundary

</domain>

<decisions>
## Implementation Decisions

### A — Dialect selection
- **D-01:** Prefer inferring dialect from `DATABASE_URL` scheme (`postgres://` / `postgresql://`, `mysql://`, `sqlite:` / `sqlite://`); optional `OXIDEAN_DB_DIALECT=postgres|mysql|sqlite` must agree with the URL when set
- **D-02:** Dialect/URL mismatch fails fast at startup with a clear operator-facing error
- **D-03:** Dialect switch is supported in Phase 2 only for an **empty** target DB, with a documented recipe (and Make helper — see D-14)
- **D-04:** Canonical operator docs: `.env.example` + README + `make` help targets that print sample URLs + `docs/database.md`

### B — Migration approach
- **D-05:** Shared **logical** migrations adapted per dialect via a **thin** adapter (types / autoincrement / quoting) — not a full migration DSL
- **D-06:** Materialize/adapt into **sqlx-compatible** per-dialect migration sets; use sqlx migrator bookkeeping (`_sqlx_migrations`)
- **D-07:** `OXIDEAN_AUTO_MIGRATE=true` by default in Compose/dev (`.env.example`); when false (prod-like), require explicit `make db-migrate` / CLI
- **D-08:** Keep Phase 2 schema trivial so portability stays easy

### C — Proof entity & diagnostic surface
- **D-09:** Proof entity is an early **product-ish** table (e.g. `instances` or `settings`) that later phases may keep — not a disposable `smoke_kv`-only table
- **D-10:** Schema includes a **dialect/version stamp** so operators can see which dialect the write hit
- **D-11:** Keep as a **supported diagnostic** long-term
- **D-12:** Shared Rust diagnostic write/read in `oxidean-db` / core; expose via minimal RPC (e.g. `system.db_probe`) so Compose smoke, CI, and a future system-admin diagnostics menu share one path — **no Phase 2 diagnostics UI**
- **D-13:** Phase 2 proves the flow via cargo integration tests + Compose/Make smoke calling that diagnostic (RPC and/or Rust path)

### D — Operator paths
- **D-14:** Explicit Make targets: `make up`, `make up-mysql`, `make up-sqlite` (or equivalent)
- **D-15:** CI proves **all three** dialects in a **matrix** in Phase 2
- **D-16:** Empty-DB dialect switch: `make db-switch-dialect` (refuse unless empty / `--force-empty`) **plus** steps in `docs/database.md`

### E — SQLite specifics
- **D-17:** Default file path: `./var/oxidean.db` (runtime-state style; gitignore `var/`)
- **D-18:** API creates missing parent directories on startup when dialect is SQLite
- **D-19:** `make up-sqlite`: no DB container; api+web(+Traefik) with SQLite file **bind-mounted** from host `./var` (align mount with D-17; not `./data`)
- **D-20:** Sensible SQLite defaults only (`WAL`, `foreign_keys=ON`) documented; no deep tuning in Phase 2

### Claude's Discretion
- Exact table name (`instances` vs `settings` vs similar) and column names within D-09–D-10
- Exact RPC procedure name/shape under `system.*` as long as D-12 holds
- Thin logical→dialect adaptation mechanism (templates vs small Rust rewriter) as long as D-05–D-06 hold
- Whether smoke hits RPC through Traefik or calls a small CLI that shares the same Rust diagnostic
- sqlx feature-flag layout and Cargo workspace wiring for three dialects
- Auth gate for `system.db_probe` may remain open/local in Phase 2; admin-only enforcement lands with system dashboard

</decisions>

<specifics>
## Specific Ideas

- Future **system dashboard → system diagnostics** menu: system-admin triggers the same DB read/write diagnostic (Phase 2 leaves the capability; UI is later)
- Operator docs should make switching dialects feel intentional (empty DB), not accidental URL edits mid-flight
- Sample URLs should be printable from Make help for copy-paste local proof

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements & roadmap
- `.planning/REQUIREMENTS.md` — PLAT-07, PLAT-08 (and PLAT-09 adjacency for CI dialects)
- `.planning/ROADMAP.md` — Phase 2 goal and success criteria
- `.planning/PROJECT.md` — multi-DB product decision

### Prior phase decisions
- `.planning/phases/01-monorepo-scaffold/01-CONTEXT.md` — D-02, D-06–D-08 (adapter + Compose dialect topology)
- `.planning/phases/01-monorepo-scaffold/01-04-SUMMARY.md` — existing MySQL profile / SQLite docs stubs

### Existing implementation touchpoints
- `crates/oxidean-db/` — current Postgres-only `Database` + `ping()`
- `crates/oxidean-api/` — RPC mount, health, env wiring
- `docker-compose*.yml` / Makefile / `scripts/compose-smoke.sh` — operator and smoke paths
- `.env.example` / README — current DB URL documentation

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `oxidean-db::Database` — uniform adapter entry; extend for multi-dialect connect, migrate, diagnostic write/read
- `system.health` / `system.echo` RPC + codegen pipeline — pattern for adding `system.db_probe`
- Compose postgres default + MySQL profile from Phase 1 — extend with `up-sqlite` and matrix CI
- `scripts/compose-smoke.sh` — extend or companion script for dialect round-trip

### Established Patterns
- sqlx behind `oxidean-db`; API must not dialect-branch ad hoc
- Dotted `system.*` RPC namespaces; header `Oxidean-RPC-Version: 1`
- Traefik ingress for Compose; Vite proxy for `make dev`

### Integration Points
- API startup: dialect resolve (D-01/D-02), optional auto-migrate (D-07), SQLite dir create (D-18)
- CI: expand beyond current gates to three-dialect matrix (D-15)
- Docs: new `docs/database.md` plus README / `.env.example` / Make help (D-04)

</code_context>

<deferred>
## Deferred Ideas

- **System dashboard / system diagnostics UI** — admin menu to trigger DB read/write test (capability in Phase 2; UI later)
- **Auth-gating** `system.db_probe` to system-admin only — with real admin/roles phases
- **Non-empty dialect migration / data portability** between SQLite ↔ Postgres ↔ MySQL — out of Phase 2 (empty DB only)
- **Deep SQLite tuning** and multi-replica guidance beyond single-writer common sense — later ops hardening
- **PLAT-09** wording already expects PG+SQLite minimum; Phase 2 decision D-15 goes further (all three in matrix) — keep REQUIREMENTS/roadmap aligned when planning

</deferred>

---

*Phase: 02-multi-db-storage*
*Context gathered: 2026-09-09*
