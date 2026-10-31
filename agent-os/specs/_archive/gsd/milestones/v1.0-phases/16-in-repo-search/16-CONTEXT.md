# Phase 16: In-Repo Search - Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss mode:** Gray areas auto-locked to **GitHub-like in-repo search** per campaign defaults (2026-09-16). No interactive discuss — user directed auto-decide.

<domain>
## Phase Boundary

Users can search **code, commits, issues, and pull requests** inside a repository they can read. Delivers **GIT-18**.

**Requirements:** GIT-18

**Success criteria (from ROADMAP):**
1. User can search code and commits within a repository they can read
2. User can search issues and PRs within a repository they can read

**Depends on:** Phase 11 (issues), Phase 12 (pull requests). Execute after Phase 12 PR domain lands; plan assumes `12-CONTEXT.md` PR model (shared `#N`, forks, Capability ACL).

**Out of scope:**
- Instance-wide / global forge search (header `GlobalSearch` stays disabled “Coming soon”)
- Full GitHub search grammar / code-search regex / semantic search
- Background code indexer (Zoekt, Elasticsearch, Tantivy, etc.)
- Cross-repo or org-scoped search
- Searching packages, releases, wikis, discussions, Actions runs
- Notifications for search (N/A)

**UI hint:** yes — repo-scoped search entry + `/{owner}/{repo}/search` results with type tabs.

</domain>

<decisions>
## Implementation Decisions

### A — Scope & entry (GitHub-like, repo-only)
- **D-SRCH-01:** Search is **repository-scoped only** — route `/{owner}/{repo}/search?q=…&type=…`. Do **not** wire the header `GlobalSearch` in this phase — **Reversibility:** reversible
- **D-SRCH-02:** Result **type tabs**: **Code | Commits | Issues | Pull requests** (GitHub repo search IA). Default type when omitted: **Code** — **Reversibility:** reversible
- **D-SRCH-03:** Entry points: **search control in repo chrome** (or title-row adjacent control) that navigates to the search route with focus on `q`; deep-linkable query string — **Reversibility:** reversible

### B — Permissions
- **D-SRCH-04:** Require **`Capability::Read`** (reuse Phase 10 ACL). Public repos: **anonymous** search allowed. Private: unauthenticated / unauthorized → soft **`repo.not_found`** (anti-enumeration) — **Reversibility:** reversible
- **D-SRCH-05:** Results never leak objects from repos the actor cannot read; PR/issue hits respect the same Read gate as list/get — **Reversibility:** reversible

### C — Code & commit backends (no external indexer)
- **D-SRCH-06:** **Code search** via **`git grep`** (or equivalent) through **`GitBackend`** on a chosen **tree-ish** (default: **default branch** tip; optional `ref` query param). No Zoekt/Elasticsearch/Meilisearch in Phase 16 — **Reversibility:** costly — swapping to an indexer later changes ops + API SLAs
- **D-SRCH-07:** **Commit search** via **`git log`** message/`--grep` and **author** filters through **`GitBackend`** (extend CLI adapter; keep trait swappable per GIT-09/10) — **Reversibility:** costly — trait surface growth
- **D-SRCH-08:** Skip **binary** blobs for code hits; enforce **soft caps** (max matches, max files scanned/returned) and a **server-side timeout** so large repos fail soft (`search.timeout` / truncated flag) rather than hang the RPC — **Reversibility:** reversible

### D — Issues & PRs (DB; Phase 12 model)
- **D-SRCH-09:** **Issues** search: title/body (and comment text if cheap) substring match in **`oxidean-db`**, building on Phase 11 `IssueListFilters.q` / ILIKE patterns — **Reversibility:** reversible
- **D-SRCH-10:** **Pull requests** search: same pattern against **Phase 12 PR persistence** (shared per-repo `#N` with issues per **D-PR-02**; title/body). Assume PR tables/RPC from `.planning/phases/12-pull-requests/12-CONTEXT.md` at execute time — **Reversibility:** reversible
- **D-SRCH-11:** Issue vs PR tabs are **separate types** (GitHub). Do not return mixed issue+PR rows in one tab — **Reversibility:** reversible

