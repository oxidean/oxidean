# Phase 22: Compose CI & Cloud Deploy — Research

**Researched:** 2026-09-16  
**Domain:** GitHub Actions Compose bring-up matrix + Railway-class same-image cloud deploy  
**Confidence:** HIGH (in-repo patterns verified; Railway IaC from current skill docs)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-CI-01…06:** PR CI Compose bring-up via `compose-smoke.sh` / Make; Postgres+SQLite required; MySQL in same PR workflow; fail-closed; keep smoke-protocol separate; no mandatory registry push
- **D-CLOUD-01…08:** Same Dockerfiles; managed Postgres for cloud; file-config gateway (not Docker-socket Traefik); forge volumes; `.railway/railway.ts` IaC; production env docs; human apply; SSH TCP documented

### Deferred (OUT OF SCOPE)
- Auto CD with prod secrets on fork PRs; multi-region HA; public registry signing; replacing local Traefik; hosted cloud on MySQL/SQLite

</user_constraints>

<architectural_responsibility_map>
## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Compose config validation | CI (GHA) | Compose files | Cheap PR signal; keep existing job |
| Compose bring-up + dialect health | CI + Make/scripts | api/web/Traefik containers | PLAT-03/09; reuse compose-smoke |
| Dialect unit probe (migrate) | CI `db-matrix` | `oxidean-db` | Already lands; complements Compose smoke |
| Protocol edge smokes | CI `smoke-protocol` | Traefik labels + SSH TCP | Keep separate (D-CI-05) |
| Cloud service topology | Ops / Railway IaC | Docker images | PLAT-02; same images |
| Cloud HTTP ingress | `deploy/cloud` gateway | api + web private hosts | No Docker socket on Railway |
| Forge persistent volumes | Cloud volumes / Compose binds | api process paths | repos/lfs/packages/… |
| Operator docs | `docs/DEPLOYMENT.md` | CONFIGURATION / TESTING | VERIFY stubs → concrete steps |

</architectural_responsibility_map>

<research_summary>
## Summary

Phase 1/2/11.1 already shipped the **operator** Compose path and **partial** CI:

| Existing | Covers | Gap vs Phase 22 |
|----------|--------|-----------------|
| `compose` job | `docker compose … config` | No build/bring-up (PLAT-03) |
| `smoke-protocol` | Postgres Compose up + git/packages routing | No `system.db_probe` dialect matrix; no SQLite/MySQL overlays |
| `db-matrix` | dialect_probe on PG/MySQL/SQLite service containers | Not full Compose UI/Traefik bring-up |
| `make smoke*` | Full bring-up health for all three dialects | **Not wired into CI** |

**Primary recommendation:** Add `compose-smoke` GHA matrix calling existing Make targets; author Railway IaC + Caddy (or Traefik-file) gateway that reuses Compose path priorities; document operator apply without putting Railway tokens in PR CI.

**Critical cloud finding:** Compose Traefik uses `--providers.docker=true` + socket mount. Railway (and most PaaS) will not expose a Docker socket. Cloud must use **file-based reverse proxy** (D-CLOUD-03) in front of private `api`/`web` services while still running the **same application images**.

</research_summary>

<current_state>
## Current State (verified in worktree)

### CI (`.github/workflows/ci.yml`)
- Triggers: `push` to `main`, all `pull_request`
- Jobs include `compose` (config), `smoke-protocol` (Postgres bring-up + protocol), `db-matrix` (three dialects, probe only)
- No job runs `make smoke` / `smoke-sqlite` / `smoke-mysql`

### Compose / Make
- Default: Traefik + web + api + postgres (`docker-compose.yml`)
- Overlays: `docker-compose.mysql.yml` (profile `mysql`), `docker-compose.sqlite.yml` (no DB container; bind `OXIDEAN_SQLITE_HOST_DIR`)
- Smoke: `scripts/compose-smoke.sh` — `up --build -d --wait`, curl `/`, `/health`, RPC `system.health` + `system.db_probe` with `EXPECT_DIALECT`

### Docker images
- API: multi-stage Rust release binary, health on `:8080/health`
- Web: Bun build + `preview` on `:3000`

### Cloud docs
- `docs/DEPLOYMENT.md` states cloud should reuse images; VERIFY comments still open for Railway URL/mapping/TLS

</current_state>

