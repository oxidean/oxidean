# Oxidean Cloud — Railway IaC

TypeScript Infrastructure as Code for **Oxidean Cloud** on Railway (PLAT-02 / D-CLOUD-01…07).

## Layout

| File | Role |
|------|------|
| `railway.ts` | Desired topology: managed Postgres + `api` + `web` + `gateway` |
| `package.json` | Isolated npm env for the `railway` IaC SDK (not part of Bun workspaces) |

**Do not** add deprecated `railway.json` / `railway.toml` (cutoff 2026-12-01).

## Same images as Compose

| Service | Builder | Dockerfile |
|---------|---------|------------|
| `api` | `DOCKERFILE` | `crates/oxidean-api/Dockerfile` (repo-root context) |
| `web` | `DOCKERFILE` | `apps/web/Dockerfile` (repo-root context) |
| `runner` | `DOCKERFILE` | `docker/oxidean-runner/Dockerfile` (repo-root context) |
| `gateway` | `DOCKERFILE` | `deploy/cloud/Dockerfile` (Caddy file proxy) |

Cloud default database is **managed Postgres** (`postgres()` helper). MySQL/SQLite remain self-host/CI dialects only (D-CLOUD-02).

## Environments

| Environment | Role | Deploy trigger |
|-------------|------|----------------|
| `preview` | Persistent base for Railway **PR Environments** | Autodeploy off (IaC / manual only) |
| `staging` | Always-on integration | Autodeploy from `main` + Wait for CI |
| `production` | Live | Autodeploy **off**; promote only via GitHub Action |

**Important:** TypeScript IaC (`github()` + `checkSuites`) does **not** disable Autodeploy. After `railway config apply` on production, open each of `api` / `web` / `gateway` → Settings → GitHub and click **Disable** (or delete `deploymentTriggers`), while leaving the repo connected for promote-by-SHA. Verify with `make cloud-production-autodeploy-check`.

Ephemeral PR environments clone `preview` (services, networking, variables) when a project member opens a PR. They are deleted when the PR merges or closes. Bot PR Environments stay off unless you explicitly enable them.

**Collaborators:** invite people to the Railway **project** (Viewer is enough) and have them link their GitHub account. Repo collaborator status alone does not grant PR Environments.

**Repo transfer or rename:** if the GitHub repo moves orgs or is renamed, the GitHub App installation changes and Railway's stored service connections go stale — `config apply` rewrites `source.repo` but does not rebind the connection, and all `deploymentTriggers` are lost. Symptoms: pushes and PRs produce no deploys and no PR environments, and `githubRepoUpdate` returns "Invalid source". Fix per service: `serviceConnect` (or Settings → Source → reconnect) with the new `owner/repo`, which rebinds to the live installation and re-creates `main` triggers on every environment — then delete the triggers on `preview` and `production` and keep `staging` on `main` + Wait for CI. If repo listing returns "Not Authorized", re-link GitHub in Railway account settings.

**Focused PR Environments (recommended):** enable in Project Settings → Environments, then set watch paths on services, for example:

| Service | Watch paths |
|---------|-------------|
| `api` | `crates/oxidean-api/**`, `crates/oxidean-core/**`, `crates/oxidean-db/**`, `packages/api-client/**` |
| `web` | `apps/web/**`, `packages/**` |
| `runner` | `docker/oxidean-runner/**`, `crates/oxidean-runner/**`, `Cargo.lock` |
| `gateway` | `deploy/cloud/**` |

**Volume size:** `forge-data` is **20480 MB** (20 GB) in IaC.

## Operator workflow (D-CLOUD-07)

`railway config apply` runs from an **operator machine**, never from CI: no workflow invokes `config plan`/`config apply`, and apply credentials exist only as the operator's own `railway login` session or env vars — **tokens never enter CI or the repo**. The single CI-held Railway credential is the human-gated promote/rollback token on the `Oxidean / production` GitHub Environment, which deploys commits only and is never used for IaC apply.

### Prerequisites

| Prerequisite | Command / note |
|--------------|----------------|
| Railway CLI ≥ 5.42.1 | `railway --version` |
| `jq` | `scripts/railway-apply.sh` uses it to verify the linked context |
| IaC SDK | `cd .railway && npm ci` once per checkout (isolated npm env — not Bun) |
| Auth on the operator machine | `railway login`, or export `RAILWAY_TOKEN` / `RAILWAY_API_TOKEN` |
| Linked context | `railway link --project <project-id> --environment <env>` |

