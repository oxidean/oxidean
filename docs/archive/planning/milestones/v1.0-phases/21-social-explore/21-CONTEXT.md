# Phase 21: Social & Explore - Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss mode:** Auto-decided to **GitHub parity** per orchestrator (isolated worktree; depends on Phases 10 + 12 fork-head model).

<domain>
## Phase Boundary

Users discover public work via explore, profiles, stars, and forks. Delivers SOC-01 … SOC-04.

**Requirements:** SOC-01, SOC-02, SOC-03, SOC-04  
**Depends on:** Phase 10 (orgs/ACL/visibility), Phase 12 CONTEXT (D-PR-01…03 fork-head PR model — fork graph must be queryable before PR plans execute)

**Success criteria (from ROADMAP):**
1. User can star and unstar repositories
2. User can view another user’s public profile and public repositories
3. Anonymous or signed-in user can browse an explore/discover page of public repositories
4. User can fork a public repository they can read

**Out of scope:**
- Watching / notifications of repo activity (Phase 17)
- Sponsors, social follows/followers graph, activity feed / contribution heatmap
- Topics/tags taxonomy beyond simple explore sort/filter
- Fork sync / “Fetch upstream” UI automation (git remotes are enough; optional mention in docs)
- Private-repo forks (SOC-04 is public-only)
- Cross-instance federation

**UI hint:** yes — `/explore` (slug already reserved), public `/{username}` profile vs org overview, Star + Fork on `RepoChrome`, fork create flow.

</domain>

<decisions>
## Implementation Decisions

### A — Stars (SOC-01)
- **D-SOC-01:** Stars are a **per-user ↔ repository** membership (unique pair). Signed-in users may star/unstar any repository they can **Read** (public or private via ACL). Anonymous cannot star — **Reversibility:** reversible
- **D-SOC-02:** Persist `star_count` denormalized on `repositories` (or equivalent maintained counter) for explore/sort; keep authoritative rows in `repository_stars` — **Reversibility:** costly — counter maintenance
- **D-SOC-03:** Expose star state on `RepoPublic` (`star_count`, `viewer_has_starred`) and RPCs `repo.star` / `repo.unstar` (idempotent). List starred repos via `user.listStarred` (offset pagination) — **Reversibility:** reversible
- **D-SOC-04:** UI: **Star** control on layout-owned `RepoChrome` title row (count + toggle); do not remount chrome in leaves (11.1 D-QH-01). Profile can show a **Stars** tab/list of starred public (and readable) repos — **Reversibility:** reversible

### B — Public profiles (SOC-02)
- **D-SOC-05:** Public user profile at **`/{username}`** when the slug is a **user** (not an org). Org overview at the same path when the slug is an org (existing Phase 10). Shared resolver: org → org overview; else user → profile; else 404 — **Reversibility:** costly — route ownership of `$owner.index`
- **D-SOC-06:** Public profile fields: **username, display_name, bio, avatar_url**; **never** email. Reuse AUTH-08 profile fields; `/settings/profile` remains the edit surface — **Reversibility:** reversible
- **D-SOC-07:** Profile repository list: repos the **viewer** may Read under that owner (anonymous → public only; signed-in → public + private grants). Sort recently updated; offset pagination — **Reversibility:** reversible
- **D-SOC-08:** RPC: `user.getPublicProfile` (by username) + reuse/extend `repo.listByOwner` ACL filtering. Anti-enumeration: unknown username → same not-found as org miss — **Reversibility:** reversible

### C — Explore (SOC-03)
- **D-SOC-09:** Explore at **`/explore`** (reserved username already). Available to **anonymous and signed-in** — **Reversibility:** reversible
- **D-SOC-10:** Lists **public** repositories only. Default sort: **`star_count` desc, then `updated_at` desc**. Offset pagination. Optional query filter by name/description substring (case-insensitive) — **Reversibility:** reversible
- **D-SOC-11:** RPC `repo.explore` (or `explore.listRepos`) with anonymous session OK. Site chrome link to Explore (header/mobile nav) — **Reversibility:** reversible

