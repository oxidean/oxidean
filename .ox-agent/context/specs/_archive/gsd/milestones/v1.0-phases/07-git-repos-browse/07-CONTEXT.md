# Phase 7: Git Repos & Browse - Context

**Gathered:** 2026-09-12
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can create filesystem-backed git repositories and browse history in the UI: create public/private repos, browse files/commits/branches/tags (GitHub-like), branch CRUD where permitted, download source archives, and store objects on a volume-backed path. Backend git operations use the **system `git` CLI (2.5+)** behind an abstraction, with a **documented future path to gitoxide** when it covers required operations.

**Requirements:** GIT-01, GIT-05, GIT-06, GIT-07, GIT-08, GIT-09 (amended — see D-32), GIT-10

**Success criteria (from ROADMAP, clarified in discussion):**
1. Authenticated (and verified) user can create a public or private repository
2. User can browse files, commits, branches, and tags in the web UI and download a source archive for a ref
3. User can create, rename, and delete branches from the web UI where permitted (owner-only in Phase 7)
4. Repository objects live on the local filesystem (volume-backed); git ops via `git` CLI with architecture allowing a future gitoxide backend

**Out of scope (later phases):**
- HTTPS clone/push with PATs (Phase 8) — UI may show HTTPS URL now
- SSH keys and SSH clone/push (Phase 9) — UI placeholder only
- Org/collaborator permissions (Phase 10) — private = owner-only until then
- Full home activity feed (needs social + richer git events)
- Public marketplace UI for stack presets (in-repo packs + guide only)
- Full branch-protection rules product (soft-protect default branch only)

**UI hint:** yes — `/new`, dashboard home, `/{owner}/{repo}` browse surfaces, repo settings visibility, admin reset modal updates.

</domain>

<decisions>
## Implementation Decisions

### A — Create flow & dashboard home
- **D-01:** Dedicated **`/new`** page; signed-in home “New repository” CTA navigates there
- **D-02:** Create uses **templates**: stack preset + optional License + `.gitignore` as **grouped independent pickers**; include common project files for stacks
- **D-03:** **Wide day-one** stack catalog; **community presets** via **in-repo packs + docs guide** (PRs); public marketplace UI later (designed so packs can feed it)
- **D-04:** **License picker: full SPDX list** (+ none); **`.gitignore`: broad gitignore.io-style catalog**
- **D-05:** **Owner fixed to current user** on `/new` (orgs Phase 10)
- **D-06:** **GitHub-ish** name/slug rules (letters, digits, hyphen, underscore, period; reject reserved names) — **Reversibility:** costly — URL and uniqueness contracts
- **D-07:** Optional **description** on create; shown on repo home
- **D-08:** Default visibility **sys-admin configurable**; if unset → **public**
- **D-09:** Default branch **`main`**; overridable in **account settings** (org settings → Phase 10)
- **D-10:** After create: **empty-state first-push guide** when no commit (forge-familiar); else Code view
- **D-11:** **New repository CTA disabled** until email verified; **`/new` blocked** until verified (redirect/wall) — aligns Phase 5 D-12 + `require_verified` on `repo.create`
- **D-12:** Duplicate name → **inline field error**
- **D-13:** Signed-in **`/` is a dashboard** (Gitea/GitHub feel): **repo list** (recently updated first, visibility badge/lock), **empty hero** when zero repos, **activity feed placeholder** only — **Reversibility:** costly — home IA contract

### B — Browse IA & URLs
- **D-14:** Repo URLs **`/{owner}/{repo}`** with **reserved-name denylist** (`login`, `setup`, `new`, `status`, etc.) — **Reversibility:** one-way — public URL scheme
- **D-15:** Default Code view: **file tree + rendered README below** when present; empty repo → first-push guide
- **D-16:** **GitHub-like IA:** Code primary; `/commits/{ref}`; `/branches`; `/tags`; `/commit/{sha}` with diffs; **`/compare/{base}...{head}`**; **`/blame/{ref}/path`**; per-file history from blob
- **D-17:** Paths: `/tree/{ref}/…`, `/blob/{ref}/…`, `/raw/{ref}/…`; refs use **branch/tag names** when possible
- **D-18:** **Safe GFM-like Markdown** rendering (sanitized) for GitHub parity
- **D-19:** **Syntax highlighting** matches **GitHub’s language/file-type coverage**, plus **`.tsrx`** and **`.ripple`**
- **D-20:** Line permalinks `#L10` / `#L10-L20`; large files **GitHub-like soft limits**; binaries: **images inline**, else download / cannot preview
- **D-21:** **Submodules** as entries with **recursive browsing**
- **D-22:** Code tab **clone/download box**: HTTPS URL now (auth Phase 8); SSH placeholder until Phase 9; archives via GIT-07

### C — Visibility (pre–Phase 10)
- **D-23:** **Private = owner-only** until orgs/collaborators — **Reversibility:** costly — ACL model stub
- **D-24:** **Anonymous read** of public repos
- **D-25:** Private / no-access → **404** (anti-enumeration)
- **D-26:** Owner can **toggle public/private** in repo settings in Phase 7

### D — Branches & archives
- **D-27:** Branch create/rename/delete: **owner only**
- **D-28:** Soft-protect default branch: **block delete/rename** in UI (full branch protection later)
- **D-29:** Archives: **zip and tar.gz** from Code clone/download menu for current ref (also tags/commits where natural)

