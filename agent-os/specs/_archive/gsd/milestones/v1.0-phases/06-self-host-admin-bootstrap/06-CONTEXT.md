# Phase 6: Self-Host Admin Bootstrap - Context

**Gathered:** 2026-09-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Empty installs get a first `sys-admin` via `OXIDEAN_ADMIN_EMAIL` + `OXIDEAN_ADMIN_PASSWORD` **or** a one-time `/setup` wizard. After bootstrap, **`allow_signup`** governs whether local signup is open (default closed). Cloud and self-host are the **same product path** — no deployment-mode forks. Also in scope (folded): eliminate signed-in `/` flicker via **SSR session gating**.

**Requirements:** AUTH-06, AUTH-07 (reframe wording as empty-instance, not “self-host only”); AUTH-05 behavior updated by `allow_signup` for this milestone.

**Success criteria (from ROADMAP, clarified in discussion):**
1. When both `OXIDEAN_ADMIN_*` are set, first boot seeds that admin account (then forced credential change on first login)
2. When either/both are unset, empty instance shows one-time wizard to create the admin; then `allow_signup` rules apply
3. Signed-in `/` first HTML matches final UI (no marketing→home flicker); `/dashboard` is not a public page

**Out of scope (later phases):**
- Invite codes / invite issuance when signup is closed
- Git repos & browse (Phase 7)
- Full IdP-only auth without local accounts (already Phase 4 provider modes)

**UI hint:** yes — `/setup`, forced credential-change after ENV seed, SSR `/` home, `allow_signup` control on wizard/admin.

</domain>

<decisions>
## Implementation Decisions

### A — Deployment scope (one product)
- **D-01:** Empty-instance bootstrap applies to **any empty DB** — same path for cloud and self-host (no mode gate)
- **D-02:** Reframe AUTH-06/07 (and related docs) as **empty-instance**, not “On self-host” — update REQUIREMENTS when touched
- **D-03:** **No deployment modes** — do not add `OXIDEAN_DEPLOYMENT_MODE` or equivalent; cloud ≡ self-host
- **D-04:** **Hard rule:** no cloud/self-host conditionals in bootstrap (or related Phase 6 paths); tests assert one path — **Reversibility:** costly — product identity depends on single release train

### B — Post-bootstrap signup (`allow_signup`)
- **D-05:** After bootstrap, **`allow_signup`** governs local signup (revises Phase 5 D-08 / AUTH-05 “always open” for this product rule)
- **D-06:** When `allow_signup` is off: **`/signup` returns 404**; logged-out UI **omits all Sign-up references** (no AuthShell soft page; no invite codes) — **Reversibility:** costly — routing + chrome contract
- **D-07:** Default when ENV unset: **`allow_signup = false`** (`OXIDEAN_ALLOW_SIGNUP`)
- **D-08:** **ENV seed** applies `OXIDEAN_ALLOW_SIGNUP` at seed time; **wizard** exposes the same control (**Switch**) — **Reversibility:** costly — persisted instance setting + ENV contract

### C — Empty-instance lock
- **D-09:** **Hard SSR/server gate** to `/setup` before paint while `needs_setup`; signup/SSO blocked until bootstrap completes
- **D-10:** Carve-outs: **`/status` and readiness** stay public; all other app UI → `/setup`
- **D-11:** **Strict RPC** while `needs_setup`: only `auth.bootstrap_status` / `auth.bootstrap_setup` (+ health) succeed
- **D-12:** After successful wizard setup (session issued): land on **signed-in `/`**

### D — ENV seed edge cases
- **D-13:** If **either or both** `OXIDEAN_ADMIN_*` unset → treat as unset; wizard creates sys-admin
- **D-14:** If both set but **seed fails** → **fail boot** (surface error; do not serve the app) — **Reversibility:** one-way — operators rely on fail-closed boot
- **D-15:** ENV-seeded username: **`system-administrator`** (not `admin`/`admin1`)
- **D-16:** ENV seed **creates** the account; first visit gates **`/setup/credentials`** — must change **default** values (username `system-administrator`); **ENV email/password may be kept** (Keep current password Switch)
- **D-17:** Forced credential change applies to **ENV-seeded admins only** — wizard-created credentials are already chosen

### E — Signed-in home SSR (folded flicker fix)
- **D-18:** **SSR session** chooses marketing vs `SignedInHome` at `/`; first HTML matches final UI
- **D-19:** Direct **`/dashboard` → 404** (not a public route; remove soft-redirect pattern)
- **D-20:** `/` SSR priority: **`needs_setup` first** → `/setup`; else session → SignedInHome vs marketing
- **D-21:** Keep `oxidean_signed_in` **presence hint** as progressive enhancement; **drop and rely on SSR** if it causes trouble — agent discretion
- **D-22:** After login and after ENV forced credential change: honor safe **`returnTo`**; else `/`

