# Architecture — Oxidean monorepo

## Architecture Analysis

### System Overview

Oxidean is a **modular monolith delivered as a monorepo**: one Rust Cargo
workspace (`crates/*`, resolver 2) and one Bun/Turborepo workspace
(`apps/*`, `packages/*`). The runtime system is two long-lived Rust
binaries plus a static SPA bundle:

- **`oxidean-web`** (Rust) serves the Astro-built static shells, injects
  per-request theme/title/session metas, gates protected routes, exposes
  `/health` + well-known endpoints, and reverse-proxies API prefixes via
  `OXIDEAN_API_ORIGIN`.
- **`oxidean-api`** (Rust/Axum) is the single backend: typed JSON-RPC
  (283 procedures), a `/api/v1` REST facade, MCP endpoint, OAuth/OIDC,
  git Smart HTTP + LFS + in-process russh SSH, package registries
  (OCI/npm/generic), the `/cli/*` ox-distribution channel, and the Actions
  runner protocol.
- **`@oxidean/web`** (Astro + Octane `.tsrx` islands, 213 `.tsrx` files) is
  the SPA; **`@oxidean/api-client`** is the generated typed RPC client it
  consumes.

Out-of-process components: **`ox`** (`oxidean-cli`, companion CLI with
self-update) and **`oxidean-runner`** (self-hosted Actions runner, published
to GHCR). `oxidean-core` is the shared domain/RPC-envelope crate;
`oxidean-db` is the sole crate allowed to branch on DB dialect
(Postgres/MySQL/SQLite via sqlx); `oxidean-git` wraps the system `git`
binary behind a `GitBackend` trait (`CliGitBackend`, git ≥ 2.5 boot gate in
`version.rs`).

### Architectural Style

**Modular monolith** (single deployable API binary with explicit internal
crate boundaries) + **static-shell/islands frontend** + **codegen-mediated
client contract**. Evidence:

- All server logic compiles into one `oxidean-api` binary (lib + 4 bins in
  one crate); crate boundaries are enforced by path dependencies, not
  network hops.
- Dialect isolation is a hard boundary: SQL lives only in `oxidean-db`
  (AGENTS.md hard boundary; 46 postgres migrations mirrored to mysql/sqlite).
- The API contract is generated, not hand-maintained: `rpc-gen` emits
  `packages/api-client/src/index.ts` (5427 lines) and `openapi-gen` emits
  `docs/openapi.yaml`; CI drift gates (`rpc-sync-check`,
  `openapi-sync-check`, `skill-sync-check`, `route-sync`) make the
  generated artifacts read-only.

### Component Relationships

```mermaid
graph TD
    subgraph Clients
        Browser["Browser (Octane SPA)"]
        Ox["ox CLI (oxidean-cli)"]
        Agent["AI agents (MCP)"]
        Runner["oxidean-runner"]
        GitC["git client (HTTP/SSH/LFS)"]
    end

    subgraph "oxidean-web (Rust binary)"
        Web["static shell server<br/>meta injection · auth gates<br/>well-known · /health"]
        Proxy["API prefix proxy<br/>(OXIDEAN_API_ORIGIN)"]
    end

    subgraph "oxidean-api (Rust binary)"
        RPC["/api/rpc typed RPC<br/>283 procedures"]
        REST["/api/v1 REST facade<br/>+ openapi.json/Swagger"]
        MCP["/api/mcp MCP server<br/>18 tools"]
        Oauth["OAuth/OIDC + device auth"]
        GitS["git Smart HTTP · LFS · russh SSH"]
        Reg["registries /v2 · /npm · /generic"]
        CliD["/cli/* ox distribution"]
        Act["/api/actions/** runner protocol"]
    end

    Core["oxidean-core<br/>domain types · RPC envelope<br/>RPC_PROTOCOL_VERSION"]
    DB["oxidean-db<br/>sqlx multi-dialect<br/>Postgres/MySQL/SQLite"]
    GitB["oxidean-git<br/>GitBackend → CliGitBackend"]
    DBMS[("Postgres / MySQL / SQLite")]
    FS[("git repositories on disk")]

    Browser -->|static shell| Web
    Browser -->|"RPC via @oxidean/api-client"| Proxy
    Proxy --> RPC
    Ox -->|typed RPC + UA ox/ver| RPC
    Ox -->|"self-update check"| CliD
    Agent --> MCP
    Agent -->|WebMCP| Browser
    Runner --> Act
    GitC --> GitS

    RPC --> Core
    REST --> Core
    MCP --> Core
    RPC --> DB
    RPC --> GitB
    DB --> DBMS
    GitB --> FS
```

