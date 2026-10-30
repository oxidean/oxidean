# Deployment Execution — Clarifying Questions

## Q1. What gets deployed and where?

**Answer:** The `feat/oxpilot-agent` change set deploys as updated `oxidean-api`/`oxidean-web` GHCR images via the existing Railway IaC (`operation/deployment-pipeline/cd-config.md`). PR previews run the `:main` channel; production promote is operator-run post-merge.

## Q2. Is anything deployed by this stage?

**Answer:** No — merge is gated on green CI + human merge decision; the feature ships flag-off regardless. This stage records the execution plan and smoke/health evidence expected, plus the preview-environment surface the user will inspect.

## Q3. Post-deploy smoke?

**Answer:** `/health` on api + web; flag-off deny on `/api/oxpilot` SSE; flag-on envs: session create → stub turn → SSE snapshot/append contract (e2e-stack assertions).

## Q4. Failure response?

**Answer:** Flag-off fast rollback (one env var), else image-pointer rollback per `rollback-runbook.md`.

## Consolidated Summary Confirmation

- Deploys are image retags on Railway channels; production is operator-run post-merge (Q1).
- This stage deploys nothing itself — evidence + plan only; PR preview env is the live surface (Q2).
- Smoke = health probes + flag-off deny + stub-turn SSE contract (Q3).
- Rollback = flag env var first, image retag second (Q4).

Does this all look correct before I write the deployment-execution artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
