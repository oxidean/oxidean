# Oxidean Cloud — Railway IaC

TypeScript Infrastructure as Code for **Oxidean Cloud** on Railway (PLAT-02 / D-CLOUD-01…07).

## Layout

| File | Role |
|------|------|
| `railway.ts` | Desired topology: managed Postgres + `api` + `web` + `runner` + `gateway` |
| `package.json` | Isolated npm env for the `railway` IaC SDK (not part of Bun workspaces) |

**Do not** add deprecated `railway.json` / `railway.toml` (cutoff 2026-12-01).

## Image-based services (oxidean#124)

Every service is an **image source** — [`.github/workflows/images.yml`](../.github/workflows/images.yml) builds all four images in GitHub Actions, gates pushes until every image is green, and publishes the all-or-nothing `sha-<commit>` tag set to GHCR (public `ghcr.io/oxidean/*` packages — Railway pulls unauthenticated). Railway never builds from the repo, so all four services deploy identical bits — no build skew, no duplicated build compute.

| Service | Image | Built from |
|---------|-------|------------|
| `api` | `ghcr.io/oxidean/oxidean-api` | `crates/oxidean-api/Dockerfile` (repo-root context) |
| `web` | `ghcr.io/oxidean/oxidean-web` | `apps/web/Dockerfile` (repo-root context) |
| `runner` | `ghcr.io/oxidean/oxidean-runner` | `docker/oxidean-runner/Dockerfile` (repo-root context) |
| `gateway` | `ghcr.io/oxidean/oxidean-gateway` | `deploy/cloud/Dockerfile` (Caddy file proxy) |

Tag model:

| Tag | Meaning |
|-----|---------|
| `sha-<sha>` | **Deploy identity** — the immutable, atomic unit deploys pin |
| `main` | Moving tip of `main` |
| `staging` / `production` | **Channel aliases** — deploy tooling retags the deployed `sha-*` image to the env's channel on every deploy |
| `<semver>` (+`latest`) | Published on component tag lanes (`oxidean-api-v*`→api, `web-v*`→web, `oxidean-runner-v*`→runner; `latest` is stable-only) |
| `pr-<n>` / `pr-<n>-sha-<sha>` | Same-repo PR builds |

IaC seeds each environment with its **channel tag** (`:production`, `:staging`, `:main` elsewhere), so a `config apply` redeploys the exact bits the environment was already running — never whatever `main` happens to be.

**One-time setup:** each `oxidean-{api,web,runner,gateway}` package exists on GHCR after the first `images.yml` run; flip each to **public** (Package settings → Change visibility → public, or link the package to this repo and keep the org default). Railway then pulls unauthenticated. If a package must stay private, attach registry credentials per service instead (service → Settings → Source → registry credentials — a GHCR PAT with `read:packages`); IaC does not manage them.

Cloud default database is **managed Postgres** (`postgres()` helper). MySQL/SQLite remain self-host/CI dialects only (D-CLOUD-02).

## Environments

| Environment | Role | Deploy trigger |
|-------------|------|----------------|
| `preview` | Persistent base for Railway **PR Environments** | Manual / IaC only; `:main` seed |
| `staging` | Always-on integration | `images.yml` deploys the `sha-*` set on every main push |
| `production` | Live | Promote only via GitHub Action (`production-deploy.yml`) |

Image-sourced services cannot carry GitHub deployment triggers at all — there is no Autodeploy to disable. `make cloud-production-autodeploy-check` still guards against a repo source being re-added.

