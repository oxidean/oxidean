# Phase 8: Git HTTPS & PATs - Context

**Gathered:** 2026-09-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Users authenticate git over **HTTPS** with **personal access tokens** (never account passwords) and manage those tokens in the UI. Delivers GIT-02 and GIT-11: create/list/revoke PATs; clone/fetch/push over HTTPS using a PAT.

**Requirements:** GIT-02, GIT-11

**Success criteria (from ROADMAP):**
1. User can create, list, and revoke personal access tokens for HTTPS git
2. User can clone, fetch, and push over HTTPS using a PAT; account password is rejected for git auth

**Out of scope (later phases / v2):**
- SSH keys and SSH clone/push (Phase 9)
- OAuth apps / third-party app integrations (PLAT-V2-02 remainder — PAT fine-grained **is** in Phase 8)
- PAT as Bearer for typed RPC (explicitly deferred — web/RPC stay on session cookies)
- Org-scoped tokens / collaborator ACL beyond owner (Phase 10)

**UI hint:** yes — `/settings/tokens`, Classic vs Fine-grained create flows, clone-box / empty-repo PAT how-to panel.

</domain>

<decisions>
## Implementation Decisions

### A — PAT capabilities
- **D-01:** PATs authenticate **git over HTTPS only** in Phase 8 — not RPC/API Bearer; web stays on opaque session cookies — **Reversibility:** costly — credential class split *(08-02 locked option `git_401_pat_https_only` with D-21)*
- **D-02:** Dual remotes like GitHub/other forges: **HTTPS + PAT now**; **SSH** remains Phase 9
- **D-03:** Support **both classic scopes and fine-grained scopes** in Phase 8 (pulls PAT portion of PLAT-V2-02 forward; OAuth apps stay later) — **Reversibility:** costly — token model + UI
- **D-04:** Fine-grained model targets **GitHub-/forge parity** (repository selection + account permissions as needed for real forge use); exact scope catalog is Claude discretion via research
- **D-05:** **Two separate create flows** — Classic PAT and Fine-grained PAT (not a single wizard)
- **D-06:** Fine-grained tokens support **selected repos** and **all current + future repositories** (GitHub-style)
- **D-07:** **Optional expiry** — user may set an expiration date or choose no expiration
- **D-08:** **Prefixed opaque** token strings — classic `oxidean_pat_`, fine-grained `oxidean_fg_`, then CSPRNG hex (32+ bytes); **no** `github` / `gh*` in the prefix — **Reversibility:** one-way — published token format / secret-scanning hooks *(08-02 locked option `oxidean_prefixes`; CONTEXT/RESEARCH had recommended shorter `ona_pat_` / `ona_fg_`)*
- **D-09:** Token list shows **last-used timestamp** and **last-used IP**

### B — HTTPS credential contract
- **D-10:** Credential form is **username + PAT as password** (forge-compatible); researcher locks exact username aliases (`git` / account username / etc.) — **Reversibility:** costly — client docs + auth parser
- **D-11:** **Hard reject** account passwords for git HTTPS, with a **hint** pointing users to create a PAT / docs (GIT-02)
- **D-12:** **PAT-only** for Smart HTTP — **session cookies never** authenticate git operations
- **D-13:** Clone box / empty-repo guidance includes a **full how-to panel** (username, password=PAT, create-token CTA, examples) — extends Phase 7 D-22

### C — Token management UX
- **D-14:** Manage tokens at **`/settings/tokens`** with settings/chrome nav — **Reversibility:** costly — settings IA
- **D-15:** After create: **one-time plaintext reveal** with copy; never shown again (must revoke + recreate)
- **D-16:** **Required note/name** on create (e.g. “laptop”, “CI”)
- **D-17:** **Confirm dialog** before revoke

### D — Smart HTTP surface
- **D-18:** Clone URL shape **`https://{host}/{owner}/{repo}.git`** on the same public origin — **Reversibility:** one-way — public git URL scheme *(08-02 locked option `owner_repo_git`)*
- **D-19:** Clone URL host comes from **`OXIDEAN_PUBLIC_ORIGIN`** (operator config), not the request Host header
- **D-20:** **Anonymous clone/fetch** of **public** repos; **push always requires a PAT**
- **D-21:** Unauthenticated access to **private** / no-access over git → **401 + WWW-Authenticate** (not the web UI’s 404 anti-enumeration) — **Reversibility:** costly — git vs web error contracts differ by design *(08-02 locked option `git_401_pat_https_only` with D-01)*
- **D-22:** Smart HTTP is served **only** on `/{owner}/{repo}.git`; bare `/{owner}/{repo}` remains the web UI *(08-02 confirmed with D-18)*
- **D-23:** Authenticated but **insufficient PAT scope** → **403** (not 401)

