# API Documentation — Oxidean

All surfaces are served by `oxidean-api` except where noted. The typed RPC
surface is the primary contract; REST is a generated facade over it.

## External API Surfaces

### 1. Typed JSON RPC — `POST /api/rpc` + WS `/api/rpc/ws`

Source: `crates/oxidean-api/src/rpc.rs`.

- **283 procedures** in the `PROCEDURES` const (`rpc.rs:127`), mirrored 1:1
  against ~285 dispatch match arms — parity asserted by tests
  (`rpc.rs:1702-1712`), so the procedure list is self-enforcing.
- **Envelope**: `RpcRequest` / `RpcResponse` (schemars types in
  `oxidean-core`).
- **Version gate**: `Oxidean-RPC-Version` header checked by
  `check_version_header` (`rpc.rs:105-117`) against compile-time
  `RPC_PROTOCOL_VERSION`; mismatch → `rpc.version_mismatch` error.
- **Meta procedures**: `system.manifest` (`rpc.rs:417`) returns
  `server_version`, `min_cli_version`, and the full procedure map;
  `system.health` (`rpc.rs:468`) returns `version` (rendered by
  `apps/web/src/routes/status.tsrx`).

### 2. REST facade — `/api/v1/**`

Source: `crates/oxidean-api/src/rest/` (`mod.rs` + per-domain modules).

- ~69 route literals across `RouteDef` tables: `meta`, `users`, `orgs`,
  `repos`, `issues`, `pulls`, `releases`, `statuses`, `admin`.
- OpenAPI: `rest/spec.rs` generates `docs/openapi.yaml`
  (`info.version: '1'`, `spec.rs:335`) and serves live
  `/api/v1/openapi.json` + Swagger UI (`rest/openapi.rs`,
  `utoipa-swagger-ui`).

### 3. MCP endpoint — `POST /api/mcp`

Source: `crates/oxidean-api/src/mcp.rs`.

- JSON-RPC methods: `initialize`, `tools/list`, `tools/call`,
  `resources/list`, `prompts/list`; **18 tool definitions**.
- **Date-versioned protocol**: negotiates the last shared version from
  `MCP_PROTOCOL_VERSIONS = ["2024-11-05","2025-03-26","2025-06-18"]`
  (`mcp.rs:~318`). `serverInfo.version` = `CARGO_PKG_VERSION`.
- The same list is duplicated in `crates/oxidean-web/src/wellknown.rs:15`
  (browser WebMCP advertisement) and `apps/web/src/lib/webmcp.ts:2` —
  comment-flagged manual sync, no CI gate (debt — see
  `code-quality-assessment.md`).
- Client side: `crates/oxidean-cli/src/mcp.rs` sends `"2025-06-18"` on
  initialize and embeds `clientInfo.version` (`mcp.rs:1273`).

### 4. CLI distribution — `/cli/*`

Source: `crates/oxidean-api/src/routes/cli_dist.rs`. Unauthenticated.

- `GET /cli/install.sh`, `/cli/install.ps1` (from `assets/cli/`),
  `/cli/skill.md` (from `oxidean-cli/skill/SKILL.md`),
  `/cli/latest` → `{ "version": <api CARGO_PKG_VERSION> }`
  (`cli_dist.rs:131`), `/cli/bin/{*file}` binary downloads.
- `SOURCE_COMMIT`/`dist_commit` file carries git-SHA provenance of the
  staged binaries (`cli_dist.rs:110-124`).
- **Semantic gap**: `/cli/latest` reports the *API's* version but describes
  *CLI* binaries — see `code-quality-assessment.md` § Technical Debt.

### 5. OAuth/OIDC + auth

`/oauth/{authorize,token,userinfo,device/code}`,
`/api/auth/{workos,oidc}/{start,callback}`, well-known discovery (served by
`oxidean-web` `wellknown.rs` + api). WorkOS + generic OIDC + device-code
flows; Mailpit/OIDC/Resend/WorkOS stubs for dev-auth Compose stack.

### 6. Git protocols

- Smart HTTP `/{owner}/{repo}.git` (info/refs + upload/receive-pack).
- LFS `info/lfs` batch API.
- SSH via in-process russh server (`OXIDEAN_SSH_PORT`).

### 7. Package registries

Path prefixes `/v2` (OCI distribution), `/npm`, `/generic`.

### 8. Actions runner protocol — `/api/actions/**`

Custom JSON protocol plus `proto/runner.proto` (prost-build in `build.rs`);
consumed by `oxidean-runner`.