**Keep every environment's serviceInstances image-sourced** (`source.repo` unset). A repo-connected instance still fires Railway deployments on the connected branch — PR environments fork `preview`'s instances wholesale, so a drifted seed makes every fresh fork's initial deploy evaluate the branch diff and the instance's `watchPatterns` cancel the non-matching legs. Each cancellation posts an `Oxidean - <svc>` commit status in state `error` ("Deployment cancelled") on the pushed commit — a no-op read as a failure on main (#206). `scripts/railway-image-source-check.sh` asserts the source model and heals drift (`--repair` repins to the env's channel image, config-only; `--repair-statuses` resolves leftover cancelled statuses as `success`); the hourly `pr-env-sweep.yml` and `images.yml`'s `deploy-staging` run it automatically.

Ephemeral PR environments fork `preview` (services, networking, variables) — including the `:main` image seed. Image-sourced services have no repo triggers, so Railway's GitHub app cannot create them: `images.yml`'s **bind-pr-env** job find-or-creates the environment via `scripts/railway-pr-env-ensure.sh` — a short poll for a Railway-provisioned env (`meta.prNumber`), then `environmentCreate` with `ephemeral: true`, `sourceEnvironmentId` = preview, and the conventional name `oxidean-pr-<n>` (self-created envs are found by name — the API cannot set `meta.prNumber`). The job then repins every service to the PR's `pr-<n>-sha-<sha>` image once the build lands (CI-created envs skip initial deploys, so their first roll is already the PR image). Every subsequent push re-pins to the new sha tag. Redeploy progress is visible on the PR as a **GitHub Deployment** in `Oxidean / oxidean-pr-<n>` (`in_progress` → `success`/`failure`, linking the env URL) — Railway only posts Deployment records for git-triggered builds, so the bind job posts its own (and waits for every service's new deployment to reach a terminal state before flipping success). Fork PRs publish nothing and are never bound. On PR close (merged or abandoned) two lanes share teardown — both call `scripts/pr-env-teardown.sh <n>`, which deletes the Railway environment (`railway-pr-env-delete.sh`), deletes the GitHub Environment the deployment statuses created (`Oxidean / oxidean-pr-<n>`, marking its deployments inactive when the delete is denied), and garbage-collects the `pr-<n>`/`pr-<n>-sha-*` GHCR tags: **`cleanup-pr`** in `images.yml` is the fast path on `pull_request:closed`, but that run is routinely cancelled before jobs start on merge (GitHub removes `refs/pull/<n>/merge` with the PR), so the authoritative teardown is **`pr-env-sweep.yml`** — hourly + `workflow_dispatch`, it scans all three surfaces and tears down every lane whose PR is no longer open. Environment deletes need an admin-scoped GitHub token (release App via `RELEASE_APP_ID`/`RELEASE_APP_PRIVATE_KEY`, then `RELEASE_TOKEN`); `RAILWAY_API_TOKEN` must be an account/workspace token — env-scoped project tokens cannot enumerate environments. Per-leg failures warn but still fail the job so leaks stay visible.

**Collaborators:** PR environments are created by `bind-pr-env` for every same-repo PR — no Railway membership or GitHub linkage needed for a preview to exist. Invite people to the Railway **project** (Viewer is enough) only so they can open the env in the dashboard.

**Repo transfer or rename:** if the GitHub repo moves orgs or is renamed, update `IMAGES` in `railway.ts` if the GHCR namespace moved — image sources have no repo connection to rebind, and PR environments are created by `bind-pr-env`, not the GitHub App.

**Volume size:** `forge-data` is **20480 MB** (20 GB) in IaC.

## Operator workflow (D-CLOUD-07)

`railway config apply` runs from an **operator machine**, never from CI: no workflow invokes `config plan`/`config apply`, and apply credentials exist only as the operator's own `railway login` session or env vars — **apply tokens never enter CI or the repo**. CI-held Railway credentials are deploy-scoped only: repo secrets `RAILWAY_API_TOKEN`/`RAILWAY_TOKEN` for `images.yml`'s staging deploys, PR-environment binds, and CLI-feed repins, plus the human-gated promote/rollback token on the `Oxidean / production` GitHub Environment — none is ever used for IaC apply.

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

1. **Per-environment `preserve()` secrets:** set in the dashboard on `api` / `web` / `runner` — see the secrets paragraph below (runner token pair, web-flow key, SSO/email keys).
2. **Domain:** attach a Railway or custom domain to `gateway` — the advertise vars reference `${{gateway.RAILWAY_PUBLIC_DOMAIN}}`.
3. **First deploy after switching to image sources:** the seeded channel tag must exist on GHCR (`:staging` / `:production` are moved by deploy tooling after the first deploy — `:main` exists after the first `images.yml` push). Until then a `config apply` redeploy pulls a missing tag — run `images.yml` first, or apply source changes together with the first image deploy.

### Verify and rollback