### E — Verify gate & abuse limits
- **D-24:** **Email verified required** to **create PATs** and to **HTTPS push** (aligns with Phase 5 `require_verified` / `repo.create`)
- **D-25:** Unverified create/push surfaces **`auth.email_unverified` / verify wall** + link to `/verify` (same as other privileged actions)
- **D-26:** **Rate-limit** failed Smart HTTP Basic/PAT auth **per IP and per user**; when limited return **429 + Retry-After**; exact thresholds left to research/planning

### Claude's Discretion
- Exact classic and fine-grained **scope catalogs** (must achieve GitHub-/forge-comparable git HTTPS capabilities)
- Exact username alias set for Basic auth (within D-10)
- Exact rate-limit **N / window** values (within D-26)
- Smart HTTP implementation approach (`git-http-backend` vs custom Axum handlers) — research/planning
- Traefik routing details to serve `/{owner}/{repo}.git` alongside web routes

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 8 goal, GIT-02 / GIT-11, success criteria
- `.planning/REQUIREMENTS.md` — GIT-02, GIT-11; note PLAT-V2-02 OAuth apps remain deferred (PAT fine-grained pulled into Phase 8 per D-03)
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — D-22 clone box, D-30 bare layout, D-32 CliGitBackend, D-23–D-25 visibility
- `.planning/phases/05-cloud-verify-reset/05-CONTEXT.md` — `require_verified` / privileged actions
- `.planning/phases/06-self-host-admin-bootstrap/06-CONTEXT.md` — session vs setup gates; single product path

### Product / architecture docs
- `docs/ARCHITECTURE.md` — Axum, sessions, RPC boundaries
- `docs/CONFIGURATION.md` — env/config patterns including public origin / repos dir
- `docs/API.md` — RPC surface (PATs must not confuse with session RPC auth)

### Existing implementation (extend)
- `apps/web/src/components/repo/clone-box.tsrx` — HTTPS clone UI (extend with how-to panel)
- `crates/oxidean-api/src/auth/gate.rs` — `require_verified`
- `crates/oxidean-api/src/auth/session.rs` — opaque cookies (must not authenticate Smart HTTP)
- `crates/oxidean-api/src/routes/repo_raw.rs` — existing non-Smart-HTTP git-ish HTTP
- `crates/oxidean-git/` — `CliGitBackend` / version gate
- `docker-compose.yml` / Traefik — routing for `.git` Smart HTTP

### UI authoring
- `.agents/skills/octane/SKILL.md` — `.tsrx` authoring for `/settings/tokens`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `CloneBox` — HTTPS URL + SSH placeholder + archives; needs PAT how-to panel (D-13)
- `require_verified` + verify wall / banner patterns from Phase 5–7
- Settings routes under `apps/web/src/routes/settings/` — add tokens sibling to profile
- Session cookie + Argon2 password hashing — do **not** reuse password check for git; reject with hint (D-11)
- `OXIDEAN_PUBLIC_ORIGIN` already used for SSR/public URLs in Compose

### Established Patterns
- Opaque session tokens hashed at rest — PAT plaintext shown once; store hash only after create
- Dialect SQL only in `oxidean-db`; new PAT tables follow that rule
- RPC via specta + `make rpc-gen` for token CRUD procedures
- Private web browse → 404; git Smart HTTP private → 401 (D-21) — intentional split

### Integration Points
- Traefik / Axum must route `/{owner}/{repo}.git/info/refs` and upload-pack/receive-pack without stealing web `/{owner}/{repo}`
- Bare repo path `{OXIDEAN_REPOS_DIR}/{owner}/{name}.git` (Phase 7 D-30)
- ACL: owner-only private until Phase 10; public anonymous read for git fetch

</code_context>

<specifics>
## Specific Ideas

- Match **GitHub and other forge alternatives** for: dual HTTPS/SSH remotes (SSH later), username+PAT-as-password, classic + fine-grained with separate create flows, all-repos FG option, optional expiry, last-used + IP
- Token prefixes must be **Oxidean-branded only** — never imply GitHub in the string

</specifics>

<deferred>
## Deferred Ideas

- SSH clone/push and SSH key management — Phase 9
- PAT / token use as RPC Bearer — out of Phase 8 by D-01
- OAuth apps / third-party integrations — remaining PLAT-V2-02
- Org-owned / fine-grained org tokens — Phase 10+
- Credential helper packaging beyond docs/how-to panel

</deferred>

---

*Phase: 08-Git HTTPS & PATs*
*Context gathered: 2026-09-13*
