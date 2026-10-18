# API Documentation — Oxidean

All surfaces are served by `oxidean-api` (plus the in-process russh SSH
listener) except where noted. The typed RPC surface is the primary contract;
REST is a generated facade that re-enters `rpc::dispatch`.

## External API Surfaces

### 1. Typed JSON RPC — `POST /api/rpc` + `GET /api/rpc/ws`

Source: `crates/oxidean-api/src/rpc.rs` + `src/app.rs`.

- **306 procedures** — the `PROCEDURES` const (`rpc.rs:150-457`) is the
  manifest source of truth, mirrored 1:1 against `dispatch()` match arms
  (`procedures_list_matches_dispatch` test enforces parity).
- **Namespaces**: `system.*`, `auth.*` (incl. 2FA), `user.*`, `admin.*`,
  `org.*`, `repo.*` (~110 procedures: browse/tree/blob/commits/branches/
  files/lfs/mirror/protection/collaborators/invites/quota/actions/secrets/
  statuses), `issue.*`, `pull.*`, `release.*`, `webhook.*`, `label.*`,
  `packages.*`, `pat.*`, `oauthApp.*`, `oauth.*` device, `sshKey.*`,
  `gpgKey.*`, `email.*`, `notification.*`, `search.global`, `invites.*`.
- **Envelope**: `{procedure, input}` → `{ok, data}` |
  `{ok:false, error:{code,message}}` (`RpcRequest`/`RpcResponse`/`AppError`
  in `oxidean-core`); errors map to HTTP status in `rpc_status()`.
- **Wire gate**: required `Oxidean-RPC-Version` header checked against
  `WIRE_VERSION` (SemVer `0.1.0`; legacy `"1"` accepted via
  `wire_compatible` in `oxidean-core`).
- **WS transport**: `/api/rpc/ws` re-resolves credentials **per frame**
  (`app.rs:802-844`) — the precedent for any streaming VCS protocol.
- **PAT scope gate**: Bearer PAT callers pass
  `pat::bearer::authorize_rpc` → `oxidean_core::proc_classes`;
  **fail-closed** — unlisted procedures are `SessionOnly` (machine tokens
  cannot call them). A new `vcs.*` family MUST register classes.
- **Registration recipe** (per procedure): name in `PROCEDURES` +
  `dispatch()` arm + handler module + types in `oxidean-core` +
  `proc_classes` entry + `rpc_gen.rs` client template (+ optional
  `rest/spec.rs` `RouteDef`).

### 2. REST facade — `/api/v1/**`

Source: `crates/oxidean-api/src/rest/`.

- **65 `RouteDef` rows** across `rest/{repos,issues,pulls,orgs,users,
  releases,statuses,admin}.rs` (+meta); each translates to an RPC
  procedure and re-runs `rpc::dispatch` — no duplicated business logic.
- `spec.rs` is the single-source route table; `GET /api/v1/openapi.json` +
  Swagger UI `/api/v1/docs` (`utoipa-swagger-ui`); `docs/openapi.yaml`
  emitted by `openapi-gen`, drift-gated by `make openapi-sync-check`.
- New RPC procedures do NOT appear in REST/OpenAPI unless a `RouteDef`
  row is added by hand.

### 3. MCP endpoint — `POST /api/mcp`

Source: `crates/oxidean-api/src/mcp.rs` (~1,820 LOC, self-contained).

- JSON-RPC 2.0 streamable-HTTP in **JSON mode** (no SSE; GET→405; no
  `Mcp-Session-Id`). Methods: `initialize`, `ping`, `tools/list`,
  `tools/call`, `resources/list` (empty), `resources/templates/list`,
  `resources/read`, `prompts/list` (empty), notifications→202.
- **20 tools** (`tool_defs()` ~612): `repo_list`, `repo_get`, `repo_tree`,
  `issue_list`, `issue_get`, `issue_create`, `issue_comment`,
  `issue_list_comments`, `pull_list`, `pull_get`, `actions_list_runs`,
  `actions_get_run`, `workflow_templates_list`, `workflow_template_get`,
  `workflow_create`, `packages_list`, `search_repos`, `search_issues`,
  `search_pulls`, `search_code` — thin wrappers mapping to RPC procedures
  via `build_rpc_input` with `Access` gates (RepoRead/RepoWrite/Identity).
