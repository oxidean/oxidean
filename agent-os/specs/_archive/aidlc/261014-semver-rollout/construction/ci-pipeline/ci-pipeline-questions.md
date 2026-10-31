# CI Pipeline — Questions & Answers

All answers are grounded in the repo's existing CI surface
(`.github/workflows/ci.yml` + the release workflows authored in U4/U5) —
no open ambiguity remained, so each question carries its verified answer.

## Q1. What CI tool is in use?

**Answer:** GitHub Actions. Workflow set: `ci.yml` (PR/push gates),
`release.yml` (tag lanes + per-component dispatch), `release-pr.yml`
(automated `chore(release):` PR), `ox-cli.yml` (ox binary matrix +
`latest.json` publishing), `publish-runner.yml` (runner image),
`production-deploy.yml` (untouched).

## Q2. What is the branch strategy?

**Answer:** Trunk-based (org memory): short-lived topic branches →
squash-merge PRs into `main`. CI triggers on push to `main` and on all
pull requests; releases trigger on `<component>-v*` tag pushes or
`workflow_dispatch`.

## Q3. What quality gates are required before merge?

**Answer:** As wired in `ci.yml`:
`commit-lint` (versioning grammar on PR range + PR title + registry
completeness), `api-rust` (nextest workspace, `ci` profile),
`web-octane` (lint/format/test/build), `route-coverage`,
`web-browser`, `browser-coverage`, `coverage-weighted`, `e2e-stack`,
`rpc-sync`, `compose`, `compose-smoke` matrix, `smoke-protocol`,
`db-matrix`, `docker-ci-images`, `skill-sync-check`.

## Q4. What artifact repositories are used?

**Answer:** GitHub Releases (component release notes from annotated tag
bodies; `ox-dist` rolling release holds CLI binaries + `latest.json`);
GHCR `ghcr.io/<owner>/oxidean-runner` for the runner image. No external
artifact registry (registry publishing deferred — all components
internal-versioned only).

## Q5. Which secrets/credentials gate the release lanes?

**Answer:** `RELEASE_TOKEN` (maintainer PAT) for tag pushes that must
cascade — the default `GITHUB_TOKEN` cannot re-trigger workflows. All
other jobs run on the default token with least-privilege `permissions`
blocks.

## Consolidated Summary Confirmation

The CI plan: document the existing GitHub Actions pipeline (ci.yml gates
+ the four release/distribution workflows wired in U4/U5), enumerate the
required quality gates, and record the phase-boundary verification that
the Build and Test commands are enforced by CI. No workflow changes are
needed — the pipeline already reflects the scope's design; this stage's
artifacts are the audit record of that.

Options:
- Looks correct — proceed
- Request changes — amend the summary

[Answer]: Looks correct
