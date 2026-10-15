# Oxidean Cloud gateway

File-configured **Caddy** reverse proxy for Railway-class hosts. Replaces Compose Traefik’s Docker-socket provider (D-CLOUD-03).

## What it does

Mirrors Compose path priorities from `docker-compose.yml` Traefik labels:

| Path | Backend |
|------|---------|
| `/{owner}/{repo}.git` | `api:8080` (Smart HTTP) |
| `/v2`, `/npm`, `/generic` | `api:8080` (packages) |
| `/api`, `/uploads`, `/health` | `api:8080` |
| everything else | `web:3000` (SPA) |

Public Host is catch-all (suitable for Railway domains / custom DNS). TLS is expected at the platform edge.

## Build

Cloud deploys pull the published image — `.github/workflows/images.yml`
builds `ghcr.io/oxidean/oxidean-gateway` (public GHCR, sha/main/semver tags)
alongside the other service images; Railway never builds this Dockerfile.
Local build only:

```bash
docker build -f deploy/cloud/Dockerfile -t oxidean-gateway .
```

Pinned base: `caddy:2.11.4-alpine` (Docker Official Image).

## Env (set by `.railway/railway.ts`)

| Variable | Default | Purpose |
|----------|---------|---------|
| `PORT` | `8080` | Listen port (Railway public) |
| `API_HOST` | private DNS of `api` | Upstream API hostname |
| `API_PORT` | `8080` | Upstream API port |
| `WEB_HOST` | private DNS of `web` | Upstream web hostname |
| `WEB_PORT` | `3000` | Upstream web port |

## Operator notes

- Do **not** mount a Docker socket on the cloud host.
- Git-over-SSH stays on the **api** service TCP publish (optional); HTTPS Smart HTTP via this gateway is the always-on cloud clone path (D-CLOUD-08).
- Forge persistence: Railway volume `forge-data` mounts at `/var` on `api` (repos / lfs / packages / release-assets / uploads / ssh).
- Plan/apply discipline: see [`.railway/README.md`](../../.railway/README.md).
