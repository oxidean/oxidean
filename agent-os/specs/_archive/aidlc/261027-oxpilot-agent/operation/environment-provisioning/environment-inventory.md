# Environment Inventory — oxpilot

Sources: per-unit `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**), `operation/deployment-pipeline/cd-config.md` (**cd-config**), `.railway/railway.ts`.

## Railway services (existing — unchanged by this intent)

| Service | Source | Notes |
|---|---|---|
| `api` | `ghcr.io/oxidean/oxidean-api:<channel>` | carries `OXIDEAN_OXPILOT_*` env (preserve()); runs migrations on boot |
| `web` | `ghcr.io/oxidean/oxidean-web:<channel>` | serves `/oxpilot` routes; proxies `/api/*` incl. SSE |
| `postgres` | managed | oxpilot schema via migrations `0087`–`0090` |
| `forge-data` volume | 20 GB | unchanged |
| `caddy` gateway | file-configured | public ingress |

## Environments

- **preview/PR** — `:main` channel; flag off by default (operator may opt in).
- **staging** — `:staging` channel; opt-in.
- **production** — `:production` channel; opt-in.

## Oxpilot-specific resources

- DB tables: `oxpilot_sessions`, `oxpilot_messages`, `oxpilot_tool_calls`, `oxpilot_computer_use_targets`, `oxpilot_grants`, `oxpilot_usage`, `oxpilot_settings`, `oxpilot_streaming_messages` (swept) — additive migrations only.
- Env vars: `OXIDEAN_OXPILOT_ENABLED`, `OXIDEAN_OXPILOT_PROVIDER`, `OXIDEAN_OXPILOT_MODEL`, budget ceilings (unbounded defaults), connector shared-secret via `preserve()`.
- No new services, volumes, or network exposure. Connector clients dial outbound to the API; nothing binds inbound on user systems.
