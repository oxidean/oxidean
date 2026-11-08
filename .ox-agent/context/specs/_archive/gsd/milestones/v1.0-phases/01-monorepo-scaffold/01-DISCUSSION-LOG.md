# Phase 1: Monorepo Scaffold - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-09
**Phase:** 01-monorepo-scaffold
**Areas discussed:** A Workspace, B Package manager, C Compose, D RPC, E Scaffold proof surface

---

## A — Workspace layout

| Option | Description | Selected |
|--------|-------------|----------|
| apps/web + crates/* + packages/api-client | Split web, Rust crates, generated client | ✓ |
| (variants discussed) | crate split including oxidean-db | ✓ |

**User's choice:** `apps/web` + `crates/*` + `packages/api-client`; crates `oxidean-api`, `oxidean-core`, later `oxidean-db`; Turborepo + Cargo + root Makefile
**Notes:** Uniform DB adapter crate expected even though multi-dialect proof is Phase 2

---

## B — Package manager

| Option | Description | Selected |
|--------|-------------|----------|
| Bun via Corepack | Preferred JS toolchain | ✓ |
| Corepack pnpm + Turborepo | Fallback | ✓ (fallback) |

**User's choice:** Bun via Corepack preferred; else Corepack pnpm + Turborepo
**Notes:** —

---

## C — Compose topology

| Option | Description | Selected |
|--------|-------------|----------|
| web+api+postgres default | MySQL profile; SQLite via env | ✓ |
| Traefik | Reverse proxy in Compose | ✓ |
| Vite/TanStack proxy | For local `make dev` | ✓ |

**User's choice:** Default postgres; MySQL profile; SQLite env/file; Traefik; Vite proxy for `make dev`
**Notes:** `oxidean-db` as uniform adapter

---

## D — RPC transport

### Transport

| Option | Description | Selected |
|--------|-------------|----------|
| HTTP only | — | |
| HTTP + WebSocket | Both mounts | ✓ |
| WS-heavy | — | |

**User's choice:** HTTP + WebSocket

### Mount paths

| Option | Description | Selected |
|--------|-------------|----------|
| `/api/rpc` + `/api/rpc/ws` | Nested under /api | ✓ |
| (other path layouts) | — | |

**User's choice:** `/api/rpc` + `/api/rpc/ws`

### Scaffold surface

| Option | Description | Selected |
|--------|-------------|----------|
| Health only | — | |
| Health + echo | Typed in/out proof | ✓ |
| Health + auth stub | — | |

**User's choice:** Health + echo

### Codegen sync

| Option | Description | Selected |
|--------|-------------|----------|
| Watch only | — | |
| Explicit only | — | |
| Both + CI sync | Watch in make dev + explicit + CI | ✓ |

**User's choice:** 3 — watch + explicit regen; CI asserts sync
**Notes:** User emphasized CI-level type sync

### Error envelope

| Option | Description | Selected |
|--------|-------------|----------|
| Typed app errors | code + message + optional data | ✓ |
| HTTP-ish only | — | |
| Stack defaults | — | |

**User's choice:** Typed app errors

### Procedure naming

| Option | Description | Selected |
|--------|-------------|----------|
| Flat names | — | |
| Dotted namespaces | system.health, system.echo | ✓ |
| Router groups / flat export | — | |

**User's choice:** Dotted namespaces from day one

### WS Phase 1

| Option | Description | Selected |
|--------|-------------|----------|
| Transport only | Same procs over WS | ✓ |
| Subscription stub | — | |
| Defer WS demos | — | |

**User's choice:** Transport only

### API versioning

| Option | Description | Selected |
|--------|-------------|----------|
| Unversioned | — | |
| Path version | — | |
| Header version | Oxidean-RPC-Version | ✓ (lightweight) |

**User's choice:** Initially preferred header versioning; after challenge, confirmed lightweight header `1` + reject mismatch (paths unversioned)
**Notes:** Challenge noted header ceremony vs unversioned for monorepo-only clients; user kept header for scale

### Browser credentials / CORS

| Option | Description | Selected |
|--------|-------------|----------|
| Same-origin assumed | — | |
| Explicit CORS now | ✓ | ✓ |
| No credentials yet | — | |

**User's choice:** Explicit CORS

### CORS allowlist

| Option | Description | Selected |
|--------|-------------|----------|
| Env list always | — | |
| Dev-open / prod-strict | + env list for prod | ✓ |
| Fixed scaffold defaults | — | |

**User's choice:** 2 with option 1 for prod — any origin in dev; `OXIDEAN_CORS_ORIGINS` required in prod/Compose

### api-client surface

| Option | Description | Selected |
|--------|-------------|----------|
| Vanilla only | — | |
| Custom React hooks | — | |
| TanStack Query helpers | No custom hooks package | ✓ |

**User's choice:** Confirmed TanStack Query helpers (user asked whether custom hooks are needed — answered no)
**Notes:** —

---

## E — Scaffold proof surface

### Product shape (free text)

**User's choice:** Homepage = marketing landing; header = public search, branding, auth; separate status page with status report + history
**Notes:** History/auth gate are longer-term; Phase 1 scoped down in E1b

### Phase 1 slice

| Option | Description | Selected |
|--------|-------------|----------|
| Shell + stubs + health/echo UI | — | |
| Landing + live status (health only) | ✓ | ✓ |
| Status history lite | — | |
| Defer landing | — | |

**User's choice:** 2 — landing + header placeholders; `/status` health-focused; no echo UI; no history persistence

### Echo proof

| Option | Description | Selected |
|--------|-------------|----------|
| Tests/CI only | ✓ | ✓ |
| Hidden/dev route | — | |
| Also on /status | — | |

**User's choice:** 1 — tests/CI only

### /status access

| Option | Description | Selected |
|--------|-------------|----------|
| Public | ✓ | ✓ |
| Footer-only / unlabeled | — | |
| Stub sign-in required | — | |

**User's choice:** 1 — public in Phase 1

### Landing depth

| Option | Description | Selected |
|--------|-------------|----------|
| Structural shell | — | |
| Credible marketing draft | ✓ | ✓ |
| Header-only | — | |

**User's choice:** 2

### /status chrome link

| Option | Description | Selected |
|--------|-------------|----------|
| Footer link | Defaulted on “Continue working” | ✓ |
| Header secondary | — | |
| Both | — | |
| URL-only | — | |

**User's choice:** Not answered explicitly; **Claude defaulted to footer link** when user said “Continue working”
**Notes:** Documented as D-26 with note that it can be changed

---

## Claude's Discretion

- `/status` footer link (E5 default)
- Exact RPC framework versions and HTTP framework choice within locked behaviors
- Visual polish within “credible draft”
- Compose healthcheck details
- Makefile/turbo naming

## Deferred Ideas

- Status history persistence and auth-gated status
- Real search/auth header behavior
- Phase 3 brand/theme
- Phase 2 multi-DB proof
- WS subscriptions
- Multi-version RPC routing
- OAuth (out of v1)
