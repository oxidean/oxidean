# Oxidean self-host — Docker Compose bundle

A production-shaped Compose stack built from the published images
(`ghcr.io/oxidean/oxidean-{api,web,runner}`). No repository checkout or
source build required — copy this directory's files anywhere.

| File | Purpose |
|------|---------|
| `docker-compose.yml` | postgres + api + web, plus `tls` (caddy) and `actions` (runner) profiles |
| `docker-compose.docker-sock.yml` | opt-in override mounting `docker.sock` into the runner for `docker://` job labels — root-equivalent, trusted workflows only |
| `Caddyfile` | TLS edge for the `tls` profile — `reverse_proxy web:3000`, auto-HTTPS for `$DOMAIN` |
| `.env.example` | every supported knob, commented — copy to `.env` |

Full guide with security checklist, variants, upgrades, backup, and
troubleshooting: [`docs/SELF-HOSTING.md`](../../docs/SELF-HOSTING.md).
Environment variable reference: [`docs/CONFIGURATION.md`](../../docs/CONFIGURATION.md).

## Quick start (local eval)

```bash
cp .env.example .env
# fill POSTGRES_PASSWORD and OXIDEAN_ACTIONS_SECRETS_KEY
# and set OXIDEAN_ENV=development  (eval mode: no Secure cookies,
# auto-generates the web-flow signing key)
docker compose up -d
```

Open <http://localhost:8080> — the `/setup` wizard creates the first
admin account.

## Quick start (real domain)

```bash
cp .env.example .env
# set DOMAIN, OXIDEAN_PUBLIC_ORIGIN=https://<domain>, OXIDEAN_SSH_HOST=<domain>,
# both secrets, and OXIDEAN_WEB_FLOW_PRIVATE_KEY (see .env.example)
docker compose --profile tls up -d
```

Open `https://<domain>` — Caddy issues a Let's Encrypt certificate once the
domain resolves to this host and ports 80/443 are reachable.

## Layout notes

- The **web tier is the origin**: it serves the UI and proxies all API
  paths (RPC, `*.git` Smart HTTP + LFS, registries, badges, Pages, OAuth,
  websockets) to `api`. No Traefik, no Docker socket for routing.
- **One volume** (`oxidean_data` at `/var`) holds every forge state root:
  repos, LFS, packages, release assets, uploads, SSH host keys, action
  logs/artifacts/cache.
- Git-over-SSH is raw TCP on `OXIDEAN_SSH_PORT` (default `2222`), straight
  to `api` — never through the HTTP edge.
- `latest` images track stable releases; pin `OXIDEAN_VERSION=x.y.z` and
  `docker compose pull && docker compose up -d` to upgrade.
