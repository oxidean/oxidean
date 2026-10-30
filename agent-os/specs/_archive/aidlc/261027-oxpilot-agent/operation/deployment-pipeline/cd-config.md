# CD Configuration — oxpilot

Sources: `construction/ci-pipeline/ci-config.md` (**ci-config**), `construction/ci-pipeline/quality-gates.md` (**quality-gates**), per-unit `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**) and `infrastructure-design/cicd-pipeline.md` (**cicd-pipeline**).

Deployment rides the existing Railway TypeScript IaC (`.railway/railway.ts`) — Oxidean Cloud only; self-hosted users deploy the same GHCR images via Compose (`docs/DEPLOYMENT.md`).

## Services and env wiring

- `api` service carries `OXIDEAN_OXPILOT_ENABLED` / `OXIDEAN_OXPILOT_PROVIDER` / `OXIDEAN_OXPILOT_MODEL` via `preserve()` — per-environment operator secrets, never in the repo.
- Managed Postgres; API runs migrations on boot (all environments incl. production promote). Oxpilot migrations `0087`–`0090` (sessions, messages, targets, grants, usage, settings) apply in the same boot path.
- Images: `ghcr.io/oxidean/oxidean-api:<channel>` built by `images.yml`; `oxidean-web` serves `/oxpilot` routes + proxies `/api/oxpilot.*` + the SSE route.

## Promotion flow

1. PR green → squash-merge to `main` → `images.yml` builds `sha-*` + retags `:main`.
2. Staging: `railway config plan` → `apply` → channel `:staging` points at the verified image.
3. Production: same plan/apply with operator approval; `:production` channel.

## Flag rollout

Ship flag-off. Enable per environment: set `OXIDEAN_OXPILOT_ENABLED=true` + provider creds via Railway variables (preserve pattern). No code redeploy needed to toggle.