- Resource URI templates `oxidean://repo/{owner}/{name}/{blob,issue,pull}/…`.
- **Auth**: session cookie OR `Bearer` PAT (`oxidean_pat_`/`oxidean_fg_`)
  OR OAuth `oxidean_oat_`; instance gate `OXIDEAN_MCP_ENABLED` +
  `admin.mcp.*` DB override; domain denials surface as `isError` tool
  content. Protocol versions negotiated from `MCP_PROTOCOL_VERSIONS`
  (`oxidean-core`); RFC 8414/9728 well-known docs on the router.
- WebMCP: `oxidean-web` serves `/.well-known/{webmcp,mcp}`
  (`wellknown.rs`) and every page registers `tools/list` as
  `document.modelContext` tools bridging to `POST /api/mcp`.

### 4. Git Smart HTTP — `/{owner}/{repo}.git`

Source: `src/routes/git_smart_http.rs` + `src/git/http_backend.rs`.

- `GET …/info/refs`, `POST …/git-upload-pack`, `POST …/git-receive-pack`.
- **HTTP Basic, password = PAT/OAuth/job-token** — session cookies never
  authorize git (D-12). Private denials → 401 Basic; archived → 403.
- Spawns `git-http-backend` CGI (`GIT_PROJECT_ROOT=OXIDEAN_REPOS_DIR`,
  `GIT_PROTOCOL` v2 passthrough, `uploadpack.allowFilter`, `OXIDEAN_*`
  protection env incl. `oxidean-protection-hook` hook binary).
- **Greedy top-level route** — any new protocol sharing URL space
  (`/{owner}/{repo}.vcs` or `/vcs/…` prefix) must be checked against
  `is_api_git_path` (`proxy.rs:82`) and Traefik `PathRegexp`.

### 5. Git over SSH — in-process russh listener

Source: `crates/oxidean-api/src/ssh/` (`server.rs`, `pack.rs`).

- Spawned from `main.rs:83` (`OXIDEAN_SSH_ENABLED`, port
  `OXIDEAN_SSH_PORT` default 2222; Compose TCP publish, not Traefik).
- Username forced `git`; identity = `ssh_public_keys` fingerprint (full
  account) or per-repo `deploy_keys` fingerprint (read/write flag).
- Exec allowlist = `git-upload-pack`/`git-receive-pack` argv-spawned;
  env allowlist = `GIT_PROTOCOL` only. `SshAuthLimiter` per-process.

### 6. Git LFS — `/{owner}/{repo}.git/info/lfs/…`

Source: `src/{lfs,routes/git_lfs.rs}`.

- Batch API + basic transfer (streaming PUT ≤2 GiB, Range GET, optional
  verify). Storage = `OXIDEAN_LFS_DIR` OID-sharded `{ab}/{cd}/{oid}` +
  DB refcounts; PAT Basic only; per-repo enable flag (Admin); quotas +
  periodic GC job. **This is object-store precedent #1 for the intent.**

### 7. Package registries — `/v2` `/npm` `/generic`

Source: `src/packages/{oci,npm,generic}.rs` (`.nest()`ed routers).

- OCI distribution `/v2`, npm `/npm`, generic `/generic`; PAT Basic/Bearer
  (cookies ignored); content-addressed blobs
  `OXIDEAN_PACKAGES_DIR/{algo}/{aa}/{bb}/{digest}` + refcount dedup;
  quotas. **Object-store precedent #2.**

### 8. OAuth2 provider + auth routes

Source: `src/oauth/` + `app.rs` mount table.

- `/oauth/authorize`, `/oauth/token` (authorization_code + RFC 8628
  device_code), `/oauth/userinfo`, `/oauth/device/code`,
  `/.well-known/oauth-authorization-server` (RFC 8414) +
  `/.well-known/oauth-protected-resource` (RFC 9728). Mints
  `oxidean_oat_` access tokens (synthetic classic-PAT row downstream).
- `/api/auth/{workos,oidc}/{start,callback}`, `/api/auth/session-check`,
  `/api/auth/setup-check`.

