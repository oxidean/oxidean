# Business Overview — Oxidean

## Purpose

Oxidean is a **self-hostable code forge** — a single product that provides git
hosting, code review, issue tracking, releases, package registries, and CI
("Actions") under one roof. One codebase serves both deployment models:

- **Oxidean Cloud** — the hosted multi-tenant offering (deployed to Railway;
  staging auto-deploys on merge, production promotes by SHA behind a manual
  environment gate).
- **Self-hosted** — operators run the same binaries/images (Docker Compose,
  `.deb`/`.rpm`/`.apk`/pacman packages, or cloud gateway topology) on their own
  infrastructure.

Canonical product docs: `README.md`, `docs/ARCHITECTURE.md`, `AGENTS.md`.

## Business Domain

Developer tooling / DevOps infrastructure. Oxidean competes in the
GitHub/GitLab/Gitea category and differentiates on being a single integrated
codebase for both SaaS and self-host, with first-class AI-agent surfaces (MCP
server, browser WebMCP, an embedded `ox` CLI skill for coding agents).

## Key Functionality

Inferred from the API surface (`crates/oxidean-api`), web app
(`apps/web`), and documentation:

| Capability | Where it lives |
|---|---|
| Git repository hosting | Smart HTTP `/{owner}/{repo}.git`, in-process russh SSH (`OXIDEAN_SSH_PORT`), LFS `info/lfs` batch — `crates/oxidean-api`, `crates/oxidean-git` |
| Issues & pull requests | REST `/api/v1` + typed RPC procedures; UI under `apps/web/src/pages` |
| Organizations & users | `orgs`/`users`/`admin` RouteDef tables, auth (OAuth/OIDC/WorkOS, device auth) |
| Releases & release assets | `/api/v1` releases routes, release-asset serving |
| Package registries | OCI `/v2`, npm `/npm`, generic `/generic` path prefixes |
| CI ("Actions") | Self-hosted `oxidean-runner` binary + `/api/actions/**` runner protocol (`proto/runner.proto`, prost-build) |
| Identity provider features | `/oauth/{authorize,token,userinfo,device/code}`, `/api/auth/{workos,oidc}/{start,callback}`, well-known discovery |
| AI-agent integration | MCP endpoint `POST /api/mcp` (18 tools), browser WebMCP, `ox` CLI with embedded agent skill (`skill/SKILL.md` → `/cli/skill.md`, `ox skill`) |
| CLI distribution | Unauthenticated `/cli/{install.sh,install.ps1,skill.md,latest,bin/*}` channel with self-update in `ox` |
| Administration | admin routes, factory reset / setup bootstrap, status page (`apps/web` `/status` renders `system.health`) |
| Scaffold/templates | Embedded `stack-presets` and template packs served to new repos (`include_dir` assets in `oxidean-api`) |

## Stakeholders / Users

- **Development teams** self-hosting their forge or using Oxidean Cloud.
- **Operators** installing via Docker images, distro packages, or Compose —
  served by `make up`, `docker-compose*.yml`, `deploy/{cloud,traefik}`.
- **AI coding agents** — first-class consumers via MCP/WebMCP and the `ox` CLI
  (`ox skill` exposes an embedded `SKILL.md` the agent installs).
- **Maintainers** — this repo's own release engineering (`ox-cli.yml`,
  `publish-runner.yml`, `production-deploy.yml`) is itself a stakeholder for the
  active intent (`261014-semver-rollout`).

## Intent Relevance (SemVer rollout)

Today every versioned surface funnels into **one** workspace version
(`Cargo.toml` `[workspace.package] version = "0.1.0"`, inherited by all 7 crates)
plus two `"0.1.0"` `package.json` fields. There are **no `v*` semver tags**, no
changelog files, and no commit-message enforcement — yet a complete
tag-triggered release pipeline already exists (`ox-cli.yml` on `v*.*.*` tags →
10-target build matrix → nfpm packages → `gh release create --generate-notes` →
rolling `ox-dist`). The business goal of the active intent is per-component
SemVer plus automated changelog/release wiring on top of that machinery.
The full version-surface inventory is recorded in `api-documentation.md`
(wire-contract versions) and `component-inventory.md` (per-component version
sources); the release pipeline detail is in `code-quality-assessment.md`.