<standard_stack>
## Standard Stack

| Piece | Choice | Why | Confidence |
|-------|--------|-----|------------|
| CI runner | `ubuntu-latest` + Docker | Existing workflow | HIGH [VERIFIED: ci.yml] |
| Bring-up proof | `scripts/compose-smoke.sh` | Already PLAT-01/08 path | HIGH [VERIFIED] |
| Cloud platform | Railway (IaC TS) | PROJECT + PLAT-02 example; skill current | HIGH [CITED: use-railway iac.md] |
| Cloud builder | DOCKERFILE | Same images as Compose (D-CLOUD-01) | HIGH |
| Cloud DB | Railway Postgres | D-CLOUD-02 | HIGH |
| Cloud gateway | Caddy 2 (file Caddyfile) | Simple path rules; no Docker provider | MEDIUM [ASSUMED] — Traefik file provider also OK per discretion |
| IaC SDK | `railway` npm (nested `.railway/`) | Required for `.railway/railway.ts` | HIGH [CITED: iac.md] |

</standard_stack>

## Package Legitimacy Audit

| Package | Registry | Purpose | Status | Notes |
|---------|----------|---------|--------|-------|
| `railway` | npm | Railway TypeScript IaC authoring SDK | [ASSUMED] | Official Railway package per skill (`npm install railway`). Executor must verify on npmjs.com/package/railway before install; human checkpoint if still ASSUMED/SUS at execute time. |
| `caddy` (container image) | Docker Hub `caddy` | Cloud gateway | [ASSUMED] | Pin a known tag (e.g. `caddy:2-alpine`); verify image publisher at execute. |

No other new npm/pip/cargo runtime deps required for Compose CI work.

<dont_hand_roll>
## Don't Hand-Roll

- New smoke shell that duplicates `compose-smoke.sh` dialect asserts
- Second pair of Dockerfiles for “cloud”
- `railway.json` / `railway.toml` (deprecated; cutoff 2026-12-01 per skill)
- Docker-socket Traefik on Railway
- Folding protocol smokes into compose-smoke matrix (D-CI-05)

</dont_hand_roll>

<common_pitfalls>
## Common Pitfalls

1. **GHA disk/time:** Three full `docker compose up --build` legs are heavy — use `fail-fast: false`, free disk step like `api-rust`, consider shared layer cache; do not drop MySQL from PR to “save time” (locked D-CI-03).
2. **SQLite host path on CI:** `OXIDEAN_SQLITE_HOST_DIR` / `.env.sqlite` — use Linux-native `./var` on ubuntu-latest; avoid WSL `docker.exe` helpers in GHA.
3. **Traefik Host(`localhost`):** Smoke uses `http://localhost` — keep that for CI; cloud gateway must use public Host / catch-all, not localhost labels.
4. **CORS / PUBLIC_ORIGIN:** Production cloud must set allowlist + public origin or browsers break SSO/magic links.
5. **Fork PRs + secrets:** Never put `RAILWAY_TOKEN` on pull_request from forks; IaC validate statically / optional manual workflow.
6. **SSH on PaaS:** TCP proxy is optional; document HTTPS git as default cloud clone path.

</common_pitfalls>

<code_reuse>
## Code Reuse Map

| Need | Reuse |
|------|-------|
| Bring-up health | `scripts/compose-smoke.sh`, `make smoke*` |
| Protocol CI | leave `scripts/ci-smoke-protocol.sh` alone |
| Path priority rules | Copy semantics from `docker-compose.yml` Traefik labels into `deploy/cloud/Caddyfile` (or Traefik dynamic file) |
| Env semantics | `docs/CONFIGURATION.md`, `.env.example` |
| Fail-closed Docker | `scripts/smoke-lib.sh` patterns |

</code_reuse>

<verification_approach>
## Verification Approach

- **PLAT-03:** GHA `compose-smoke` job green on PR for Postgres (at minimum one leg proves build+health)
- **PLAT-09:** Same job matrix includes sqlite + mysql legs (or sibling jobs) green
- **PLAT-02:** `.railway/railway.ts` + `deploy/cloud/*` + DEPLOYMENT.md describe deploy of existing Dockerfiles; optional `railway config plan` when linked; human apply checkpoint at execute
- Local: `make smoke && make smoke-sqlite && make smoke-mysql` still work

</verification_approach>
