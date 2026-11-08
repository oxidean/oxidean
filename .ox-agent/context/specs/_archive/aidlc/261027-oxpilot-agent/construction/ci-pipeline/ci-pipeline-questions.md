# CI Pipeline — Clarifying Questions

Answered inline from the repository's existing CI (`.github/workflows/`, `docs/TESTING.md`) — no new pipeline is being created; this stage documents and validates the existing configuration against the intent's needs.

## Q1. CI tool

**Answer:** GitHub Actions — `.github/workflows/ci.yml` (required status checks), `images.yml`, `ox-cli.yml` (path-filtered). Jobs run inside self-owned `ghcr.io/oxidean/oxidean-ci:*` images built by `ci-base.yml`.

## Q2. Branch strategy

**Answer:** Trunk-based (org.md). Feature branch `feat/oxpilot-agent` → PR #203 → squash to `main`. All `ci.yml` jobs + `images.yml` `meta` are required checks on the head commit.

## Q3. Quality gates before merge

**Answer:** `commit-lint`, `rust-build` (build + nextest + clippy + fmt + doc), `rpc-sync`, `web-octane`, `web-browser`, `browser-coverage`, `route-coverage`, `dead-code`, `db-dialects`, `e2e-stack`, `coverage-weighted` (floor 0.70 composite), `images/meta`. Ruleset procedure in `docs/TESTING.md`.

## Q4. Artifact repositories

**Answer:** `ghcr.io` for CI images; no build artifact publishing required by this intent (self-hostable forge; deploy artifacts are Railway-managed per `.railway/railway.ts`).