### 9. Actions runner protocol — `/api/actions/*`

Source: `src/actions/runner_proto.rs`.

- register/declare/fetch_task/report; runner token Bearer; `prost` types
  generated from `proto/runner.proto` via `build.rs`.

### 10. Misc HTTP routes (`app.rs:268-407`)

`/health`, `POST|DELETE /api/user/avatar`, `/uploads/avatars/{file}`,
release-asset up/download, `/api/admin/templates` upload,
`/api/repos/{o}/{r}/raw/{ref}/{*path}` + `/archive/{*file}`,
`/api/repos/{o}/{r}/mirror/hook`, Atom feeds (`…/activity.atom`,
`releases.atom`, `/api/users/{u}/activity.atom`), CLI dist
`/cli/{install.sh,install.ps1,skill.md,latest,bin/*}`.

## Internal API Surfaces / Contracts

| Contract | Producer → Consumer | Mechanism |
|---|---|---|
| `GitBackend` trait | `oxidean-git` → `oxidean-api` (`Arc<dyn GitBackend>` on `AppState`) | `backend.rs:410-771`, ~40 async methods; `CliGitBackend` argv-spawns system git ≥2.5 (boot gate `version.rs`); `GixGitBackend` documented-unimplemented — **the VCS-engine seam** |
| `Database` façade | `oxidean-db` → all consumers | `Clone`, `skipped()` no-DB mode (`main.rs:52-54`); accept by value, tolerate `is_skipped` |
| `SessionService` / `EmailSender` | `oxidean-api` auth/email modules | trait seams for side effects |
| `RouteDef`/`ToolDef` tables | `rest/spec.rs`, `mcp.rs` → `rpc::dispatch` | single-source tables; registration recipe above |
| TS RPC client | `rpc-gen` bin → `packages/api-client/src/index.ts` | generated; `make rpc-sync-check` drift gate |
| OpenAPI doc | `openapi-gen` bin → `docs/openapi.yaml` + `/api/v1/openapi.json` | `make openapi-sync-check` |
| ox skill doc | `crates/oxidean-cli/skill/SKILL.md` → `/cli/skill.md` + `ox skill` | `include_dir` embed; `skill-sync` gate |
| Manifest gate | `system.manifest` ↔ `ox` `required_procedures` (`commands.rs:481`) | CLI refuses commands whose procedures are absent server-side |
| Runner proto | `proto/runner.proto` → api + runner | prost-build at compile time |
| PAT proc classes | `proc_classes.rs` → `pat::bearer::authorize_rpc` | fail-closed scope classification |
| WebMCP list | `apps/web/src/lib/webmcp.ts` ↔ `oxidean-web/src/wellknown.rs` | hand-synced, no gate |

## Auth & Credential Model (per transport)

| Surface | Accepted credentials | Notes |
|---|---|---|
| `/api/rpc` (+WS) | session cookie; Bearer PAT/OAT | PATs additionally scope-gated by `proc_classes` |
| `/api/mcp` | session cookie; Bearer PAT (`oxidean_pat_`/`oxidean_fg_`); OAuth `oxidean_oat_` | instance + admin gates |
| Smart HTTP `.git` + LFS | **Basic only** — PAT/OAuth/job-token as password | cookies never authorize git (D-12); `FailedAuthLimiter` |
| SSH :2222 | pubkey fingerprint (`ssh_public_keys` / `deploy_keys`) | russh in-process |
| `/v2`, `/npm`, `/generic` | PAT Basic/Bearer | cookies ignored |
| `/api/actions/*` | runner token Bearer | job tokens also accepted on git fetch |
| `/cli/*` | unauthenticated | distribution channel |

A new VCS protocol should **reuse `sha256` PAT lookup +
`FailedAuthLimiter`** (`pat/rate_limit.rs`) rather than mint a parallel
credential type.

## Error Conventions

RPC errors are typed codes on the envelope
(`{ok:false, error:{code,message}}`); `rpc_status()` maps code→HTTP status.
REST facade translates per `RouteDef`. MCP domain denials → `isError`
tool content (not protocol errors). `system.manifest` is the
machine-readable capability surface clients key on.
