---
phase: 22
slug: compose-ci-deploy
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-09-16
---

# Phase 22 — Validation Strategy

> Requirement → verify map for Compose CI bring-up + Railway-class cloud deploy (PLAT-02 / PLAT-03 / PLAT-09).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Make smoke scripts + GitHub Actions `ci.yml` |
| **Compose bring-up** | `scripts/compose-smoke.sh` via `make smoke` / `smoke-sqlite` / `smoke-mysql` |
| **CI wrapper** | `scripts/ci-compose-smoke.sh` + job `compose-smoke` |
| **Cloud** | Static presence of `.railway/railway.ts` + `deploy/cloud/` (live apply = human) |
| **Quick run** | `rg compose-smoke .github/workflows/ci.yml`; `test -f .railway/railway.ts` |
| **Full local** | `make smoke && make smoke-sqlite && make smoke-mysql` (needs Docker) |

---

## Sampling Rate

- **After plan 01 tasks:** Workflow YAML + TESTING.md CI table
- **After plan 02 tasks:** IaC/gateway file presence; no `railway.json`
- **Before `/gsd-verify-work`:** VALIDATION rows green or env-blocked with evidence
- **Live Railway apply:** Never claimed green by CI — human-verify only (D-CLOUD-07)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|---------|-----------------|-----------|-------------------|-------------|--------|
| 22-01-01 | 01 | 1 | PLAT-03 | T-22-01/03 | Postgres Compose bring-up in PR CI; fail-closed | ci / smoke | `rg -n "compose-smoke" .github/workflows/ci.yml`; `./scripts/ci-compose-smoke.sh postgres` | ✅ | ✅ green (workflow wired) |
| 22-01-02 | 01 | 1 | PLAT-09 | T-22-02 | SQLite Compose bring-up matrix leg | ci | `rg -n "sqlite" .github/workflows/ci.yml` (compose-smoke matrix) | ✅ | ✅ green |
| 22-01-03 | 01 | 1 | PLAT-09 | T-22-02 | MySQL Compose bring-up matrix leg | ci | matrix includes `mysql`; `make smoke-mysql` | ✅ | ✅ green |
| 22-02-01 | 02 | 2 | PLAT-02 | T-22-SC | Package legitimacy (`railway`, `caddy`) | human | npmjs + Docker Hub Official Image | ✅ | ✅ approved |
| 22-02-02 | 02 | 2 | PLAT-02 | T-22-10/12 | IaC + Caddy gateway; no secrets; no railway.json | file | `test -f .railway/railway.ts && test -f deploy/cloud/Caddyfile && test ! -f railway.json` | ✅ | ✅ green |
| 22-02-03 | 02 | 2 | PLAT-02 | T-22-10/13 | Volumes + DEPLOYMENT operator steps + cloud-plan | file / docs | `rg -n "cloud-plan|forge-data|OXIDEAN_ALLOW_SIGNUP" Makefile docs/DEPLOYMENT.md .railway/railway.ts` | ✅ | ✅ green |
| 22-03-01 | 03 | 3 | PLAT-* | T-22-21 | This VALIDATION map | docs | `rg -n "PLAT-02\|PLAT-03\|PLAT-09" 22-VALIDATION.md` | ✅ | ✅ green |
| 22-03-02 | 03 | 3 | PLAT-* | T-22-20 | ARCHITECTURE / TESTING / DEPLOYMENT sync | docs | `rg -n "deploy/cloud\|\\.railway\|compose-smoke" docs/` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky / env-blocked*

---

## Requirement Coverage

| Requirement | Truth | Automated evidence | Human evidence | Status |
|-------------|-------|--------------------|----------------|--------|
| **PLAT-03** | PR CI builds/brings up Compose and asserts health | GHA `compose-smoke` (postgres) → `make smoke`; local `make smoke` | Optional: open CI run artifacts on failure | COVERED |
| **PLAT-09** | CI exercises Postgres + SQLite; MySQL in same PR workflow | `compose-smoke` matrix `[postgres, sqlite, mysql]`; `db-matrix` remains complementary (probe-only, not a substitute) | — | COVERED |
| **PLAT-02** | Operator can deploy same images to Railway-class host | Files: `.railway/railway.ts`, `deploy/cloud/*`, `make cloud-plan`, `docs/DEPLOYMENT.md` Oxidean Cloud section | **Live** `railway config apply` + domain + first migrate — operator only (D-CLOUD-07) | COVERED (IaC/docs); live apply = human-verify |

---

## Complementary (not substitutes)

| Job / target | Proves | Does **not** replace |
|--------------|--------|----------------------|
| `compose` (config-only) | Compose file validity | Bring-up health (PLAT-03) |
| `smoke-protocol` | Git/packages/SSH routing (D-QH-04) | Dialect `db_probe` matrix (D-CI-05) |
| `db-matrix` | Migrations + `dialect_probe` on service containers | Full Traefik UI bring-up |

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions | UAT |
|----------|-------------|------------|-------------------|-----|
| First production Railway apply | PLAT-02 | Needs `RAILWAY_TOKEN` + linked project; must not run on fork PRs | `make cloud-plan` → review → `railway config apply` → hit public `/health` | ⬜ operator |
| Custom domain + CORS/PUBLIC_ORIGIN | PLAT-02 | Dashboard secrets | Set origins to `https://<domain>`; reload SPA | ⬜ operator |
| Optional SSH TCP publish | PLAT-02 / D-CLOUD-08 | Platform TCP proxy availability | Confirm `OXIDEAN_SSH_PORT` TCP; HTTPS git still works without it | ⬜ optional |

---

## Wave / plan ownership

| Wave | Plan | Delivers |
|------|------|----------|
| 1 | 22-01 | `compose-smoke` CI matrix + TESTING.md |
| 2 | 22-02 | `.railway/` + `deploy/cloud/` + DEPLOYMENT cloud path |
| 3 | 22-03 | This VALIDATION + architecture/docs sync |

---

## Validation Sign-Off

- [x] PLAT-03 mapped to compose-smoke / `make smoke`
- [x] PLAT-09 mapped to sqlite + mysql CI legs (+ db-matrix note)
- [x] PLAT-02 mapped to IaC/gateway files + human live apply
- [x] No claim that live Railway apply is CI-proven