```bash
railway status --json                          # linked context
railway service list --json                    # api, web, runner, gateway, postgres
railway variable list --service api --json     # IaC vars materialized; preserves intact
```

Config apply mutates configuration, not running deployments — the next deploy picks it up (staging tracks `images.yml` main-push deploys; production promotes via the GitHub Action). Then `GET https://<domain>/health` must return 200.

There is no IaC undo. To revert config: check out the prior git revision of `.railway/railway.ts`, re-plan, review, re-apply. Destructive diffs (resource removal) require the CLI's `--confirm-destructive` — treat them as data-loss operations. To roll back a bad **deploy**, use the Production deploy action below — not a config re-apply.

Secrets (`OXIDEAN_ENV`, `OXIDEAN_ACTIONS_SECRETS_KEY`, SSO/email keys, etc.) stay in the Railway dashboard or `preserve()` — not in git. Set a unique `OXIDEAN_ACTIONS_SECRETS_KEY` on each environment’s **api** service (`openssl rand -base64 32`); without it, mirror credentials and Actions secrets cannot be saved. `OXIDEAN_WEB_FLOW_PRIVATE_KEY` (optional, **api**) pins the web-flow commit-signing key — required on `production`/`cloud` where auto-generation fails closed; preview/staging/PR Environments auto-generate on first use when unset.

IaC sets public browser/SSH advertise vars from the **gateway** domain (not `preserve()`):

| Variable | Source |
|----------|--------|
| `OXIDEAN_PUBLIC_ORIGIN` / `OXIDEAN_CORS_ORIGINS` (api + web origin) | `https://${{gateway.RAILWAY_PUBLIC_DOMAIN}}` |
| `OXIDEAN_SSH_HOST` | `${{gateway.RAILWAY_PUBLIC_DOMAIN}}` |
| `OXIDEAN_API_ORIGIN` (web) | `http://${{api.RAILWAY_PRIVATE_DOMAIN}}:8080` |
| `OXIDEAN_WEB_BEHIND_PROXY` (web) | `1` — the gateway sanitizes `X-Forwarded-*`, so `oxidean-web` trusts the edge's chain |
| `OXIDEAN_AUTO_MIGRATE` | `true` on all environments (including production) |
| `OXIDEAN_PROTECTION_HELPER` | `/usr/local/bin/oxidean-protection-hook` (API image) |
| `OXIDEAN_CLI_DIST_URL` (api) | Owned by **deploy tooling**, not IaC — `scripts/railway-staging-api-redeploy.sh` and `scripts/railway-production-deploy.sh` upsert it (skipDeploys) to the `ox-v*` tag-release download URL for the deployed commit. Staging pins the tag **pointing at** that commit; production falls back to the newest stable `ox-v*` tag merged into it, so a stable-cut bump commit that didn't touch `ox` still serves the latest released CLI (empty = no released `ox` at all → host-only). **Runtime env var** on Cloud: the api container's entrypoint (`crates/oxidean-api/docker-entrypoint.sh`) fetches + verifies the matrix at boot into a URL-keyed dir on the `forge-data` volume and points `OXIDEAN_CLI_DIST_DIR` at it — feed unreachable or checksum mismatch → container never reports healthy → the previous replica stays. Self-host builds keep the **build-arg** path baked into the image (default empty → host-only `ox`). Preview / PR envs leave it unset → same-commit host `ox` only, with `SOURCE_COMMIT` baked at build time for a truthful `dist_commit` — and host-only images synthesize a `latest.json` for that staged binary, so `/cli/latest` advertises it and `ox self-update` can align to the deployment's own `ox`. |

### ox CLI multi-arch matrix

Cloud CLI distribution is **tag-pinned**: deploys fetch the matrix from the
`ox-v*` release matching the deployed commit — there is no rolling feed.