Token choice: a **project token** (`RAILWAY_TOKEN`, project Settings → Tokens) is scoped to one environment — preferred for apply. An **account/workspace token** (`RAILWAY_API_TOKEN`) or CLI login suits operators applying across environments. Keep tokens in the shell: not in files, Make targets, CI secrets, or IaC source. `config plan`/`apply` take **no** `--project`/`--environment` flags — the link selects the mutation target, so verify it before every run.

### Preview and apply one environment

```bash
# 1. Link the target environment (repeat per environment)
railway link --project <project-id> --environment staging

# 2. Preview — checks auth, prints the linked project/environment, then runs
#    `railway config plan`. --environment asserts the link before proceeding.
scripts/railway-apply.sh --environment staging        # plan only (default)
# or: make cloud-plan                                 # bare plan, no assertion

# 3. Review the plan: expect only intended diffs — new resources on first
#    apply, small variable/trigger deltas after. Secrets stay preserve() /
#    hidden; never `--show-values` into a shared terminal or file.

# 4. Apply — the CLI reprints the plan and asks for confirmation; that
#    prompt is the human-verify gate.
scripts/railway-apply.sh --environment staging --apply
# equivalent: railway config apply                    # answer the prompt
```

Apply the same topology to `preview`, `staging`, and `production` — relink and re-plan for each. `scripts/railway-apply.sh --apply` refuses to run when `CI` is set. To apply **exactly** the reviewed plan (CLI ≥ 5.45.1): `railway config plan --out tmp/railway-plan.json` then `railway config apply --plan tmp/railway-plan.json --yes` — keep the artifact under gitignored `tmp/`; it can carry secret-bearing values.

### After apply — dashboard steps IaC cannot do

