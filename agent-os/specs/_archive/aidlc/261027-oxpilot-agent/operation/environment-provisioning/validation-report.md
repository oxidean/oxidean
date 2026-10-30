# Environment Validation Report — oxpilot

## IaC validation

- `.railway/railway.ts` type-checks under the repo toolchain (`bun` workspace build — CI `web-octane`/image jobs); `OXIDEAN_OXPILOT_*` keys are `preserve()`d so `config apply` cannot clobber operator secrets.
- Migrations `0087`–`0090` validated by `db-dialects` CI across Postgres/MySQL/SQLite (`dialect_oxpilot` probe); reserved-word `window` → `window_day` rename landed pre-push.

## Security posture validation

- **Secrets:** no production secrets in repo, CI, or docs (`preserve()` + `.env.example` placeholders only) — verified by inspection and the secrets-scan CI gates already in the pipeline.
- **Exposure:** no new listeners; SSE + procs behind existing auth middleware; flag-off denial tested (`sse_routes_deny_when_disabled`, proc denial tests in `oxidean-api --test oxpilot`).
- **Computer-use:** consent-gated grants persisted in `oxpilot_grants`; PAT credentials refused session scope (`pat_credential_denied_session_only`); connector authn via shared secret over the connector protocol.
- **Log hygiene:** `oxidean-core` redaction covers provider credentials/content (`cargo test -p oxidean-core redact`).

## Health checks

- `/health` on api and web (existing probes in railway.ts) — unchanged.
- Oxpilot smoke (flag-on envs): session create → stub-provider turn → SSE frames — covered by `e2e-stack` assertions.

## Provisioning status

No live provisioning performed by this stage — Railway environments are operator-managed via `railway config plan/apply` (see `deployment-pipeline/cd-config.md`). This stage validated that no *new* infrastructure is required and that the oxpilot schema/env surface is correctly wired into the existing IaC.
