# Environment Provisioning — Clarifying Questions

Answered from `.railway/railway.ts` + `docs/DEPLOYMENT.md` — Railway-managed, not AWS.

## Q1. Are all environments provisioned per Infra Design?

**Answer:** Yes — Railway project carries api + web + postgres + `forge-data` volume + Caddy gateway per `.railway/railway.ts`; environments preview/staging/production share the IaC with per-env channel tags. Oxpilot adds no new services — schema migrations ride the api boot path.

## Q2. Networking/security groups correct?

**Answer:** Railway-internal networking; public ingress via Caddy gateway only. API serves `/api/oxpilot.*` + SSE behind the same auth middleware as existing routes — no new exposure surface. Computer-use targets connect outbound via the connector protocol (consent-gated), so no inbound ports are required on user systems.

## Q3. Secrets correctly injected?

**Answer:** `OXIDEAN_OXPILOT_*` and provider keys via `preserve()` — operator-managed per env, never committed (verified: `.env.example` carries only placeholder comments).

## Q4. Cross-service connectivity validated?

**Answer:** api↔postgres over Railway private networking (existing); `oxidean-web` reverse-proxies `/api/*` to `OXIDEAN_API_ORIGIN` (existing). Validation evidence: e2e-stack CI job boots the same topology.

## Consolidated Summary Confirmation

- No new infrastructure — oxpilot runs entirely inside existing Railway services + migrations (Q1).
- No new network exposure; consent-gated outbound connector only (Q2).
- Secrets operator-managed via preserve(); flag ships off (Q3).
- Existing api↔db↔web topology; e2e-stack CI validates the same shape (Q4).

Does this all look correct before I write the environment-provisioning artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
