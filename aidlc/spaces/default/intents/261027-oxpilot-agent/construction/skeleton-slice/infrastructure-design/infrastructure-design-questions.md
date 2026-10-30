# Infrastructure Design — Questions (skeleton-slice)

Decisions for this stage. Resolved under the standing self-drive instruction; ratify at PR review.

## Q1 — New infrastructure footprint
Does U1 provision any new infrastructure?
[Answer]: A
- A. Zero new infrastructure — U1 lands entirely inside the existing `api` service (new crate + SSE routes), `oxidean-db` tables on the existing databases (sqlite/postgres/mysql), and the `web` service (new route). No new Railway services, queues, caches, or workers.
- B. A dedicated worker service for turn tasks.
- C. A shared state store (Redis) for ActiveTurn.

## Q2 — CI/CD change set
What does the pipeline need for U1?
[Answer]: A
- A. Reuse existing jobs — `oxidean-api-oxpilot` joins the workspace (compiled + nextest-sharded by existing rust lanes), `dialect_oxpilot.rs` joins the `db-dialects -E` filter + TESTING.md job table, the route joins coverage manifests + `check-route-sync`. No new job or matrix value ⇒ no `Main` ruleset change (per team.md's merge contract).
- B. A dedicated oxpilot CI job.
- C. New jobs + ruleset update.

## Q3 — Monitoring/alerting
What monitoring does U1 add at platform level?
[Answer]: A
- A. None — `tracing` spans only (NFR6); Railway platform logs carry stdout. No new alerts/dashboards/SLO pipeline; `oxpilot.status` is the operator diagnostic surface. The N=1 smoke ceilings live in a stack test, not monitoring.
- B. New metrics exporter + alert rules.
- C. Health-check endpoint additions.

## Q4 — Deployment strategy
How does the change reach environments?
[Answer]: A
- A. Existing path unchanged: PR → `pr-<n>` images on the ephemeral Railway PR environment → squash-merge → `sha-<sha>` images → Railway staging; production stays the manual promote. The skeleton rides the api/web image rebuilds; env vars ship documented (`OXIDEAN_OXPILOT_ENABLED` defaults off ⇒ zero-risk rollout).
- B. A staged canary for the SSE routes.
- C. A separate oxpilot deploy track.

## Q5 — Secrets/config in deployment
How do `OXIDEAN_OXPILOT_*` env vars reach services?
[Answer]: A
- A. Same path as every env var: `.env.example` + `docs/CONFIGURATION.md` documented, Railway IaC envs optional (default-off means absence is valid). No secrets in U1 — provider/model are config strings (custody chain is U4/U6's concern).
- B. Secrets manager entries in U1.
- C. Build-time baking.

## Consolidated Summary Confirmation

Decisions consolidated:
- Q1 A: zero new infrastructure — everything lands inside existing api/web services + oxidean-db tables.
- Q2 A: reuse existing CI jobs — workspace lanes + db-dialects -E filter + coverage manifests; no ruleset change.
- Q3 A: no new monitoring — tracing only; status proc is the diagnostic surface.
- Q4 A: existing deploy path (PR env -> staging -> manual promote); flag defaults off.
- Q5 A: env-var docs path only; no secrets material in U1.

Does this all look correct before I write the infrastructure artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
