# Discovered Rules — Oxidean

> Affirmed version — integrated by aidlc-pipeline-deploy-agent (Step 5). Hard
> constraints only: each rule below is enforced by a hook, a CI gate, a
> stated hard boundary in repo docs, or an affirmed interview decision —
> not merely a convention. Candidates resolved at the interview are noted
> under `## Resolved at interview`; genuinely open debt sits under
> `## Recorded Debt` and is not rules.

## Mandated

- ALWAYS write every commit subject in the SemVer grammar `type(optional-scope)[!]: subject` (allowed types in `versioning.toml` `[grammar]`; `release` scope reserved) — enforced by `.githooks/commit-msg` (`make setup`) and CI `commit-lint` on the PR range and PR title; subjects drive per-component version bumps and changelogs.
- ALWAYS regenerate generated artifacts from their producers and commit the regenerated output in the same change — `make rpc-gen` after RPC/type changes (`packages/api-client`), `make openapi-gen` (`docs/openapi.yaml`), and the skill/route/stack-preset/workflow-template sync targets; CI `rpc-sync` runs each check with `git diff --exit-code` semantics.
- ALWAYS keep dialect-specific SQL inside `crates/oxidean-db`; API, web, and any new crate must not branch on DB dialect (migrations are written ×3 in `migrations/{postgres,mysql,sqlite}` with filename-parity tests).
- ALWAYS reach `main` through a pull request — the `Main` ruleset admits only PR merges; the sole bypass actor is the release App for the stable-cut `chore(release):` commit.
- ALWAYS pass `make web-lint` and `make web-format-check` before merging `apps/web` changes; CI `web-octane` gates both (type-aware oxlint is the web type gate — plain `tsc` does not parse `.tsrx`).
- ALWAYS declare new Makefile targets `.PHONY` with a runnable recipe under `scripts/` paths that exist — `scripts/check-makefile.sh` and the CI `makefile-lint` job enforce it.
- ALWAYS register coverage evidence for surfaces the manifests govern: every new/changed `apps/web` route needs a route-coverage row, and new/changed high-risk interactive UI needs a `*.browser.test.tsx` or documented stack-browser evidence — the change-aware `route-coverage` and `browser-coverage` CI jobs fail on fresh skip-only rows.
- ALWAYS install JS dependencies via `bun install --frozen-lockfile` (workspace) or `npm ci` (`.railway/`) — the committed lockfiles are the dependency contract (promoted at interview Q5; enforced across `ci.yml`/`ci-base` and `.railway/.npmrc`).
- ALWAYS deploy the all-or-nothing `sha-<sha>` image set; channel tags (`:staging`/`:production`/`:main`) move only via deploy tooling after a green deploy — `images.yml`/`production-deploy.yml` run `cancel-in-progress: false` so a tag set is never half-published (promoted at interview Q5).
- ALWAYS keep `railway config apply` off CI and IaC-apply credentials out of repo/CI secrets — `scripts/railway-apply.sh` refuses to run under CI and apply tokens never enter the repo or CI (promoted at interview Q5).
- ALWAYS require a production promote SHA to be an ancestor of `main` with the latest CI run green; rehearse changes with `dry_run` before mutating (promoted at interview Q5; enforced by `production-deploy.yml`).
- ALWAYS document every new `OXIDEAN_*` env var in `.env.example` and `docs/CONFIGURATION.md` in the same PR that introduces it — PR-checklist requirement, affirmed hard rule for this intent's new crate config (promoted at interview Q5).
- ALWAYS authenticate protocol/machine transports with PAT/pubkey-class credentials — session cookies are never accepted on transport surfaces (D-12 precedent: git transports accept Basic-PAT/pubkey only; load-bearing for this intent's new VCS transport; promoted at interview Q5).
- ALWAYS use the `oxidean-release` GitHub App (stable cuts) or `RELEASE_TOKEN` (beta/dispatch tag lanes) for release-automation pushes — never `GITHUB_TOKEN`, which cannot re-trigger tag lanes (FR3.4; promoted at interview Q5).

## Forbidden

- NEVER commit secrets — no `.env`, tokens, or private keys in the repo, CI config/logs, or docs examples; examples live in `.env.example` / `docs/dev-auth.env.example` (AGENTS.md hard boundary, PR checklist).
- NEVER hand-edit generated sources as if they were the source of truth — `packages/api-client/src/index.ts`, `docs/openapi.yaml`; change the Rust producers and regenerate.
- NEVER add AI or tooling attribution (`Co-authored-by`, `Generated with`, `Made-with`, bot generator signatures) to commits or PRs — `.cursor/rules/no-ai-attribution.mdc`.
- NEVER reintroduce `Swatinem/rust-cache` or per-job Rust toolchain installs into `ci.yml` — the digest-pinned `oxidean-ci:*` images are the dep cache by design (AGENTS.md / ci.yml header).

## Resolved at interview

- **Coverage floor for the new crate** — resolved (Q3-A): the enforced 0.70
  weighted composite (web) remains the only coverage floor; no new numeric
  Rust floor is invented for the VCS crate, and Rust coverage stays
  collect-only — recorded as debt in `evidence.md`/`team-practices.md`, not
  a rule.
- **Test-first/TDD** — resolved (Q3-A): `test-after` is the affirmed
  methodology; the in-repo `tdd` skill is opt-in, not a mandate. Not a rule.
- **Env-var documentation** — resolved (Q5-A): promoted to `## Mandated`.
- **Session-cookie auth on transports** — resolved (Q5-A): promoted to
  `## Mandated` (PAT/pubkey-class only, D-12).
- **Release-automation pushes** — resolved (Q5-A): promoted to
  `## Mandated` (release App/`RELEASE_TOKEN` only, FR3.4).
- **Fork-PR publish-free lanes** — resolved (Q4-A): affirmed as structural
  intent (fork code is never checked out, so forks cannot publish or
  deploy); recorded in `## Deployment`, no separate rule needed.

## Recorded Debt

Genuinely open items the interview declined to make rules — recorded for
honesty, to be revisited as their own intents:

- **`install.sh` checksum is fail-open** — a missing `.sha256` or missing
  sha256 tool installs anyway (`install.sh:75-86`), inconsistent with the
  fail-closed fetch paths in image builds and container boots. Undecided:
  keep for old-POSIX tolerance or make fail-closed.
- **No security scanning** — no SAST/DAST, no `cargo audit`/`bun audit`
  dependency-vulnerability gate, no secret scanning, no update bot
  (Dependabot/Renovate absent). Affirmed as acknowledged status quo, not
  assumed deliberate policy.
- **Tag-vs-digest pinning granularity** — Docker base images
  (`lukemathwalker/cargo-chef:latest-*`, `debian:bookworm-slim`,
  `oven/bun:1.4-debian`) and GitHub Actions (`@v4`–`@v8` major tags,
  `dtolnay/rust-toolchain@stable` branch pin) stay tag-pinned; the
  `download.docker.com` CLI tarball and compose/buildx plugin downloads are
  version-pinned but not checksum-verified. Only the self-owned
  `oxidean-ci:*` images are digest-resolved.
- **Root containers + `docker.sock`** — service images run as root (no
  `USER`), and the self-host runner mounts `docker.sock` read-write for
  `docker://` jobs (opt-in profile, documented). Accepted risk for the
  self-host threat model, not a mandated invariant.
