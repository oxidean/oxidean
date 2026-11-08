# Deployment Pipeline — Clarifying Questions

Answered from the existing Railway IaC (`.railway/railway.ts`, `docs/DEPLOYMENT.md`) — no new pipeline created.

## Q1. Deployment strategy

**Answer:** Rolling redeploy via Railway service image retag. Services pull GHCR images built by `images.yml`; `scripts/railway-image-deploy.sh` retags the environment's channel to the live `sha-*` image — `config apply` redeploys identical bits, no source drift.

## Q2. Environment promotion gates

**Answer:** preview/PR-env (`:main` channel) → staging (`:staging`) → production (`:production`). `railway config plan` then `apply` only with explicit operator approval.

## Q3. Production approval workflow

**Answer:** Human operator runs `railway config plan` + `apply`; PR merge requires all required status checks green (see `ci-pipeline/quality-gates.md`).

## Q4. Rollback procedure

**Answer:** Retag the channel to the previous `sha-*` image and `config apply` — images are immutable in GHCR, so rollback is a pointer move. DB migrations are forward-only; see rollback-runbook.md.

## Q5. Feature flags

**Answer:** `OXIDEAN_OXPILOT_ENABLED` (preserve() — never committed), following the `OXIDEAN_MCP_ENABLED` pattern. Ships flag-off; operators opt in per environment. `OXIDEAN_OXPILOT_PROVIDER`/`OXIDEAN_OXPILOT_MODEL` select the provider; `stub` is the safe default.

## Consolidated Summary Confirmation

- Rolling image-retag deploys on the existing Railway IaC; no new pipeline (Q1).
- preview -> staging -> production promotion, channel-tagged GHCR images, operator plan/apply approval (Q2, Q3).
- Rollback = channel retag to prior sha image; additive migrations mean no down-migration (Q4).
- OXIDEAN_OXPILOT_* via preserve(), ships flag-off; stub provider default (Q5).

Does this all look correct before I write the deployment-pipeline artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
