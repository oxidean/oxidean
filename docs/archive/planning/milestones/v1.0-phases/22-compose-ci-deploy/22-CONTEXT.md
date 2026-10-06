# Phase 22: Compose CI & Cloud Deploy — Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss:** Auto-decided (campaign defaults — CI Compose matrix Postgres+SQLite minimum with MySQL in CI; Railway-class same-image cloud path; reuse Compose/Make/CI)

---

<domain>
## Phase Boundary

Every PR proves Compose health across supported databases, and the same images deploy as Oxidean Cloud on a Railway-class container host.

**Requirements:** PLAT-02, PLAT-03, PLAT-09

**Depends on:** Phase 2 (multi-DB), Phase 19 (Actions), Phase 20 (packages), Phase 21 (social/explore) — **plan now; execute after those land** (parallel track).

**Success criteria (from ROADMAP):**
1. Project CI builds and validates Docker Compose (bring-up health) on every PR
2. Project CI exercises at least PostgreSQL and SQLite; MySQL is in CI or covered by an explicit compatibility test job
3. Operator can deploy the same images/stack to a container host (e.g. Railway) as Oxidean Cloud

**Out of scope (do not invent):**
- Automatic CD from GitHub Actions → Railway with production secrets on fork PRs
- Multi-region HA / geo-replicated storage
- Replacing local Compose Traefik with a different local ingress
- Hosted Oxidean Cloud on MySQL or SQLite (cloud default remains managed Postgres; MySQL/SQLite stay self-host/CI dialects)
- Publishing images to a public registry as a mandatory gate (Compose `--build` in CI is sufficient for PLAT-03)

</domain>

<decisions>
## Implementation Decisions

### A — CI Compose bring-up (PLAT-03 / PLAT-09)

- **D-CI-01:** Add a PR CI job that **builds and brings up** Compose (not config-only). Reuse `scripts/compose-smoke.sh` via existing Make targets (`make smoke`, `make smoke-sqlite`, `make smoke-mysql`). Keep the existing `compose` config-validation job (cheap signal). — **Reversibility:** reversible
- **D-CI-02:** Compose bring-up matrix **must** include **Postgres** and **SQLite** on every PR (PLAT-09 minimum). — **Reversibility:** reversible
- **D-CI-03:** **MySQL Compose bring-up** runs in the **same PR workflow** as a third matrix leg (or sibling job with `fail-fast: false`) via `make smoke-mysql` — not deferred to nightly. Existing `db-matrix` dialect_probe job stays; it does **not** replace Compose bring-up. — **Reversibility:** reversible
- **D-CI-04:** Fail-closed under `CI=true` / Docker-missing (same discipline as `smoke-protocol`). Upload Compose logs/artifacts on failure. — **Reversibility:** reversible
- **D-CI-05:** Keep `smoke-protocol` as a **separate** job (forge protocol routing). Compose-smoke proves dialect health + Traefik `/` + `/health` + `system.db_probe`; do not merge into one mega-job. — **Reversibility:** reversible
- **D-CI-06:** Image proof = `docker compose … up --build` inside smoke (existing path). No mandatory GHCR/Docker Hub push in this phase. — **Reversibility:** reversible

### B — Oxidean Cloud / Railway-class deploy (PLAT-02)

- **D-CLOUD-01:** Oxidean Cloud deploys the **same Dockerfiles** already used by Compose: `crates/oxidean-api/Dockerfile` and `apps/web/Dockerfile` (repo-root build context). Railway (or equiv.) services use **DOCKERFILE** builder — not a second app architecture / Railpack-from-source fork. — **Reversibility:** costly — cloud release train
- **D-CLOUD-02:** Hosted cloud **default database = managed Postgres** (Railway Postgres plugin or equivalent). MySQL/SQLite remain supported for self-host/CI, not the Oxidean Cloud default. — **Reversibility:** reversible
- **D-CLOUD-03:** Local Compose keeps **Traefik + Docker provider**. Cloud ingress uses a **file-configured gateway** (Caddy preferred; Traefik file provider acceptable) under `deploy/cloud/` that mirrors Compose path priorities (git `.git`, packages `/v2|/npm|/generic`, `/api`/`/uploads`/`/health`, then SPA). Do **not** require a Docker socket on the cloud host. — **Reversibility:** costly — ingress contract
- **D-CLOUD-04:** Persist forge volumes on the cloud host: repos, LFS, packages, release-assets, uploads, SSH host keys (map to Railway volumes / equivalent mounts). — **Reversibility:** costly — ops/backup layout
- **D-CLOUD-05:** Author cloud topology as Railway **IaC** in `.railway/railway.ts` (TypeScript). Do **not** add deprecated `railway.json` / `railway.toml`. Secrets stay out of git (`preserve()` / dashboard vars). — **Reversibility:** costly — platform lock-in of IaC shape
- **D-CLOUD-06:** Cloud env defaults documented: `OXIDEAN_ENV=production`, required `OXIDEAN_CORS_ORIGINS` + `OXIDEAN_PUBLIC_ORIGIN`, `OXIDEAN_ALLOW_SIGNUP=true` for open cloud signup, prefer `OXIDEAN_AUTO_MIGRATE=false` with explicit migrate/preDeploy. — **Reversibility:** reversible
- **D-CLOUD-07:** Phase 22 delivers **IaC + operator docs + Make helpers**; live `railway config apply` / first production project is a **human operator action** (blocking checkpoint when executing). No production Railway token in PR CI. — **Reversibility:** reversible
- **D-CLOUD-08:** Git-over-SSH: document Railway (or host) **TCP publish** for `OXIDEAN_SSH_PORT` when available; HTTPS Smart HTTP remains the always-on cloud git path. — **Reversibility:** reversible