### 9. Meta / misc

`/health` on both api and web (gated by `Oxidean-Health-Probe` header),
Atom feeds, avatar/uploads, template packs, release assets.

## Internal API Surfaces / Contracts

| Contract | Producer → Consumer | Mechanism |
|---|---|---|
| TS RPC client | `oxidean-api --bin rpc-gen` → `packages/api-client/src/index.ts` | generated; exports `RPC_VERSION = 1` + `RPC_VERSION_HEADER`; drift-gated by `make rpc-sync-check` |
| OpenAPI doc | `oxidean-api --bin openapi-gen` → `docs/openapi.yaml` + `/api/v1/openapi.json` | generated; `make openapi-sync-check` |
| ox skill doc | `crates/oxidean-cli/skill/SKILL.md` → `/cli/skill.md` + `ox skill` | `include_dir` embed; `make skill-sync-check`; frontmatter claims `compatibility: ox 0.1+` |
| Web route corpus | `apps/web/src/pages/**` ↔ `oxidean-web` `SHELLS` | `scripts/check-route-sync.ts` |
| Runner protocol | `proto/runner.proto` → runner + api | prost-build at api build time |
| Manifest gate | `system.manifest` ↔ `ox` (`manifest::compat_notices`) | `MIN_CLI_VERSION` vs running ox version; tested in `manifest_gate.rs`, `rpc_http.rs` |

## Version & Contract Surfaces (intent-critical)

Every wire-visible version constant, distinct from package versions:

| Surface | Location | Value today | Notes |
|---|---|---|---|
| `RPC_PROTOCOL_VERSION` | `oxidean-core/src/lib.rs:57` | `1` (u32, not semver) | mirrored to generated client `index.ts:3` (`RPC_VERSION`) |
| `Oxidean-RPC-Version` header | sent by client + `cli/src/rpc.rs:111`; enforced `rpc.rs:105-117` | `1` | compile-time; `OXIDEAN_RPC_VERSION` env var is dead config (`.env.example:19`, `docker-compose.yml:63`) |
| `MIN_CLI_VERSION` | `oxidean-api/src/rpc.rs:121` | `"0.1.0"` | hand-bumped; asserts at `rpc.rs:1722`, `tests/rpc_http.rs:122`, `cli tests/manifest_gate.rs:36` |
| OpenAPI `info.version` | `rest/spec.rs:335` → `docs/openapi.yaml:5` | `'1'` | |
| MCP protocol versions | `webmcp.ts:2`, `wellknown.rs:15`, negotiated `mcp.rs:~318` | `2024-11-05 / 2025-03-26 / 2025-06-18` | date-versioned; list duplicated in 2 files |
| `skills-lock.json` schema | repo root | `"version": 1` + per-skill `computedHash` pins | vendored `.agents/skills/*` |
| Service-worker cache | `apps/web/public/sw.js:15` | `oxidean-shell-__OXIDEAN_SW_BUILD__` | commit-SHA fingerprint, not semver |
| Provenance | `SOURCE_COMMIT`/`dist_commit`, `OXIDEAN_SOURCE_COMMIT`, `RAILWAY_GIT_COMMIT_SHA` | git SHA | real binary provenance for `/cli/*` artifacts |

## `env!("CARGO_PKG_VERSION")` Embed Sites

All currently `"0.1.0"`; after per-component versioning each resolves to its
own crate's version. Owning component in parentheses:

- `oxidean-cli`: `main.rs:27` (`ox --version`), `commands.rs:834-835`
  (version report text + `json.data.version`), `commands.rs:801`
  (→ `manifest::compat_notices`), `http.rs:51,93` + `rpc.rs:112` +
  `self_update.rs:68` (`User-Agent: ox/{v}`), `mcp.rs:1273`
  (`clientInfo.version`), `self_update.rs:171` (self-update `current`).
- `oxidean-api`: `rpc.rs:417` (`system.manifest` → `server_version`),
  `rpc.rs:468` (`system.health` → `version`), `mcp.rs:332`
  (`serverInfo.version`), `routes/cli_dist.rs:131` (`/cli/latest` →
  `version` — **describes CLI binaries**, see debt).
- Tests embedding it: `oxidean-cli/tests/{self_update_http.rs:38,95,
  commands_e2e.rs:540}`.

## Error Conventions

RPC errors are typed codes on the envelope (e.g. `rpc.version_mismatch`);
REST facade translates to HTTP status per `RouteDef`. `system.manifest` is
the machine-readable capability/version surface clients should key on.