1. **Deploy triggers:** `config apply` connects GitHub and may create `deploymentTriggers`. Per the [environments table](#environments): `staging` keeps `main` autodeploy + Wait for CI; `preview` and `production` get autodeploy **off** (service → Settings → GitHub → Disable). Verify with `make cloud-production-autodeploy-check`.
2. **Per-environment `preserve()` secrets:** set in the dashboard on `api` / `web` / `runner` — see the secrets paragraph below (runner token pair, web-flow key, `OXIDEAN_VITE_ALLOWED_HOSTS`).
3. **Domain:** attach a Railway or custom domain to `gateway` — the advertise vars reference `${{gateway.RAILWAY_PUBLIC_DOMAIN}}`.

### Verify and rollback

```bash
railway status --json                          # linked context
railway service list --json                    # api, web, runner, gateway, postgres
railway variable list --service api --json     # IaC vars materialized; preserves intact
```

Config apply mutates configuration, not running deployments — the next deploy picks it up (staging autodeploys; production promotes via the GitHub Action). Then `GET https://<domain>/health` must return 200.

There is no IaC undo. To revert config: check out the prior git revision of `.railway/railway.ts`, re-plan, review, re-apply. Destructive diffs (resource removal) require the CLI's `--confirm-destructive` — treat them as data-loss operations. To roll back a bad **deploy**, use the Production deploy action below — not a config re-apply.

Secrets (`OXIDEAN_ENV`, `OXIDEAN_VITE_ALLOWED_HOSTS`, `OXIDEAN_ACTIONS_SECRETS_KEY`, SSO/email keys, etc.) stay in the Railway dashboard or `preserve()` — not in git. Set a unique `OXIDEAN_ACTIONS_SECRETS_KEY` on each environment’s **api** service (`openssl rand -base64 32`); without it, mirror credentials and Actions secrets cannot be saved. `OXIDEAN_WEB_FLOW_PRIVATE_KEY` (optional, **api**) pins the web-flow commit-signing key — required on `production`/`cloud` where auto-generation fails closed; preview/staging/PR Environments auto-generate on first use when unset.

IaC sets public browser/SSH advertise vars from the **gateway** domain (not `preserve()`):

| Variable | Source |
|----------|--------|
| `OXIDEAN_PUBLIC_ORIGIN` / `OXIDEAN_CORS_ORIGINS` (api + web origin) | `https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}` |
| `OXIDEAN_SSH_HOST` | `${{gateway.RAILWAY_PUBLIC_DOMAIN}}` |
| `OXIDEAN_API_ORIGIN` (web) | `http://${{api.RAILWAY_PRIVATE_DOMAIN}}:8080` |
| `OXIDEAN_AUTO_MIGRATE` | `true` on all environments (including production) |
| `OXIDEAN_PROTECTION_HELPER` | `/usr/local/bin/oxidean-protection-hook` (API image) |

PR Environments inherit from `preview`; dynamic gateway refs and auto-migrate on every environment (including production) keep schema current and avoid stale preview origins. The API/web also replace a stale `*.up.railway.app` origin with `RAILWAY_SERVICE_GATEWAY_URL` / `RAILWAY_PUBLIC_DOMAIN` (custom domains are left alone).

On **`web`**, set `OXIDEAN_VITE_ALLOWED_HOSTS` so `vite preview` accepts the gateway Host header (e.g. `.up.railway.app,app.oxidean.dev`). Details: [docs/CONFIGURATION.md](../docs/CONFIGURATION.md).

### Promote / rollback production

Do **not** rely on Environment Sync for promote: Sync includes variables and can clobber production-only origins and `OXIDEAN_ENV`.

1. Confirm the commit is healthy on **staging**.
2. GitHub → **Actions** → **Production deploy** → Run workflow:
   - **promote** — `serviceInstanceDeployV2` with `commitSha` for `api` / `web` / `gateway` (default: `main` HEAD). Gated on CI success for that SHA. Leaves production variables alone.
   - **rollback** — `deploymentRollback` to the prior `canRollback` deployment on each of those services.
   - **dry_run** — toggle on to print the plan without mutating Railway (still needs `RAILWAY_TOKEN`/`RAILWAY_API_TOKEN` for rollback target lookup).
3. Smoke `https://app.oxidean.dev/health` (skipped on dry-run).

Scripts: [`scripts/railway-production-deploy.sh`](../scripts/railway-production-deploy.sh), [`scripts/railway-production-autodeploy-check.sh`](../scripts/railway-production-autodeploy-check.sh) (`make cloud-production-autodeploy-check`). Workflow: [`.github/workflows/production-deploy.yml`](../.github/workflows/production-deploy.yml).

**GitHub Environment `Oxidean / production`:** add `RAILWAY_TOKEN` (Railway project token scoped to the production environment — sent as `Project-Access-Token`, not Bearer; an account/workspace token in `RAILWAY_API_TOKEN` also works). Enable required reviewers if you want an approval gate on the button.

## Actions runner service

The `runner` service runs `oxidean-runner` (native Rust, `crates/oxidean-runner`) against the api over the private network. **Host execution only** — Railway exposes no Docker socket, so `docker://` labels are unsupported; the declared labels (`ubuntu-latest,self-hosted`) run steps on the runner host. `actions/checkout` is implemented as a git clone of `OXIDEAN_PUBLIC_ORIGIN` (the api's private domain) — no public egress needed.

State (runner token, workspaces) persists on the `runner-data` volume at `/data`.

**One-time token setup (per environment):** `OXIDEAN_RUNNER_REGISTRATION_TOKEN` must hold the **same value** on the `api` and `runner` services — `preserve()` cannot share a variable across services. In each environment's dashboard:

1. Generate once: `openssl rand -hex 32`
2. Set it on `api` → `OXIDEAN_RUNNER_REGISTRATION_TOKEN` **and** `runner` → `OXIDEAN_RUNNER_REGISTRATION_TOKEN`
3. Redeploy `runner` — it self-registers on boot (or reuses the persisted token on `runner-data`)

The env bootstrap token is reusable while set — acceptable for preview/staging/PR Environments. On **production** prefer Admin-minted one-time tokens (`admin.actions.createRegistrationToken`) and leave the api-side env unset; set `OXIDEAN_RUNNER_REGISTRATION_TOKEN` on `runner` to the minted `reg_…` value instead.

Optional: `OXIDEAN_RUNNER_GIT_TOKEN` (a PAT with `repo` scope) on `runner` enables cloning **private** repositories — public repos clone anonymously.

## Related

- Gateway: [`deploy/cloud/README.md`](../deploy/cloud/README.md)
- Deploy steps: [`docs/DEPLOYMENT.md`](../docs/DEPLOYMENT.md)