### Claude's Discretion

- Exact CI matrix YAML shape (single `strategy.matrix` vs three jobs) as long as D-CI-02/D-CI-03 hold
- Whether gateway is Caddy or Traefik-file; image tag pins for gateway
- Whether `.railway/` gets a nested `package.json` for the `railway` IaC SDK vs root workspace add
- Exact Make target names (`cloud-plan`, `cloud-docs`, etc.)
- How aggressively to cache Docker layers in GHA for compose-smoke

</decisions>

<specifics>
## Specific Ideas

- Prefer extending `.github/workflows/ci.yml` and existing Make/smoke scripts over new workflow files
- Update `docs/DEPLOYMENT.md` VERIFY placeholders with concrete Railway-class steps once IaC exists
- Update `docs/TESTING.md` CI table so compose bring-up matrix is documented next to config-only + smoke-protocol + db-matrix
- Campaign note: **execute after** Phases 19–21 land on the integration branch; planning is intentionally ahead

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements & roadmap
- `.planning/ROADMAP.md` — Phase 22 goal, success criteria, deps (2, 19, 20, 21)
- `.planning/REQUIREMENTS.md` — PLAT-02, PLAT-03, PLAT-09
- `.planning/PROJECT.md` — dual-mode Docker Compose; Railway-class cloud
- `.planning/parallel-tracks/phase-22.md` — plan-now / execute-later rules

### Prior phase decisions
- `.planning/phases/01-monorepo-scaffold/01-CONTEXT.md` — D-06–D-09 Compose + Traefik
- `.planning/phases/02-multi-db-storage/02-CONTEXT.md` — dialect overlays, smoke targets
- `.planning/phases/11.1-quality-hardening/11.1-CONTEXT.md` — D-QH-04 smoke-protocol CI discipline

### Implementation touchpoints
- `.github/workflows/ci.yml` — existing `compose`, `smoke-protocol`, `db-matrix` jobs
- `docker-compose.yml`, `docker-compose.mysql.yml`, `docker-compose.sqlite.yml`
- `Makefile` — `smoke` / `smoke-mysql` / `smoke-sqlite` / `smoke-protocol-ci`
- `scripts/compose-smoke.sh`, `scripts/ci-smoke-protocol.sh`
- `crates/oxidean-api/Dockerfile`, `apps/web/Dockerfile`
- `docs/DEPLOYMENT.md`, `docs/CONFIGURATION.md`, `docs/TESTING.md`
- `.agents/skills/use-railway/SKILL.md` + `references/iac.md` + `references/deploy.md`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable assets
- `scripts/compose-smoke.sh` — full bring-up + Traefik health + `system.db_probe` (dialect assert)
- Make targets already encode Postgres / MySQL / SQLite smoke recipes
- CI already builds images indirectly via `smoke-protocol` (Postgres Compose only)
- `db-matrix` proves migrations/dialect_probe on all three dialects without full Compose UI

### Gaps this phase closes
- `compose` CI job is **config-only** — does not satisfy PLAT-03 bring-up health
- No CI Compose bring-up for **SQLite** or **MySQL** overlays (PLAT-09)
- No `railway` / cloud IaC; `docs/DEPLOYMENT.md` still has VERIFY stubs for Railway

### Established patterns
- Fail-closed smoke under `CI` / `SMOKE_REQUIRE_STACK`
- Dialect overlays via Compose files + profiles (not separate product forks)
- One product path for cloud and self-host (env flags, not code forks)

</code_context>

<deferred>
## Deferred Ideas

- GitHub Actions → Railway automatic deploy with environment protection rules
- Multi-region replicas / HA Postgres
- Public image registry publish + signing
- Replacing Compose Traefik locally
- Hosted cloud on MySQL or SQLite

</deferred>
