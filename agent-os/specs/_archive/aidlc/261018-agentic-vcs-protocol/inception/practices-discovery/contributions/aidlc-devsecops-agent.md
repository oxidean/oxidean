**Collaborator:** aidlc-devsecops-agent

## Contribution

### Scope and evidence inspected

All seven workflows that actually exist — `.github/workflows/{ci,ci-base,images,production-deploy,release,release-beta,ox-cli}.yml`. Note for the record: the dispatch brief named `security.yml`, `e2e-production.yml`, `binary-e2e.yml`, `frontend-deploy.yml`, `sdk-publish.yml`, `deploy/db/`, and `packaging/` Dockerfiles — none of those exist in this repo; `deploy/` holds only `cloud/` (Caddyfile+Dockerfile) and `traefik/README.md`, and `packaging/` holds only `nfpm.yaml`. Also inspected: `docker-compose*.yml` (5 files), `deploy/cloud/{Caddyfile,Dockerfile}`, `.railway/{railway.ts,README.md,.npmrc,package.json}`, `docker/{ci-base,oxidean-runner}/Dockerfile`, `docker/Dockerfile.release-layer`, `crates/oxidean-api/{Dockerfile,docker-entrypoint.sh}`, `apps/web/Dockerfile`, `crates/oxidean-api/assets/cli/install.sh`, `scripts/railway-*.sh` + `ghcr-images.sh` + `github-deployment.sh`, `packaging/nfpm.yaml`, `Cross.toml`, `.env.example`, `bun.lock`, `Cargo.toml`/`Cargo.lock`, `.githooks/commit-msg`, `.github/pull_request_template.md`, `Makefile`, `docs/{DEPLOYMENT,CONFIGURATION,CODE_PRACTICES,TESTING}.md`, `CONTRIBUTING.md`, and codekb `technology-stack.md`/`dependencies.md`.

### Deployment & environments — verified accurate

- Merge-to-`main` → staging is real: `images.yml` builds all four service images `push:false`, gates the GHCR publish on every leg green (`images.yml:256-336`), then `deploy-staging` repins Railway to the all-or-nothing `sha-<sha>` set and moves the `:staging` channel tag (`:341-402`).
- Production is a true manual promote: `workflow_dispatch` on GitHub Environment `Oxidean / production`, promote SHA must be an ancestor of `main` AND have the latest CI run green (`production-deploy.yml:143-184`), waits for all `sha-*` manifests, then repins + retags `:production` + health-probes `app.oxidean.dev/health` (`scripts/railway-production-deploy.sh:102-180`). `rollback` uses `deploymentRollback` to prior `canRollback` deployments; `dry_run` plans without mutating; both paths post GitHub Deployment records since Railway emits none for image repins.
- Runner is correctly excluded from the production set (`railway.ts:169-175`, `railway-production-deploy.sh:47-51`).
- Railway pulls images, never builds; `.railway/railway.ts` seeds each env's channel tag over managed Postgres — all confirmed.
- **Correction needed:** the draft says "Stable cuts happen only inside production promote." `release.yml`'s `workflow_dispatch` also offers `channel: stable` on `release/*` branches (`release.yml:24-33, 76-145`) — a maintainer dispatch lane exists outside promote. Soften to acknowledge it.
- No fixed release cadence is visible — stable timing is operator-chosen (evidence.md already lists this unresolved).

### Lint/format gates — verified

All merge-blocking in `ci.yml`: `makefile-lint` (checkmake + `check-makefile.sh`), `rust-fmt` (`cargo fmt --check`), `web-octane` (type-aware `oxlint --deny-warnings` via `oxlint-tsgolint` + oxfmt/prettier `--check` + vitest + build), `dead-code` (knip + `cargo machete` + `clippy -D warnings` + `RUSTDOCFLAGS="-D warnings"`), `commit-lint` (SemVer grammar on PR range + title). `apps/web` has no ESLint by design. The draft's Code Style section is accurate.

### Secrets posture — verified strong

- No secrets in repo: `.env.example` placeholders are explicitly `not-for-production`; compose defaults are labelled local-only (`docker-compose.yml:82-88`); IaC keeps every secret `preserve()` (`railway.ts:86-108`); `.railway/` is an isolated `npm ci` env with `package-lock=true`.
- CI credential scoping: `GITHUB_TOKEN` defaults `contents: read` with per-job elevation; fork PRs get a read-only token and `images.yml` never even checks out fork code (`:79-84`) — forks structurally cannot publish or deploy; same-repo PRs publish only `pr-*` tags + ephemeral envs torn down on close (`:411-566`).
- Deploy credentials split by blast radius: repo secrets cover staging/PR-env binds; the production token is an environment-scoped project token on `Oxidean / production`. IaC apply is operator-machine-only — `railway-apply.sh:100` refuses under CI; apply tokens never enter repo or CI (`DEPLOYMENT.md:50`).
- Release pushes use the org-owned `oxidean-release` GitHub App (short-lived installation tokens, sole ruleset bypass) for stable cuts, and `RELEASE_TOKEN` (maintainer PAT) for beta/dispatch tag lanes — never `GITHUB_TOKEN`, which cannot re-trigger tag lanes.
- Product-side security is genuinely engineered: argon2 password hashing, AES-256-GCM for Actions/mirror/webhook secrets (`OXIDEAN_ACTIONS_SECRETS_KEY` required, fail-closed), in-process failed-auth rate limiting with rightmost-XFF trust rules, `OXIDEAN_WEB_BEHIND_PROXY` forwarded-header sanitization, `OXIDEAN_OIDC_ALLOW_INSECURE` dev-env-gated, header-gated `/health` on the web tier, transport auth that ignores session cookies.
- **Gap:** no automated secret scanning — no gitleaks/trufflehog/git-secrets hook or CI job; hygiene rests on the PR checklist. GitHub secret scanning / push protection status is a repo-settings question.