### E — On-disk storage & git backend
- **D-30:** **Bare repos** at `{OXIDEAN_REPOS_DIR}/{owner}/{name}.git` (default `var/repos`) + **Compose volume** — **Reversibility:** costly — storage layout
- **D-31:** **`OXIDEAN_REPOS_DIR`** configurable (mirror uploads pattern)
- **D-32:** **REQUIREMENT AMENDMENT (GIT-09):** Phase 7 implements git ops via **system `git` CLI**, not gitoxide-first. Keep a **`GitBackend` abstraction**; **document future gitoxide** path when it covers needed operations — **Reversibility:** costly — roadmap/requirements wording + crate design
- **D-33:** **Fail boot** if `git` missing or version **&lt; 2.5** — **Reversibility:** one-way — operator contract
- **D-34:** Factory reset: **modal with radio buttons** for reset scope (e.g. DB-only vs DB+repos) — extends Phase 6 danger zone
- **D-35:** Repo delete: **soft-delete in DB**; **async/later disk purge**
- **D-36:** **Periodic orphan reconcile**; sys-admin configures **cleanup frequency**
- **D-37:** **Scheduled `git gc`** + sys-admin **manual GC** trigger
- **D-38:** Repos owned by **API process user**; document Compose UID/GID

### Claude's Discretion
- Exact SPDX packaging / license text sourcing and gitignore.io catalog packaging
- Exact reserved-name denylist contents (must cover all flat app routes)
- Exact factory-reset radio options copy and defaults
- Soft-delete retention window / purge job timing details
- Highlighting engine choice (must meet D-19 coverage including `.tsrx` / `.ripple`)
- Compare/blame/diff UX details within GitHub parity
- `GitBackend` trait shape and how CLI is invoked (researcher/planner)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 7 goal and success criteria
- `.planning/REQUIREMENTS.md` — GIT-01, GIT-05–GIT-10 (amend GIT-09 per D-32)
- `.planning/PROJECT.md` — forge product; git object layer note (update mental model: CLI now, gitoxide later)
- `.planning/STATE.md` — current focus; post-06 Query/Octane assumptions
- `.planning/phases/06-self-host-admin-bootstrap/deferred-items.md` — Query session cache + Octane `.tsrx` truths
- `.planning/phases/05-cloud-verify-reset/05-CONTEXT.md` — `require_verified`, disabled CTAs, `repo.create` as first privileged consumer
- `.planning/phases/06-self-host-admin-bootstrap/06-CONTEXT.md` — SSR `/`, `/dashboard` 404, factory reset existence

### Product docs
- `docs/ARCHITECTURE.md` — RPC, `var/` layout, Traefik
- `docs/CODE_PRACTICES.md` — monorepo boundaries, Octane, RPC codegen
- `docs/CONFIGURATION.md` — extend with `OXIDEAN_REPOS_DIR`, git version, visibility defaults
- `docs/database.md` — migration parity pattern
- `AGENTS.md` — Octane ≠ React; Query session helpers
- `.agents/skills/octane/SKILL.md` — `.tsrx` authoring

### Existing implementation (extend)
- `apps/web/src/components/signed-in-home.tsrx` — New repository CTA stub
- `crates/oxidean-api/src/auth/gate.rs` — `require_verified`
- `crates/oxidean-api/src/rpc.rs` — procedure dispatch (add `repo.*`)
- `crates/oxidean-api/src/routes/avatar.rs` — filesystem + volume precedent
- `docker-compose.yml` — volume bind pattern for `var/uploads`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `require_verified` + `auth.email_unverified` + disabled home CTA pattern
- `SignedInHome` / chrome / session Query helpers (`session-queries.ts`)
- Avatar `uploads_dir` + Compose bind — template for `OXIDEAN_REPOS_DIR`
- Factory reset RPC/UI — extend with reset-scope modal (D-34)
- Tri-dialect migrations under `crates/oxidean-db/migrations/`

### Established Patterns
- RPC + `make rpc-gen` for typed client
- Octane `.tsrx` Rivet templates; no React/`@{` mixing
- TanStack Query for server domain data; local state for forms
- Stable error codes for UI

### Integration Points
- Enable home CTA → `/new` → `repo.create`
- Traefik: archive/raw download routes may need API path rules
- Bootstrap `needs_setup` gates still apply before any repo UI
- Reserved routes must not collide with `/{owner}/{repo}`

</code_context>

<specifics>
## Specific Ideas

- Match **GitHub / Gitea** UX deliberately (home dashboard, Code IA, clone menu, 404 for private)
- Activity feed on home is **placeholder** until social/repo/git events exist
- Highlighting must include **`.tsrx`** and **`.ripple`** beyond GitHub’s set
- Community stack presets: **in-repo + guide now**, marketplace UI later
- Git backend: user explicitly chose **CLI now** because gitoxide is not full 1:1 yet

</specifics>

<deferred>
## Deferred Ideas

- Full home **activity feed** (social + repo + git) — further planning required
- **Org-level** default branch / visibility settings — Phase 10
- **Public marketplace UI** for stack presets — later phase
- HTTPS PAT auth (Phase 8) / SSH (Phase 9) — clone URL may show early
- Full **branch protection** product rules — soft default-branch protect only in Phase 7
- Update `.planning/REQUIREMENTS.md` GIT-09 wording to CLI-primary + gitoxide-later (planner/docs task)

</deferred>

---

*Phase: 7-Git Repos & Browse*
*Context gathered: 2026-09-12*