### Data Flow

Primary request path: `Browser → oxidean-web (shell) → /api/* proxy →
oxidean-api → {oxidean-db → DBMS | oxidean-git → repos on disk}`.
Domain types and the RPC envelope are shared through `oxidean-core`, which
everything else depends on (highest fan-out node in the workspace — see
`dependencies.md`).

Version-bearing responses flow through `env!("CARGO_PKG_VERSION")` embeds:
`system.manifest` (server_version, min_cli_version, procedures map),
`system.health` (→ web `/status` page), MCP `serverInfo`, and `/cli/latest`
(see `api-documentation.md` § Version & Contract Surfaces).

### Key Design Decisions

| Decision | Evidence | Implication |
|---|---|---|
| Typed JSON-RPC is the primary contract; REST is a generated facade | `rpc.rs` PROCEDURES (283) + `rest/` RouteDef tables → `docs/openapi.yaml` | One authoritative procedure list; REST/OpenAPI drift is CI-gated |
| Generated TS client, never hand-edited | `rpc_gen.rs` → `packages/api-client/src/index.ts`; `make rpc-sync-check` | Wire changes are Rust-first; client drift fails CI |
| Single workspace version `0.1.0` for all crates | `Cargo.toml` `[workspace.package]`, `version.workspace = true` in every crate | Every `CARGO_PKG_VERSION` embed currently reports the same string — per-component SemVer (the active intent) must assign semantics per surface |
| Dialect branching confined to `oxidean-db` | AGENTS.md hard boundary; `migrations/{postgres,mysql,sqlite}` | API never branches on dialect; test matrix covers 3 dialects |
| `oxidean-web` is the edge when `OXIDEAN_API_ORIGIN` set; else edge is in front | `main.rs` behind-proxy handling, `OXIDEAN_WEB_BEHIND_PROXY` | Same SPA ships dev (Compose/Traefik) and prod (Railway/gateway) topologies |
| CLI binaries distributed via the API itself | `cli_dist.rs`, `install.{sh,ps1}`, `ox self-update` (version OR sha256 → swap) | `/cli/latest` conflates api version with cli binaries — flagged in `code-quality-assessment.md` |
| Assets embedded via `include_dir` | `assets/cli/*`, `stack-presets/**`, `skill/SKILL.md` | Binaries are self-contained; preset manifests carry scaffold-only versions (`0.0.0`/`1.0.0`) that are NOT repo versions |

### Improvement Opportunities

Cross-reference `code-quality-assessment.md` § Technical Debt for the full
list; architecturally significant items:

- `/cli/latest` reports `oxidean-api`'s version while serving
  `oxidean-cli` binaries — correct today only because all crates share
  `0.1.0`; per-component versioning forces a decision (server vs CLI vs
  dist-channel semantics).
- Dependency-aware change detection exists as precedent
  (`.railway/railway.ts` `watchPatterns` lists transitive crate deps) but
  `ox-cli.yml` path filters omit `crates/oxidean-core/**`.
- `OXIDEAN_RPC_VERSION` env var is dead config — documented but never read;
  the wire version is compile-time.
- MCP protocol-version lists are hand-synced between
  `apps/web/src/lib/webmcp.ts` and `crates/oxidean-web/src/wellknown.rs`
  with no CI gate.

## Interaction Diagrams

How representative business transactions execute across components.

### 1. Browser session: load a protected page and call the API

```mermaid
sequenceDiagram
    participant B as Browser
    participant W as oxidean-web
    participant A as oxidean-api
    participant D as oxidean-db

    B->>W: GET /owner/repo (protected route)
    W->>W: session stamp / auth gate (cookie + session meta)
    alt authenticated
        W-->>B: Astro static shell + injected metas (theme/title/session)
        B->>W: POST /api/rpc (repos.get, Oxidean-RPC-Version header)
        W->>A: proxy via OXIDEAN_API_ORIGIN
        A->>A: check_version_header → dispatch arm
        A->>D: sqlx query (dialect-specific inside oxidean-db only)
        D-->>A: rows → oxidean-core types
        A-->>B: RpcResponse (typed JSON)
    else unauthenticated
        W-->>B: redirect to login
    end
```

