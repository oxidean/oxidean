# Rollback Runbook — oxpilot

## Fast rollback (preferred)

The feature is flag-gated — set `OXIDEAN_OXPILOT_ENABLED=false` in the environment's Railway variables and restart the `api` service. UI routes remain reachable but render the flag-off state; SSE/procs deny. No image redeploy needed. Use this for any post-enable regression.

## Full image rollback

1. Identify the previous good `sha-*` tag in GHCR (`gh api` or Railway deployment history).
2. `scripts/railway-image-deploy.sh` to retag the environment channel to that image (same path as deploy — rollback is a pointer move on immutable images).
3. `railway config apply` (with approval) → services restart on the prior image.

## Database considerations

Migrations `0087`–`0090` are additive (new `oxpilot_*` tables/columns only) — rolling the image back leaves the schema in place harmlessly; no down-migration required. Do NOT drop oxpilot tables for rollback (data loss); the flag gate is the containment.

## Post-rollback verification

- `GET /health` 200 on api and web services.
- `POST /api/oxpilot/chat` without flag → flag-off denial (expected).
- Railway deploy status healthy; check logs for migration errors (none expected — additive).