### D — Forks (SOC-04) — align with Phase 12 D-PR-01…03
- **D-SOC-12:** User with **Read** on a **public** source repo may fork it. Private sources are **out of scope** for this phase (SOC-04 wording) — **Reversibility:** reversible (can widen later)
- **D-SOC-13:** Fork creates a **new repository** under a chosen owner the actor may create under (**self** or **org** with create permission — same as `repo.create` owner picker). Default name = source name; name conflict → `repo.name_taken` — **Reversibility:** costly — create path
- **D-SOC-14:** Schema: `forked_from_id` (immediate parent repo id, nullable) + `fork_network_id` (root/upstream repo id; for roots equals own `id`). One **active fork per (owner, fork_network_id)** — **Reversibility:** one-way — fork graph for PR heads
- **D-SOC-15:** Git storage: **bare copy** of all refs from source (`git clone --bare` / backend method) into the new owner path; description defaults to source description; visibility defaults **public**; default_branch copied — **Reversibility:** costly — GitBackend surface
- **D-SOC-16:** RPC `repo.fork` returns `RepoPublic` including fork metadata (`forked_from`, `is_fork`, `parent` summary). UI: **Fork** button on `RepoChrome` → confirm owner/name → redirect to new repo — **Reversibility:** reversible
- **D-SOC-17:** Export ACL/query helpers Phase 12 will use: **head is valid for base** iff same repo **or** `head.fork_network_id == base.id` (D-PR-01, D-PR-03). Do not implement PR open UI here — **Reversibility:** reversible
- **D-SOC-18:** Cannot fork into an owner that already has a fork in the same network; cannot treat soft-deleted source as forkable; deleted forks free the (owner, network) slot — **Reversibility:** reversible

### E — ACL & visibility (carry forward)
- **D-SOC-19:** Reuse Phase 10 `Capability::{Read,Write,Admin}` and `repo.not_found` anti-enumeration for private sources. Explore never lists private. Stars/forks require session + Read on the target — **Reversibility:** reversible

### Claude's Discretion
- Exact RPC camelCase vs dotted names (`repo.star` vs `repo.toggleStar`)
- Whether star_count is a column vs `COUNT(*)` with indexed table only (prefer column if explore sort needs it)
- Fork confirm page vs modal
- Whether profile Stars tab is a query param (`?tab=stars`) or path (`/{user}/stars`) — prefer GitHub-like `?tab=` or subpath without colliding with repo names (reserved: use `/users/{user}/stars` only if flat `/{user}` conflict; **prefer** tabs on `/{user}` index to avoid new reserved segments)
- Exact explore page empty/loading copy within brand tokens

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 21 goal, SOC-01…04, depends on 10 + 12
- `.planning/REQUIREMENTS.md` — SOC-01…04 wording
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Capability ACL, shared slug namespace, `$owner` routes
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — **D-PR-01…03** (fork heads), D-PR-28 compare from forks
- `.planning/phases/11.1-quality-hardening/11.1-CONTEXT.md` — RepoChrome layout ownership (D-QH-01)
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — repo create, bare paths, visibility
- AUTH-08 (profile edit) already shipped — extend read-side only for public profiles

### Code mirrors
- `crates/oxidean-api/src/repo/acl.rs` — Read/Write/Admin
- `crates/oxidean-api/src/repo/mod.rs` — `repo.create` / `to_public` / listByOwner
- `crates/oxidean-db/migrations/*/0015_packages.sql` — next migration after packages
- `crates/oxidean-core/src/repo_types.rs` — `RepoPublic`
- `crates/oxidean-core/src/auth_types.rs` — `explore` reserved; `UserPublic` profile fields
- `apps/web/src/routes/$owner.index.tsrx` — org-only today; must branch for users
- `apps/web/src/components/repo/repo-chrome.tsrx` — Star/Fork affordances
- `apps/web/src/components/chrome.tsrx` — SiteHeader Explore link
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` mandatory for UI

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Polymorphic owners (`owner_type` + `owner_id`), ACL, public/private visibility
- `repo.create` + bare `init_bare`; owner picker on `/new`
- Org overview SSR (`ssr-org.ts`) pattern for owner pages
- AUTH-08 `user.get_profile` / `user.update_profile` + avatar
- Reserved username `explore` already blocks slug collisions

### Gaps
- No `repository_stars` / fork columns / explore RPC
- `$owner.index` assumes organization only (users 404)
- `GitBackend` has `init_bare` but no bare-copy/clone helper for forks
- `RepoPublic` lacks star/fork metadata
- No Explore nav entry

</code_context>

<deferred>
## Deferred Ideas

- Watch / subscribe notifications (Phase 17)
- Followers / following social graph
- Contribution heatmap / activity feed
- Private repository forks
- Automated upstream sync UI
- Topics / curated explore collections
</deferred>