### E — Query syntax (modest GitHub subset)
- **D-SRCH-12:** Support bare keywords plus a **small qualifier set**: `is:open` / `is:closed` (Issues & PRs), `author:<login>` (Commits / Issues / PRs), `path:<prefix>` (Code). Unknown qualifiers are ignored or treated as literal text (document choice in RESEARCH/plans) — **Reversibility:** reversible
- **D-SRCH-13:** Full GitHub code-search grammar, regex, `language:`, `org:`, cross-repo `repo:` — **deferred** (Phase 11 already parked full grammar) — **Reversibility:** reversible (deferral)

### F — API & UI contract
- **D-SRCH-14:** Single nested RPC family **`repo.search`** with `type` enum (`code` | `commits` | `issues` | `pulls`), `q`, optional `ref`, offset/limit — change Rust → `make rpc-gen` — **Reversibility:** costly — published RPC shape
- **D-SRCH-15:** Octane results page: query input, type tabs, hit lists linking to blob/commit/issue/PR detail routes; empty and truncated states — **Reversibility:** reversible
- **D-SRCH-16:** Preserve layout-owned **`RepoChrome`** (Phase 11.1); add search entry without remounting chrome in leaves; extend `RepoChromeActive` only if a dedicated highlight is needed (search may keep Code active or add `"search"`) — **Reversibility:** reversible

### Claude's Discretion
- Exact soft-cap numbers and timeout duration (document in RESEARCH / env knobs if operator-tunable)
- Whether issue search includes comment bodies in Phase 16 or title/body only
- Exact chrome control placement (icon vs compact input)
- Whether `ref` is exposed in UI in Phase 16 or default-branch-only in UI with RPC support ready
- RPC error code strings for empty `q`, timeout, invalid type
- Whether unknown qualifiers are stripped vs literal

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 16 goal, GIT-18, depends on 11+12
- `.planning/REQUIREMENTS.md` — GIT-18 wording
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — PR identity, shared `#N`, ACL (D-PR-01…29)
- `.planning/phases/11-issues/11-CONTEXT.md` — issue list filters / soft not-found; deferred full search grammar note
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Capability ACL
- `.planning/phases/11.1-quality-hardening/11.1-CONTEXT.md` — RepoChrome layout ownership (D-QH-01)
- `.planning/parallel-tracks/phase-16.md` — execute after Phase 12 lands

### Product / stack
- `docs/ARCHITECTURE.md` — git / ACL surfaces
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` UI (mandatory for UI plans)
- `crates/oxidean-git/src/backend.rs` — `GitBackend` trait to extend
- `crates/oxidean-db/src/issues.rs` — `IssueListFilters.q` ILIKE pattern
- `apps/web/src/components/global-search.tsrx` — global search stays out of scope
- `apps/web/src/components/repo/repo-chrome.tsrx` — repo entry chrome

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `GitBackend::{log, show_commit, ls_tree, cat_blob, blame}` — browse primitives; **no grep yet**
- `issue.list` + `IssueListFilters.q` — substring title/body search
- `resolve_repo_for_read` / `Capability::Read` — ACL + soft not_found
- Issues list UI search field pattern (`issues-list.tsrx`)
- Repo chrome + `repoChromeActiveFromPath`

### Established Patterns
- Nested RPC (`repo.*`, `issue.*`) + `make rpc-gen`
- Dialect SQL only in `oxidean-db`
- Octane `.tsrx` + TanStack Query; no Zustand for server data

### Integration Points
- New `/{owner}/{repo}/search` route + chrome control
- Phase 12 PR list/get tables for pulls type (execute-time dependency)
- Optional ENV: search timeout / max matches (CONFIGURATION.md)

</code_context>

<specifics>
## Specific Ideas

- Campaign locked **GitHub-like** UX (tabs + permission-aware) without requiring GitHub’s hosted indexer
- Prefer **on-demand `git grep` / `git log`** to stay within existing CLI git adapter and Compose footprint
- Assume Phase 12 PR model verbatim; do not re-litigate forks, shared `#N`, or review UX here

</specifics>

<deferred>
## Deferred Ideas

- Global header search (users/orgs/repos/code across instance)
- Zoekt / Elasticsearch / Tantivy background indexing
- Full GitHub search grammar (`language:`, regex, `label:`, `assignee:`, etc.)
- Cross-repo / org search
- Semantic / Copilot-style search
- Searching packages, releases, Actions, wikis

</deferred>

---

*Phase: 16-in-repo-search*  
*Context gathered: 2026-09-16*