### SAST / DAST / dependency scanning — absent

No SAST (no CodeQL/Semgrep — the product's own workflow-template catalog even excludes `code-scanning` because it needs GHAS), no DAST, no `cargo audit`/`cargo deny`/`bun audit` gate, no Dependabot/Renovate config (codekb `dependencies.md:107`: "dependency updates are manual"), no dependency freshness floor (no root `.npmrc`/`bunfig.toml`; Bun has no `minimumReleaseAge` equivalent anyway — that's pnpm/npm territory), no SBOM, no container/image scanning (no Trivy/Grype), no OpenSSF Scorecard/zizmor/actionlint. The only freshness automation is the weekly `ci-base.yml` cron rebuild for base-image patch drift (`:54-56`).

### Supply chain — mixed, mostly solid

**Strong:**
- `bun.lock` carries per-package sha512 integrity and every install is `bun install --frozen-lockfile`; `Cargo.lock` is committed; CI tools install via `cargo install --locked` (`ci-base/Dockerfile:56-60`); `cargo machete` gates unused deps.
- CI itself runs inside self-owned `oxidean-ci:*` images with content-hash tags resolved to `@sha256` digests per run (`ci.yml:76-110`) — same-repo PRs may publish a content tag only matching their own inputs (`ci-base.yml:18-24,143`), and `-latest` stays main-owned.
- `ox` releases ship per-target `.sha256` + `SOURCE_COMMIT` + `latest.json`; the `image-matrix` job verifies every staged digest (`ox-cli.yml:547-566`); image builds and container boots verify checksums **fail-closed** (`Dockerfile:55-94`, `docker-entrypoint.sh:71-90`); nfpm itself is checksum-verified at install (`ox-cli.yml:389-399`).
- Deploys pin immutable `sha-<sha>` tags; channel tags move only via deploy tooling; publish/deploy lanes run `cancel-in-progress: false` so a tag set is never half-published.

**Gaps:**
- Docker base images are tag-pinned, not digest-pinned — notably `lukemathwalker/cargo-chef:latest-rust-1-bookworm` (a mutable `latest-*` tag from a third-party publisher, used in all three service builds), floating `debian:bookworm-slim`, `oven/bun:1.4-debian` in `apps/web` vs pinned `oven/bun:1.4.0-debian` in ci-base (inconsistent granularity), deliberate `ghcr.io/cross-rs/*:main` in `Cross.toml`.
- GitHub Actions are major-version-tag pinned (`@v4`–`@v8`) and `dtolnay/rust-toolchain@stable` is a branch pin — no SHA pinning and no bot to bump them.
- `download.docker.com` CLI tarball and compose/buildx plugin downloads in ci-base + runner images are version-pinned but **not** checksum-verified (unlike nfpm).
- `install.sh`'s checksum check is **fail-open** — a missing `.sha256` or missing sha256 tool installs anyway (`install.sh:75-86`), inconsistent with the fail-closed fetch paths everywhere else.
- No binary/image signing or provenance attestation (no cosign/GPG/SLSA) — sha256 is the whole integrity contract for a product that is itself a forge.
- Runtime hardening: service images run as root (no `USER`); the self-host runner mounts `docker.sock` RW for `docker://` jobs (opt-in profile, documented); Traefik mounts it RO with `security_opt label=disable`.

### Suggested wording for team-practices.md

Append to `## Deployment`:

> The deploy pipeline's security posture: workflow `GITHUB_TOKEN` defaults to `contents: read` and elevates per job; fork PRs get a read-only token and publish/deploy nothing; same-repo PRs publish only `pr-*` images to ephemeral Railway environments torn down on close. Deploy credentials are scoped by blast radius — repo secrets cover staging and PR environments, while the production token is an environment-scoped project token on the `Oxidean / production` GitHub Environment. Railway IaC apply runs from operator machines only (`railway-apply.sh` refuses under CI); apply tokens never enter the repo or CI. Release pushes use the `oxidean-release` GitHub App (stable cuts) or `RELEASE_TOKEN` (beta/dispatch lanes) — never `GITHUB_TOKEN`, which cannot re-trigger tag lanes.
>
> Supply-chain contract: `bun install --frozen-lockfile` (bun.lock carries sha512 integrity) and `npm ci` (`.railway/package-lock.json`) are the only dependency-install paths; `Cargo.lock` is committed and CI tools install `cargo install --locked`. CI runs inside self-owned `oxidean-ci:*` images resolved to `@sha256` digests per run — our one digest-pinning point. Docker base images and GitHub Actions stay version-tag-pinned rather than digest-pinned (weekly ci-base rebuilds absorb base-image patch drift). Released `ox` binaries ship per-target `.sha256` + `SOURCE_COMMIT`; image builds and container boots verify those checksums fail-closed. We run no SAST/DAST, dependency-vulnerability, or secret scanning, and no update bot — dependency freshness is manual.

