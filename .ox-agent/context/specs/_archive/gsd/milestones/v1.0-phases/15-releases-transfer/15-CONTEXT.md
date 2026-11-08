# Phase 15: Releases & Transfer - Context

**Gathered:** 2026-09-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Users ship tag-based releases with notes and downloadable assets, and admins can rename or transfer repositories. Delivers GIT-14, GIT-15, GIT-16, GIT-17.

**Requirements:** GIT-14, GIT-15, GIT-16, GIT-17

**Success criteria (from ROADMAP):**
1. User can create a release for a tag with notes and downloadable assets, and download those assets from the web UI
2. User with permission can rename a repository
3. User with permission can transfer a repository to another user or organization

**Out of scope:**
- Release reactions / discussions (beyond notes)
- Automated changelog generation from PRs/commits
- Cross-instance transfer / migration tooling
- Package registry publishing from releases (Phase 20)

**UI hint:** yes — Releases tab; release list/detail; rename + transfer in repo settings.

</domain>

<decisions>
## Implementation Decisions

### A — Release model
- **D-REL-01:** **Tag must already exist**; release binds to that tag (no create-tag-on-publish in Phase 15) — **Reversibility:** reversible
- **D-REL-02:** Support **draft** and **prerelease** flags (GitHub-like) — **Reversibility:** reversible
- **D-REL-03:** **Author or Write+** can edit release notes/assets after publish; **Admin** can **delete** a release — **Reversibility:** reversible

### B — Release assets
- **D-REL-04:** Store assets on a **separate release-assets volume** (not the LFS OID store) — **Reversibility:** costly — storage split from LFS
- **D-REL-05:** **Configurable max asset size**; **replacing** an asset on edit is allowed — **Reversibility:** reversible
- **D-REL-06:** Authenticated download from web UI with **Read** (public repos: anonymous read of published release assets) — **Reversibility:** reversible

### C — Rename
- **D-REL-07:** **Admin only** may rename a repository (user-owned or org-owned) — **Reversibility:** reversible
- **D-REL-08:** Keep **HTTP redirects** from the old `/{owner}/{repo}` path for a **configurable retention window** — **Reversibility:** costly — redirect table + purge job

### D — Transfer
- **D-REL-09:** **Admin only** may transfer; destination is **user or org** — **Reversibility:** reversible
- **D-REL-10:** Transfer moves **git data + issues + LFS object associations**; confirm with **type-the-repo-name** — **Reversibility:** costly — ownership rewrite across tables
- **D-REL-11:** Webhooks / packages (future) follow ownership when those phases exist; Phase 15 does not invent stub webhooks — **Reversibility:** reversible

### E — Permissions & UI
- **D-REL-12:** **Write+** can create/publish releases; **Admin** for rename/transfer/delete release — **Reversibility:** reversible
- **D-REL-13:** Add **Releases** tab to repo chrome; list + detail + asset upload UI — **Reversibility:** reversible

### Claude's Discretion
- Exact default redirect retention duration
- Exact default max release asset size
- Whether drafts are visible only to Write+ (assume yes — GitHub-like)
- Exact settings IA for rename vs transfer (same danger zone vs separate)
- Whether release asset download URLs are stable after rename/transfer (prefer yes via release id)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 15 goal, GIT-14…17
- `.planning/REQUIREMENTS.md` — GIT-14…17 wording
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Admin/Write ACL, org ownership
- `.planning/phases/11-issues/11-CONTEXT.md` — issues must move on transfer
- `.planning/phases/14-git-lfs/14-CONTEXT.md` — LFS refs move; assets volume is separate from LFS

### Code mirrors
- `crates/oxidean-api/src/repo/acl.rs` — Write/Admin gates
- `crates/oxidean-api/src/repo/mod.rs` — repo mutate patterns
- `apps/web/src/components/repo/repo-chrome.tsrx` — add Releases tab
- `apps/web/src/routes/$owner.$repo.settings*.tsrx` — rename/transfer danger zone
- `docs/CONFIGURATION.md` — new volume + size limit env knobs

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Tag listing already in browse (Phase 7) — release picker sources tags
- ACL Write/Admin from Phase 10
- Upload volume pattern from avatars / LFS compose binds

### Integration Points
- Releases tab + `/{owner}/{repo}/releases`
- Settings: rename + transfer with confirm
- Factory reset / transfer must include release asset files for that repo
- Rename redirect middleware or route fallback

</code_context>

<specifics>
## Specific Ideas

- User accepted the full recommended package in one shot (GitHub/Gitea-leaning)
- Explicit: assets volume ≠ LFS store; transfer includes issues + LFS associations

</specifics>

<deferred>
## Deferred Ideas

- Auto-generated release notes from merged PRs
- Soft-delete releases / recycle bin
- Transfer across instances
- Attaching container packages to a release (Phase 20)

</deferred>

---

*Phase: 15-releases-transfer*
*Context gathered: 2026-09-14*
