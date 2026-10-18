# Business Overview — Oxidean

## Purpose

Oxidean is a **self-hostable code forge** — a single product that provides git
hosting, code review, issue tracking, organizations, LFS, releases, package
registries, and CI ("Actions") under one roof. One codebase serves both
deployment models:

- **Oxidean Cloud** — the hosted multi-tenant offering (Railway IaC in
  `.railway/railway.ts`, Caddy file-gateway in `deploy/cloud/`).
- **Self-hosted** — operators run the same binaries/images (Docker Compose
  `docker-compose*.yml`, nfpm `.deb`/`.rpm`/`.apk` packages in
  `packaging/`, Traefik reference in `deploy/traefik/`).

Canonical product docs: `README.md`, `docs/ARCHITECTURE.md`, `AGENTS.md`.

## Business Domain

Developer tooling / DevOps infrastructure. Oxidean competes in the
GitHub/GitLab/Gitea category and differentiates on being a single integrated
codebase for both SaaS and self-host, with **first-class AI-agent surfaces**:
an instance MCP server (`POST /api/mcp`, 20 tools), browser WebMCP
(`/.well-known/webmcp`), and the `ox` CLI with an embedded agent skill
(`ox skill`, `/cli/skill.md`).

## Key Functionality

Inferred from the API surface (`crates/oxidean-api`), web app (`apps/web`),
and documentation:

| Capability | Where it lives |
|---|---|
| Git repository hosting | Smart HTTP `/{owner}/{repo}.git` (upload/receive-pack via `git-http-backend` CGI), in-process russh SSH (`OXIDEAN_SSH_PORT`, default 2222), LFS `info/lfs` batch — `crates/oxidean-api`, `crates/oxidean-git` |
| Issues & pull requests | Typed RPC `issue.*`/`pull.*` + REST `/api/v1`; UI under `apps/web` |
| Organizations & users | `org.*`/`user.*`/`admin.*` procedures; auth = session cookie, WorkOS, OIDC, OAuth2 provider (incl. RFC 8628 device flow), 2FA/WebAuthn |
| Releases & assets | `release.*` procedures, release-asset up/download routes |
| Package registries | OCI `/v2`, npm `/npm`, generic `/generic` — content-addressed blobs + refcounts + quotas |
| CI ("Actions") | `oxidean-runner` binary + `/api/actions/*` runner protocol (`proto/runner.proto`, prost-build) |
| Identity provider | `/oauth/{authorize,token,userinfo,device/code}` + RFC 8414/9728 well-known docs; mints `oxidean_oat_` tokens |
| AI-agent integration | MCP endpoint (20 tools), WebMCP browser bridge, `ox` CLI + embedded skill |
| Personal access tokens | `pat.*` procedures; PATs `oxidean_pat_`/fine-grained `oxidean_fg_` scope-gated per-procedure via `proc_classes` (fail-closed `SessionOnly` default) |
| Mirrors & protection | `mirror/*` sync engine, branch/tag protection + `oxidean-protection-hook` bin |
| Search | `search.global` + `search_*` MCP tools |
| CLI distribution | Unauthenticated `/cli/{install.sh,install.ps1,skill.md,latest,bin/*}` + `ox self-update` |
| Administration | `admin.*` procedures, setup/factory-reset bootstrap, template packs, `system.health` → web `/status` |

## Stakeholders / Users

- **Development teams** self-hosting their forge or using Oxidean Cloud.
- **Operators** installing via Docker images, distro packages, or Compose.
- **AI coding agents** — first-class consumers via MCP/WebMCP and `ox`;
  the audience the active intent expands.
- **Maintainers** — the repo's own release engineering (per-component SemVer
  via `versioning.toml`, `release.yml`, `ox-cli.yml`).

## Intent Relevance (agentic VCS protocol)

The active intent (`261018-agentic-vcs-protocol`) adds a **side-by-side,
non-git source-control protocol** — Scion-inspired: server-authoritative
changesets, content-addressed objects, want-based fetch, sparse/virtual
checkout — served over MCP + CLI + RPC surfaces and living in a **new
dedicated Rust crate**. It targets agentic coding throughput: a protocol
shaped for machine writers/readers rather than human git workflows, running
alongside (not replacing) the existing git path.

Business context that matters:

- The forge already treats AI agents as first-class customers (MCP, WebMCP,
  `ox skill`), so an agent-native VCS is a product-direction continuation,
  not a pivot.
- Content-addressed object storage has two in-house precedents — LFS
  (`OXIDEAN_LFS_DIR`, OID-sharded + refcount + quota + GC job) and package
  registries (`OXIDEAN_PACKAGES_DIR`, digest-sharded) — the new object store
  clones that operating pattern.
- PAT/OAuth machine auth, per-procedure PAT scoping (`proc_classes`),
  per-repo enable flags, quotas, and background GC jobs are all existing
  product behaviors the new protocol inherits expectations from.
- Edge parity is a hard product requirement: every new path prefix must
  reach browsers/clients through `oxidean-web`'s `API_PREFIXES`, Traefik
  labels, and the cloud `Caddyfile` — three-place sync, each tested.

The detailed seam analysis (where the new crate plugs in) is in
`architecture.md` § Key Design Decisions and `api-documentation.md`.