### Suggested discovered-rules additions

`## Mandated` (all already hard-enforced by CI or script):
- ALWAYS install JS dependencies via `bun install --frozen-lockfile` (workspace) or `npm ci` (`.railway/`) — the committed lockfiles are the dependency contract.
- ALWAYS deploy the all-or-nothing `sha-<sha>` image set; channel tags (`:staging`/`:production`) move only via deploy tooling after a green deploy.
- ALWAYS keep `railway config apply` off CI and IaC-apply credentials out of repo/CI secrets (the script refuses under CI).
- ALWAYS require a promote SHA to be an ancestor of `main` with the latest CI run green; rehearse changes with `dry_run`.

`## Candidates` (interview):
- Whether every externally fetched artifact must be checksum-verified (today: nfpm yes, docker CLI tarball no, `install.sh` fail-open).
- Whether fork-PR lanes must stay publish-free (structurally true today — affirm intent).
- Whether tag-pinned base images/Actions remain the standard or move to digest pinning.
- Whether root containers + the self-host runner's `docker.sock` mount are accepted debt for the self-host threat model.

### Interview gaps a human must resolve

1. **Required reviewers on `Oxidean / production`:** the workflow calls them optional — if not enabled, the "manual gate" is single-operator: any maintainer who can run `workflow_dispatch` can promote and cut a stable release alone.
2. **Scanning stance:** is the absence of SAST/DAST, dependency scanning (`cargo audit`/`bun audit`/Dependabot), and secret scanning deliberate? If a first step is wanted, the cheapest is a scheduled `cargo audit` + enabling GitHub secret scanning/push protection.
3. **GitHub-side controls invisible in-repo:** secret scanning + push protection enabled? Dependabot *alerts* on? Are `*-v*` release tags protected by a tag ruleset, or can any maintainer push them directly (the release lanes assume only RELEASE_TOKEN/App pushes)?
4. **`RELEASE_TOKEN`:** accepted long-lived maintainer PAT, or migrate the beta/dispatch lanes to the existing release App (Contents: write, short-lived tokens)?
5. **Artifact assurance:** for a forge product whose job is hosting others' supply chains, are sha256-only releases the affirmed contract, or should `ox` binaries/images gain signatures/attestations/SBOM?
6. **`install.sh` fail-open checksum:** keep (old-POSIX tolerance) or make fail-closed like the image/boot paths?
7. **`Main` ruleset facts:** PR-only, squash-merge enforced, sole bypass = `oxidean-release` App — visible only via comments/history; confirm at the gate.

## Positions

- AGREE: Deployment section's staging-on-merge + gated-promote model — verified end-to-end against `images.yml`, `production-deploy.yml`, `railway-production-deploy.sh`, and `railway.ts`; rollback/dry-run/health-probe/Deployment-record details are all accurate.
- AGREE: Way of Working claims on PR-only main + release-App sole bypass — consistent with workflow comments and zero merge commits (GitHub-side settings remain interview-confirmable).
- AGREE: Lint/format enforcement — rustfmt, type-aware oxlint, oxfmt/prettier, checkmake, and commit-lint are all merge-blocking CI gates as described.
- AGREE: Secrets hygiene — `preserve()` IaC vars, local-only compose defaults, no committed secrets, and well-scoped CI/deploy credentials are genuinely good practice.
- OBJECT: "Stable cuts happen only inside production promote" — `release.yml`'s dispatch lane also offers `channel: stable` on `release/*` branches; the claim needs a carve-out for the maintainer path.
- OBJECT: The Deployment section describes mechanics but omits the security posture that makes them trustworthy — add the suggested paragraph on token scoping, fork isolation, and the checksum contract.
- OBJECT: No security-scanning stance is stated anywhere — silence reads as oversight, not decision; the interview must affirm the deliberate gap or schedule first scans, and the Candidates list should record it.
- OBJECT: `discovered-rules.md` misses four deploy-path rules already hard-enforced (frozen-lockfile installs, sha-set deploys, no-CI-apply, ancestor+CI-green promote) — promote them to Mandated.
- OBJECT: `install.sh`'s checksum verification is fail-open while every other `ox` fetch path is fail-closed — record as debt or affirm as accepted behavior.
- Minor OBJECT: service images run as root and the self-host runner mounts `docker.sock` RW for `docker://` jobs — acceptable documented risk, but worth one line in Deployment so the affirmation is informed.