1. An `ox-v*.*.*` tag push runs [`.github/workflows/ox-cli.yml`](../.github/workflows/ox-cli.yml), which publishes the tag's GitHub Release with the full matrix (`ox-targets.txt` + `ox-*` + `.sha256` + `SOURCE_COMMIT` + installers/packages, plus `latest.json` on stable cuts).
2. `images.yml`'s **staging-cli-feed** job (workflow_run on ox-cli completing for an `ox-v*` tag) invokes [`scripts/railway-staging-api-redeploy.sh`](../scripts/railway-staging-api-redeploy.sh) (`RAILWAY_API_TOKEN` or staging-scoped `RAILWAY_TOKEN`; skipped when absent). Tag resolution is shared via [`scripts/ox-dist-tag.sh`](../scripts/ox-dist-tag.sh) (pipefail-safe; PR / path-filter ox-cli runs never enter this job). The script upserts `OXIDEAN_CLI_DIST_URL` to the tag's download URL and `serviceInstanceRedeploy`s staging `api` — the container's boot fetch then serves the new matrix. No image rebuild or retag is involved.
3. [`scripts/railway-production-deploy.sh`](../scripts/railway-production-deploy.sh) `promote` applies the pin on production `api` before the image deploy, falling back to the newest stable `ox-v*` tag merged into the deployed commit. Preview / PR envs keep the var unset → they serve the same-commit host `ox` (linux-amd64 only), advertised on `/cli/latest` via the synthesized manifest; deploys with no released `ox` tag at all go host-only.
4. Verify: `GET …/cli/latest` lists darwin/windows (and may include `dist_commit`); macOS `install.sh` succeeds.

Self-hosted images leave the build-arg empty (Compose default) or point at **their** artifact feed — never require upstream Oxidean's releases. Details: [docs/CLI.md](../docs/CLI.md).

PR Environments inherit from `preview`; dynamic gateway refs and auto-migrate on every environment (including production) keep schema current and avoid stale preview origins. The API/web also replace a stale `*.up.railway.app` origin with `RAILWAY_SERVICE_GATEWAY_URL` / `RAILWAY_PUBLIC_DOMAIN` (custom domains are left alone).

The **`web`** service is `oxidean-web` (Rust) — there is no JS runtime or Host allowlist to configure; the gateway owns Host routing. Details: [docs/CONFIGURATION.md](../docs/CONFIGURATION.md).

### Promote / rollback production

Do **not** rely on Environment Sync for promote: Sync includes variables and can clobber production-only origins and `OXIDEAN_ENV`.

1. Confirm the commit is healthy on **staging**.
2. GitHub → **Actions** → **Production deploy** → Run workflow:
   - **promote** — pins `source.image` to `ghcr.io/oxidean/oxidean-<svc>:sha-<sha>` for `api` / `web` / `runner` / `gateway` (default: `main` HEAD) via [`scripts/railway-image-deploy.sh`](../scripts/railway-image-deploy.sh), then moves the `:production` channel tag to the deployed images. Gated on CI success **and** all four `sha-*` manifests existing on GHCR — the stable cut's `chore(release):` bump commit gets its image set as a seconds-long release-layer retag by `images.yml`, so the wait is normally short. Also **creates the stable release** (changelog + `chore(release):` bump commit + `*-vX.Y.Z` tags as the release GitHub App (`RELEASE_APP_ID` / `RELEASE_APP_PRIVATE_KEY`)) and deploys the resulting bump commit; the first-ever promote tags the `0.1.0` baseline for every component. `cut_release: false` skips the cut (plain re-promote). When a fresh `ox-v*` tag was just cut, the job waits for the `ox-cli.yml` asset matrix before deploying. Leaves production variables alone.
   - **rollback** — `deploymentRollback` to the prior `canRollback` deployment on each of those services (restores the previous image refs).
   - **dry_run** — toggle on to print the plan without mutating Railway (still needs `RAILWAY_TOKEN`/`RAILWAY_API_TOKEN` for rollback target lookup).
3. Smoke `https://app.oxidean.dev/health` (skipped on dry-run).

Scripts: [`scripts/railway-production-deploy.sh`](../scripts/railway-production-deploy.sh), [`scripts/railway-image-deploy.sh`](../scripts/railway-image-deploy.sh), [`scripts/railway-production-autodeploy-check.sh`](../scripts/railway-production-autodeploy-check.sh) (`make cloud-production-autodeploy-check`). Workflows: [`.github/workflows/production-deploy.yml`](../.github/workflows/production-deploy.yml), [`.github/workflows/images.yml`](../.github/workflows/images.yml).

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