### the agent's Discretion
- Presence hint retention vs removal if flicker/race issues persist (D-21)
- Exact wizard copy/layout for `allow_signup` control and forced-change screen (match AuthShell patterns)
- How `allow_signup` is persisted (settings table vs dedicated column) — researcher/planner choose existing admin.auth settings patterns

### Folded Todos
- **Fix signed-in home flicker on load** (`.planning/todos/pending/2026-09-11-fix-signed-in-home-flicker-on-load.md`) — SSR session gate for `/`; `/dashboard` 404; aligns with D-18–D-22

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 6 goal, AUTH-06/07 success criteria
- `.planning/REQUIREMENTS.md` — AUTH-06, AUTH-07 (reframe empty-instance); AUTH-05 interaction with `allow_signup`
- `.planning/PROJECT.md` — dual-mode one product; self-host bootstrap constraint (update mental model: empty-instance, not mode fork)
- `.planning/phases/05-cloud-verify-reset/05-CONTEXT.md` — D-04 auto-verify seeded/wizard admin; D-08 open signup (superseded by Phase 6 `allow_signup`)
- `.planning/phases/04-auth-sessions-email/04-CONTEXT.md` — Rust-native auth, username rules, admin auth settings, provider modes
- `.planning/todos/pending/2026-09-11-fix-signed-in-home-flicker-on-load.md` — folded SSR home flicker fix (preferred direction)

### Operator / architecture docs
- `docs/CONFIGURATION.md` — `OXIDEAN_ADMIN_*` (extend with `OXIDEAN_ALLOW_SIGNUP`)
- `docs/ARCHITECTURE.md` — bootstrap / empty users seed + wizard notes
- `.env.example` — admin seed comments

### Existing implementation (extend, don’t rewrite blindly)
- `crates/oxidean-api/src/auth/bootstrap.rs` — `needs_setup`, `bootstrap_status`, `bootstrap_setup`
- `crates/oxidean-api/src/auth/seed.rs` — ENV `maybe_seed_admin`
- `crates/oxidean-api/tests/auth_bootstrap.rs` — AUTH-07 coverage
- `apps/web/src/routes/setup.tsrx` — wizard UI
- `apps/web/src/lib/bootstrap.ts` — client `redirectIfNeedsSetup` (to be replaced/superseded by SSR gate)
- `apps/web/src/lib/session-hint.ts` — presence hint
- `apps/web/src/routes/index.tsrx` — home / marketing vs signed-in
- `apps/web/src/components/signed-in-home.tsrx` — signed-in home tree
- `apps/web/src/routes/dashboard.tsrx` — remove as public route (404)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `auth.bootstrap_status` / `auth.bootstrap_setup` + `/setup` AuthShell form — extend with `allow_signup` control and stricter gates
- `maybe_seed_admin` in `seed.rs` — change username to `system-administrator`; wire `OXIDEAN_ALLOW_SIGNUP`; fail-boot on error
- `AuthShell`, signup/login forms, `returnTo` helpers — forced credential-change page and post-login redirects
- `session-hint` + `SignedInHome` / page skeletons — SSR home rewrite target

### Established Patterns
- Auto-verified sys-admin on seed/wizard (Phase 5 D-04) — keep
- Reserved username bypass for bootstrap (`admin` today) — extend/replace for `system-administrator`
- Client `redirectIfNeedsSetup` on `/`, login, signup — elevate to SSR/server gate
- Dotted RPC `auth.*` — add settings/`allow_signup` procedures alongside existing admin.auth

### Integration Points
- App boot: seed then serve — fail closed on seed error when both ENV set
- SPA router + TanStack Start SSR: `/` session + `needs_setup` priority; `/dashboard` 404
- Signup RPC + `/signup` route: enforce `allow_signup` after bootstrap
- Admin auth settings UI: expose `allow_signup` post-bootstrap (wizard also sets it)

</code_context>

<specifics>
## Specific Ideas

- Username **`system-administrator`** for ENV-seeded account
- Forced **email and password change** on first login for ENV-seeded admins only
- Wizard remains the path when ENV is incomplete; ENV path is seed + forced change, not silent forever-credentials
- User OK dropping presence hint if it fights SSR

</specifics>

<deferred>
## Deferred Ideas

- **Invite codes / invite issuance** when `allow_signup` is off — later phase; Phase 6 is hard-block only

</deferred>

---

*Phase: 6-Self-Host Admin Bootstrap*
*Context gathered: 2026-09-11*
