# Phase 11: Issues - Context

**Gathered:** 2026-09-14
**Status:** Ready for planning

<domain>
## Phase Boundary

Users track work with repo-scoped issues: create/edit/close/reopen, comments, labels, assignees, and links toward PRs. Delivers ISS-01, ISS-02, ISS-03, ISS-04.

**Requirements:** ISS-01, ISS-02, ISS-03, ISS-04

**Success criteria (from ROADMAP):**
1. User can create, edit, close, and reopen issues
2. User can comment on issues and assign labels and assignees
3. User can link issues and PRs by reference

**Out of scope (later phases / v2):**
- Full pull request objects, review, and merge (Phase 12)
- Closing keywords enforced on PR merge (`fixes #N` / `closes #N`) — Phase 12
- Branch protection (Phase 13)
- Notifications for issue activity (Phase 17)
- Cross-instance / external tracker links
- Projects / milestones / boards (not in v1 roadmap)

**UI hint:** yes — Issues tab in repo chrome; `/{owner}/{repo}/issues`, detail, new, labels settings as needed.

</domain>

<decisions>
## Implementation Decisions

### A — Issue model & numbering
- **D-ISS-01:** **Per-repo sequential `#N`** issue numbers (GitHub-style URLs `/{owner}/{repo}/issues/{n}`) — **Reversibility:** one-way — public URLs + DB sequence per repo
- **D-ISS-02:** Lifecycle is **open ↔ closed** (reopen allowed). **Repo Admin** may **hard-delete** an issue (GitHub/Gitea parity; confirm in UI) — **Reversibility:** costly — delete cascades comments/reactions/links
- **D-ISS-03:** **Author + Write+** may edit title/body anytime after create — **Reversibility:** reversible
- **D-ISS-04:** **Full edit history** for issue title/body (diff trail, not timestamp-only) — **Reversibility:** costly — history tables + UI

### B — Labels & assignees
- **D-ISS-05:** Labels are **org defaults + per-repo overrides** (org-level catalog with repo-local customize) — **Reversibility:** costly — dual-scope schema + settings UI
- **D-ISS-06:** **Multiple assignees** per issue — **Reversibility:** reversible
- **D-ISS-07:** **Write+** can assign labels/assignees on an issue; only **Admin** can create/edit/delete **label definitions** — **Reversibility:** reversible
- **D-ISS-08:** Assignee eligibility = anyone with **Read+** on the repo (collaborators + org members with access) — **Reversibility:** reversible

### C — Comments & markdown
- **D-ISS-09:** Author can edit/delete own comments; **Write+** can delete others’ comments (moderation) — **Reversibility:** reversible
- **D-ISS-10:** Markdown authoring uses **Write | Preview** tabs; reuse existing `renderGfm` + sanitize pipeline — **Reversibility:** reversible
- **D-ISS-11:** **GitHub-style emoji reactions** on issues and comments in Phase 11 — **Reversibility:** costly — reaction tables + RPC
- **D-ISS-12:** **Full edit history** on comments (match issues) — **Reversibility:** costly — history tables + UI

### D — Issue↔PR linking
- **D-ISS-13:** Phase 11 ships **`#N` and `owner/repo#N` autolink** in markdown plus a **Linked PRs** sidebar/panel that may hold **stubs** until Phase 12 PR objects exist — **Reversibility:** costly — link table + stub UX
- **D-ISS-14:** Also ship a **manual “Link issue/PR”** control that writes stub (or real) links — **Reversibility:** reversible
- **D-ISS-15:** **Closing keywords** (`fixes` / `closes` `#N`) **wait for Phase 12** merge — do not enforce auto-close from commits in Phase 11 — **Reversibility:** reversible (deferral)

### E — List / filter IA
- **D-ISS-16:** Issues list defaults to **Open**; **Closed** and **All** available — **Reversibility:** reversible
- **D-ISS-17:** Phase 11 filters: **author, label, assignee, + text search** (not full GitHub search grammar) — **Reversibility:** reversible
- **D-ISS-18:** Sort **newest-updated first**; **offset pagination** (not infinite scroll) — **Reversibility:** reversible
- **D-ISS-19:** Add **Issues** tab to repo chrome; **New issue** from list and empty state — **Reversibility:** reversible

### ACL (carry forward — not re-litigated)
- **D-ISS-20:** Reuse Phase 10 `Capability::{Read,Write,Admin}`: **Read** → list/view; **Read + verified sign-in** → create issues, comment, react (git-forge parity — public participation does not require collaborator status); **author or Write+** → edit title/body, close/reopen, delete own comment; **Write+** → assign, label-on-issue, links, moderate-delete others' comments; **Admin** → label definition CRUD + hard-delete issues. Keep web **`repo.not_found`** anti-enumeration for private repos.

### Claude's Discretion
- Exact reaction emoji set (match a small GitHub subset vs full)
- Exact org-label vs repo-label override merge rules (inherit + hide + local-only)
- Exact stub shape for Linked PRs before Phase 12 (opaque id + display placeholder vs empty state copy only)
- Whether hard-delete requires typing the issue number (GitHub-style confirm)
- Exact RPC naming (`issue.*` vs `repo.issues.*`) and migration number after orgs/ACL
- Milestone/project fields: omit (out of scope)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 11 goal, ISS-01…04
- `.planning/REQUIREMENTS.md` — ISS-01…04 wording
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — ACL capabilities, owner slug, collaborator ladder
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — repo chrome / owner-repo routes

### Code mirrors
- `crates/oxidean-api/src/repo/acl.rs` — `Capability`, resolve helpers for read/write/admin
- `crates/oxidean-api/src/rpc.rs` — procedure dispatch pattern
- `crates/oxidean-api/src/repo/mod.rs` — domain RPC module pattern to mirror for `issue`
- `apps/web/src/components/repo/repo-chrome.tsrx` — Issues tab omitted until this phase
- `apps/web/src/routes/$owner.$repo*.tsrx` — repo-scoped route shell
- `apps/web/src/lib/markdown.ts` — `renderGfm` / sanitize for bodies and comments
- `apps/web/src/components/repo/collaborators-panel.tsrx` — list + mutation Query patterns

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `renderGfm()` — issue/comment body HTML
- Repo SSR helpers — cookie-forward `repo.get` then issue RPCs
- Collaborators / members panels — assign picker + autocomplete patterns
- Repo capability flags (`can_write` / `can_admin`) — gate New issue, label settings, delete

### Established Patterns
- Nested RPC names (`repo.collaborators.*`, `org.members.*`) — prefer `issue.*` or `repo.issues.*` consistently
- Soft not-found for private ACL denials
- Octane `.tsrx` + TanStack Query for server state; local `useState` for forms

### Integration Points
- Repo chrome nav → Issues tab
- Future Phase 12 PRs consume link table + replace stubs
- Org settings may need label catalog UI for org defaults (D-ISS-05)

</code_context>

<specifics>
## Specific Ideas

- Prefer GitHub/Gitea parity for lifecycle (open/close + admin delete) and list IA
- Linked PRs panel is allowed to be stub-backed until Phase 12
- Full edit history on both issues and comments (user explicitly chose parity over thinner v1)

</specifics>

<deferred>
## Deferred Ideas

- Closing keywords on merge → Phase 12
- Full PR objects / review / merge → Phase 12
- Issue activity notifications → Phase 17
- Full GitHub search grammar (`is:open label:bug`) — later polish / Phase 16 adjacency
- Projects, milestones, issue templates — not in current v1 phase set (note if product later wants them)

</deferred>

---

*Phase: 11-issues*
*Context gathered: 2026-09-14*