### 2. `ox` CLI: authenticated call + self-update

```mermaid
sequenceDiagram
    participant U as User
    participant Ox as ox CLI (oxidean-cli)
    participant A as oxidean-api
    participant GH as GitHub Releases (ox-dist)

    U->>Ox: ox repos list
    Ox->>A: POST /api/rpc (token creds; User-Agent: ox/{ver}; Oxidean-RPC-Version)
    A->>A: check_version_header vs RPC_PROTOCOL_VERSION
    A-->>Ox: RpcResponse
    Ox->>A: GET /cli/latest (during self-update check)
    A-->>Ox: { version: CARGO_PKG_VERSION(api) }
    Note over Ox: needs_binary_update(current, latest,<br/>local_sha, expected_sha) —<br/>version ≠ OR sha256 ≠ → swap
    Ox->>GH: download new binary (ox-dist rolling release / tag)
    Ox->>Ox: verify sha256 → swap binary
```

### 3. Git push over Smart HTTP

```mermaid
sequenceDiagram
    participant G as git client
    participant A as oxidean-api
    participant GB as oxidean-git
    participant FS as Repo storage (disk)
    participant D as oxidean-db

    G->>A: POST /{owner}/{repo}.git/git-receive-pack
    A->>A: auth + repo permission check
    A->>GB: GitBackend → CliGitBackend (system git ≥ 2.5, gated at boot)
    GB->>FS: spawn git receive-pack
    FS-->>G: pack result (via A)
    A->>D: update repo metadata / refs bookkeeping
```

### 4. Actions runner job loop

```mermaid
sequenceDiagram
    participant R as oxidean-runner (GHCR image)
    participant A as oxidean-api (/api/actions/**)
    participant D as oxidean-db

    loop poll
        R->>A: runner protocol request (custom JSON / prost proto)
        A->>D: claim queued job
        A-->>R: job assignment
        R->>R: execute steps (DOCKER_CLI_VERSION-pinned docker)
        R->>A: logs + status updates
        A->>D: persist run state
    end
```

### 5. Release pipeline (existing machinery the SemVer intent wires into)

```mermaid
flowchart LR
    Dev["push to main / tag v*.*.*"] --> CI[ox-cli.yml]
    CI --> Build["build matrix<br/>10 targets + FreeBSD"]
    Build --> Pkg["nfpm packages<br/>deb/rpm/apk/pkg.tar.zst<br/>version: tag or 0.0.0.dev.{run_id}"]
    Pkg --> Dist["publish-dist<br/>rolling ox-dist release"]
    Pkg --> Rel["release (tag only)<br/>gh release create --generate-notes"]
    Dist --> Stg["staging-api redeploy<br/>(Railway)"]
    Dev --> Pub["publish-runner.yml<br/>GHCR oxidean-runner<br/>branch/tag/sha/latest tags"]
    Dev --> Rail["Railway api service<br/>watchPatterns incl. transitive deps"]
    Manual["workflow_dispatch"] --> Prod["production-deploy.yml<br/>promote-by-SHA (env gate)"]
```

Notes: no `v*` tag has ever been cut (`git tag -l` → only rolling
`ox-dist`); release notes come from `--generate-notes` only — there is no
changelog infrastructure (see `code-quality-assessment.md`).

### 6. Codegen contract pipeline (build-time)

```mermaid
flowchart LR
    R["oxidean-api<br/>PROCEDURES + types"] -->|cargo run --bin rpc-gen| Gen["packages/api-client/src/index.ts<br/>RPC_VERSION, typed methods"]
    R -->|cargo run --bin openapi-gen| OAS["docs/openapi.yaml<br/>info.version '1'"]
    Gen --> Web["@oxidean/web build (Astro + Octane)"]
    CI2["CI: rpc-sync-check · openapi-sync-check<br/>skill-sync-check · route-sync"] -. drift fails build .-> Gen
```
