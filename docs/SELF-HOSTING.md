# Self-hosting Oxidean with Docker Compose

Run a complete Oxidean forge — web UI, API, database, Git-over-SSH, optional
TLS edge and Actions runner — from published images. No repository checkout,
no Rust or Bun toolchain, no build step.

The bundle lives in [`deploy/selfhost/`](../deploy/selfhost/) and consists of
three files you can drop into any empty directory:

```bash
mkdir oxidean && cd oxidean
# copy docker-compose.yml, Caddyfile, .env.example from deploy/selfhost/
cp .env.example .env
```

Related docs: [CONFIGURATION.md](CONFIGURATION.md) (every env var),
[DEPLOYMENT.md](DEPLOYMENT.md) (all deployment targets incl. Oxidean Cloud),
[database.md](database.md) (dialects).

## Architecture

| Service | Image | Role |
|---------|-------|------|
| `web` | `ghcr.io/oxidean/oxidean-web` | The single HTTP origin: serves the UI **and** reverse-proxies every API path — RPC, `/{owner}/{repo}.git` Smart HTTP + LFS, `/v2` `/npm` `/generic` registries, `/cli`, badges, Pages, OAuth, `/api/rpc/ws` websockets — to `api`. |
| `api` | `ghcr.io/oxidean/oxidean-api` | Forge API + git backend. Owns the `/var` state volume and the raw-TCP SSH listener. |
| `postgres` | `postgres:18-alpine` | Bundled database (swap for an external `DATABASE_URL`, MySQL, or SQLite — see Variants). |
| `caddy` *(profile `tls`)* | `caddy:2-alpine` | TLS terminator for `$DOMAIN` (Let's Encrypt auto-HTTPS) → `web:3000`. Optional — bring your own edge instead. |
| `runner` *(profile `actions`)* | `ghcr.io/oxidean/oxidean-runner` | CI job executor; opt-in, one per fleet. |

Because `oxidean-web` already routes API paths internally, **no
Docker-socket-aware proxy is needed**: the edge is a one-line
`reverse_proxy web:3000`. SSH is the only second socket — raw TCP on
`OXIDEAN_SSH_PORT` straight to `api`.

All forge state lives in **one named volume** (`oxidean_data` mounted at
`/var` on `api`): bare repos, LFS objects, packages, release assets,
uploads, template packs, VCS objects, action logs/artifacts/cache, and the
SSH host-key dir.

## Prerequisites

- Docker Engine + Compose v2 (`docker compose`)
- For the TLS path: a DNS name pointing at the host and ports 80/443 reachable (Let's Encrypt)
- For git-over-SSH: a free TCP port (default `2222`)

## Quick start A — local evaluation

```bash
cp .env.example .env
# set POSTGRES_PASSWORD and OXIDEAN_ACTIONS_SECRETS_KEY
# (openssl rand -base64 24 / openssl rand -base64 32)
# for eval only, also set OXIDEAN_ENV=development
docker compose up -d
```

Open <http://localhost:8080>. On an empty instance the `/setup` wizard
creates the first `sys-admin` account.

`OXIDEAN_ENV=development` is the eval-friendly choice: session cookies are
not `Secure`-flagged (so plain `http://` works) and the web-flow signing key
auto-generates into `/var/ssh`. Don't carry `development` into real use —
see the HTTP note below.

## Quick start B — real domain with TLS

```bash
cp .env.example .env
```

Required edits in `.env`:

```bash
POSTGRES_PASSWORD=<openssl rand -base64 24>
OXIDEAN_ACTIONS_SECRETS_KEY=<openssl rand -base64 32>
DOMAIN=git.example.com
OXIDEAN_PUBLIC_ORIGIN=https://git.example.com
OXIDEAN_SSH_HOST=git.example.com
OXIDEAN_ENV=production   # default
# Seed-commit signing — required in production BEFORE creating repos via UI:
#   ssh-keygen -t ed25519 -N '' -f web-flow && base64 -w0 web-flow
OXIDEAN_WEB_FLOW_PRIVATE_KEY=<single-line base64 output>
```

```bash
docker compose --profile tls up -d
```

Open `https://git.example.com` — Caddy issues a certificate on first
request once DNS and ports 80/443 resolve to the host.

## First boot

Two paths to the first `sys-admin` — pick one, they are equivalent:

- **`/setup` wizard** (default): visit the origin on an empty users table.
- **ENV seed**: set `OXIDEAN_ADMIN_EMAIL` + `OXIDEAN_ADMIN_PASSWORD` before
  first boot → creates `system-administrator` with
  `must_change_credentials` (forces `/setup/credentials` on first sign-in).
  Seed failure fails the boot.

`OXIDEAN_ALLOW_SIGNUP` defaults **false** — after bootstrap, only
invite-created accounts can join. Mail (Resend → SMTP) is optional; without
it, invite/verify links are shown once in the UI instead of emailed.

## Required secrets

| Variable | Generate | Purpose |
|----------|----------|---------|
| `POSTGRES_PASSWORD` | `openssl rand -base64 24` | Bundled DB credential (feeds `DATABASE_URL`) |
| `OXIDEAN_ACTIONS_SECRETS_KEY` | `openssl rand -base64 32` | AES-256-GCM key: Actions/mirror/webhook secrets + TOTP at rest — encrypt fails closed without it |
| `OXIDEAN_WEB_FLOW_PRIVATE_KEY` | `ssh-keygen -t ed25519 -N '' -f web-flow && base64 -w0 web-flow` | Signs UI-seeded initial commits. Required in `production`/`cloud` (fail closed); auto-generated elsewhere |

Optional but recommended on real installs: `OXIDEAN_SECRETS_KEY` (dedicated
TOTP key), `OXIDEAN_SMTP_URL`/`OXIDEAN_RESEND_API_KEY` (mail delivery).

## Ports and clone URLs

| Host | Service | Notes |
|------|---------|-------|
| `443`/`80` | caddy (`tls` profile) | Public HTTPS + ACME |
| `${OXIDEAN_HTTP_BIND:-127.0.0.1:8080}` | web | Direct app port — loopback by default |
| `${OXIDEAN_SSH_PORT:-2222}` | api | Raw TCP — never through the HTTP edge |

Clone URLs (shown in the UI's CloneBox):

- **HTTPS** — `{OXIDEAN_PUBLIC_ORIGIN}/{owner}/{repo}.git`, PAT auth via HTTP Basic
- **SSH** — `git@{OXIDEAN_SSH_HOST}:{owner}/{repo}.git`. When the port ≠ 22
  users need a `Port` entry in `~/.ssh/config` (CloneBox shows the hint).

## Variants

**External database** — set `DATABASE_URL` in `.env` and delete the
`postgres` service plus the `depends_on` block under `api`. Scheme picks the
dialect: `postgres://`, `mysql://`, `sqlite:`.

**SQLite** — `DATABASE_URL=sqlite:/var/oxidean.db` (file inside the
`oxidean_data` volume), remove `postgres`. Suitable for small installs.

**MySQL** — run your own MySQL and point `DATABASE_URL` at
`mysql://user:pass@host:3306/db`. The dev overlay
[`docker-compose.mysql.yml`](../docker-compose.mysql.yml) shows a bundled
variant if you want one.

**Your own reverse proxy** — publish the web port on a reachable bind
(`OXIDEAN_HTTP_BIND=0.0.0.0:8080`), keep `OXIDEAN_WEB_BEHIND_PROXY=1`, and
point your edge at it. Everything — UI, RPC, git clone, registries, badges,
websockets — forwards correctly from the single port; just make sure your
edge overwrites `X-Forwarded-*` (any mainstream proxy does). If you expose
the web port to clients **without** a sanitizing edge, set
`OXIDEAN_WEB_BEHIND_PROXY` empty so the tier scrubs spoofable forwarding
headers itself.

**Disable git-over-SSH** — `OXIDEAN_SSH_ENABLED=false` (optionally remove
the `ports:` entry under `api`).

**Air-gapped / custom image registry** — retag the three
`ghcr.io/oxidean/oxidean-*` images into your registry and edit the `image:`
lines; the drift check's allowlist only applies to the shipped file.

## Actions runner

```bash
# .env: OXIDEAN_RUNNER_REGISTRATION_TOKEN=<token>
docker compose --profile actions up -d
```

- The env token is **reusable while set** — prefer one-time DB-minted tokens
  (`admin.actions.createRegistrationToken`, or Admin → Runners in the UI)
  on internet-facing instances and leave the env var unset.
- The runner mounts `docker.sock` for `docker://` job labels — that socket
  is root-equivalent on the host. Remove the mount to run host-mode jobs
  only.
- `OXIDEAN_RUNNER_ORIGIN` defaults to `http://api:8080` (in-network); remote
  runners point it at the public origin instead.

## Upgrades

```bash
# .env: OXIDEAN_VERSION=<new semver tag>
docker compose pull
docker compose up -d
```

`OXIDEAN_AUTO_MIGRATE=true` (default) applies pending migrations on api
boot — a failed migration exits before `/health` passes, so the previous
container state is not lost. `latest` tracks the newest stable tag; pinning
`OXIDEAN_VERSION=x.y.z` keeps upgrades deliberate. Check release notes
before jumping.

## Backup

Two things hold state — the DB and the `oxidean_data` volume:

```bash
docker compose exec postgres pg_dump -U oxidean oxidean > oxidean-db.sql
docker run --rm -v <project>_oxidean_data:/var:ro -v "$PWD:/backup" \
  alpine tar czf /backup/oxidean-data.tgz -C /var .
```

(`<project>` is the directory name — `docker volume ls | grep oxidean_data`
confirms.) Restore order: DB first, then the volume, then `up -d`.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Sign-in loops / never sticks on `http://<host>` | Session cookies are `Secure` for every `OXIDEAN_ENV` except `development`/`dev` — browsers only send them over HTTPS (or localhost) | Terminate TLS (recommended), or accept the trade-offs and set `OXIDEAN_ENV=development` |
| api exits: `OXIDEAN_CORS_ORIGINS is required` | Non-dev env without CORS list | Set `OXIDEAN_CORS_ORIGINS` (defaults to `OXIDEAN_PUBLIC_ORIGIN`) |
| Creating a repo fails in production | Web-flow signing key missing (auto-gen disabled in `production`/`cloud`) | Provision `OXIDEAN_WEB_FLOW_PRIVATE_KEY` or drop `web-flow`/`web-flow.pub` into `/var/ssh` in `oxidean_data` |
| `… required to encrypt secrets at rest` | `OXIDEAN_ACTIONS_SECRETS_KEY` unset | Generate `openssl rand -base64 32` into `.env` |
| Clone shows wrong host/port | `OXIDEAN_PUBLIC_ORIGIN` / `OXIDEAN_SSH_*` don't match the public address | Align all three with the real URL |
| No mail, invite links "missing" | Mail unset → log sink | Invite URLs are shown once in the UI; configure `OXIDEAN_SMTP_URL` or `OXIDEAN_RESEND_API_KEY` |
| ssh clone refused | Port ≠22 needs `Port` in `~/.ssh/config`, or listener disabled | `OXIDEAN_SSH_ENABLED=true`, publish `${OXIDEAN_SSH_PORT}` |
| Runner jobs never claimed | Token mismatch or profile not started | Match `OXIDEAN_RUNNER_REGISTRATION_TOKEN` on both services; `--profile actions` |
| All clients share one rate-limit bucket | `OXIDEAN_WEB_BEHIND_PROXY=1` while the web port is reached without a sanitizing edge — client IPs aren't preserved | Either put a real edge in front or unset the var |

## Security checklist

- Unique `OXIDEAN_ACTIONS_SECRETS_KEY` and `POSTGRES_PASSWORD` — never reuse
  the dev-compose defaults from the repository root stack
- `OXIDEAN_ENV=production` + HTTPS for anything beyond localhost eval
- Leave `OXIDEAN_RUNNER_REGISTRATION_TOKEN` unset on internet-facing
  instances; mint one-time tokens
- `OXIDEAN_ALLOW_SIGNUP` stays `false` unless you intend open registration
- Keep the web port on loopback when an edge fronts it; unset
  `OXIDEAN_WEB_BEHIND_PROXY` only when the web tier itself is the edge
- Treat `docker.sock` on the runner as root-equivalent — network-isolate it

## For contributors

`make selfhost-check` (CI job `selfhost-example`) validates this example:
compose parses on all profile combos, image refs stay in the published GHCR
set, every referenced env var resolves in `crates/**` or
[CONFIGURATION.md](CONFIGURATION.md), and `.env.example` stays wired and
documented. Update the example, `.env.example`, and this doc together when
adding or renaming configuration knobs.
