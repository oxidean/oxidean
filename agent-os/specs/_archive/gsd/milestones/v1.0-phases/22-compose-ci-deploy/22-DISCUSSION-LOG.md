# Phase 22 — Discussion Log

**Date:** 2026-09-16  
**Mode:** Auto-decide (user: discuss → plan; campaign defaults)

## Inputs

- ROADMAP Phase 22: Compose CI & Cloud Deploy; PLAT-02/03/09
- Existing CI: config-only `compose`, Postgres `smoke-protocol`, three-way `db-matrix`
- Existing Make: `smoke` / `smoke-sqlite` / `smoke-mysql`
- Railway skill: Dockerfile builder + `.railway/railway.ts` IaC (no deprecated railway.json)

## Gray areas resolved without interactive Q&A

| Area | Decision |
|------|----------|
| Compose bring-up in CI | Reuse compose-smoke Make targets; matrix Postgres + SQLite + MySQL in PR |
| MySQL coverage | In-PR Compose smoke leg (not nightly-only) |
| Cloud host | Railway-class; same Dockerfiles; managed Postgres |
| Cloud ingress | File-config gateway (Caddy/Traefik-file); Traefik docker socket stays Compose-local |
| CD | Docs + IaC only; no PR secrets / auto-apply |

## Outcome

`22-CONTEXT.md` locked for research + planning.
