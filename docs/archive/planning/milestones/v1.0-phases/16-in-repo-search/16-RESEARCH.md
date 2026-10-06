# Phase 16: In-Repo Search - Research

**Researched:** 2026-09-16  
**Domain:** Permission-aware repository search (code, commits, issues, PRs)  
**Confidence:** HIGH (codebase ACL/git/issues patterns + Gitea git-grep precedent); MEDIUM (exact Phase 12 PR table names until Phase 12 executes)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
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

### Claude's Discretion (resolved below)
- Soft caps / timeout defaults → see Standard Stack ENV
- Issue search: **title/body only** in Phase 16 (comments deferred — cheaper + matches issue list `q`)
- Chrome: compact search icon/control next to title row that routes to `/search`
- UI: default-branch only in v1 UI; RPC accepts optional `ref`
- Empty `q` → empty results (200), not error; invalid `type` → `rpc.bad_input`
- Unknown qualifiers → **stripped** (not treated as literal keywords)

### Deferred Ideas (OUT OF SCOPE)
- Global header search
- Zoekt / Elasticsearch / Tantivy background indexing
- Full GitHub search grammar
- Cross-repo / org search
- Semantic search
- Packages / releases / Actions / wikis in search
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| GIT-18 | User can search code, commits, issues, and PRs within a repository they can read | `repo.search` + git grep/log + issue/PR DB filters + Read ACL + Octane `/search` UI |
</phase_requirements>

## Summary

GitHub’s hosted search uses a multi-shard indexer and rich grammar. Self-hosted forges (notably **Gitea**) ship **repo-level code search via `git grep` without an indexer**, with optional Bleve/Elasticsearch for scale. Oxidean already stores bare repos on disk behind `CliGitBackend`, has paged `git log`, and has issue title/body ILIKE filters — the natural Phase 16 path is **on-demand git grep + git log search + DB issue/PR search**, gated by existing `Capability::Read`, exposed as **`repo.search`**, with a GitHub-shaped repo search page.

**Primary recommendation:** Extend `GitBackend` with `grep` + `log_search`; implement `repo.search` dispatching by `type`; reuse issue filters and Phase 12 PR rows; ship Octane `/{owner}/{repo}/search` with type tabs. No new search crates or Compose services.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| ACL + soft not_found | API / Backend | — | Same as repo browse / issue.list |
| Code grep / commit search | API / Backend | Database / Storage (bare repo FS) | Git CLI on bare path; no browser git |
| Issue / PR text search | Database / Storage | API / Backend | Dialect SQL LIKE/ILIKE in `oxidean-db` |
| Qualifier parsing | API / Backend | — | Single shared parser before backends |
| Search results UI | Browser / Client | Frontend Server (SSR) | Octane route + TanStack Query; optional SSR preload |
| Global header search | — | — | Explicitly out of scope (stub remains) |

## Project Constraints (from .cursor/rules/)

- One product; Bun + Cargo — no parallel app structure. [VERIFIED]
- Octane `.tsrx` for UI; TanStack Query for server data. [VERIFIED]
- RPC: Rust → `make rpc-gen`. [VERIFIED]
- Dialect SQL only in `oxidean-db`. [VERIFIED]
- Prefer extending `GitBackend` / ACL / issue filters over new frameworks. [VERIFIED]

## Standard Stack

### Core

| Library / Component | Version / Location | Purpose | Why Standard |
|---------------------|--------------------|---------|--------------|
| `GitBackend` / `CliGitBackend` | `crates/oxidean-git` | `git grep`, `git log --grep` / `--author` | Existing CLI adapter; Gitea-proven; GIT-09/10 swappable [VERIFIED: `backend.rs`] |
| Axum RPC `repo.*` | `oxidean-api` | `repo.search` | Matches browse RPCs [VERIFIED: `rpc.rs`] |
| `Capability::Read` | `repo/acl.rs` | Permission gate | Phase 10 |
| `IssueListFilters.q` | `oxidean-db/src/issues.rs` | Issue substring search | Already shipped [VERIFIED] |
| Phase 12 PR tables | (execute-time) | PR title/body search | D-PR-02 shared `#N` [ASSUMED from 12-CONTEXT] |
| Octane + TanStack Query | `apps/web` | Search UI | Project UI stack |

### Supporting

| Component | Purpose | When |
|-----------|---------|------|
| `OXIDEAN_SEARCH_TIMEOUT_MS` | Cap git subprocess wall time | Default **8000** |
| `OXIDEAN_SEARCH_MAX_MATCHES` | Cap code/commit hits returned | Default **100** |
| `OXIDEAN_SEARCH_MAX_FILES` | Cap distinct code files in page | Default **50** |
| Qualifier parser (in-tree) | Split `is:` / `author:` / `path:` | Small pure Rust module — do not add a search-DSL crate |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `git grep` | Zoekt / Bleve / ES | Better scale; new ops, disk, index lag — deferred (D-SRCH-06) |
| `git log --grep` | Index commit messages in DB on push | Faster repeat queries; write-path complexity — deferred |
| Single `repo.search` | Separate `repo.searchCode` RPCs | More codegen surface; unified type tabs favor one method (D-SRCH-14) |
| Full GitHub grammar | Modest qualifiers | Phase 11 already deferred full grammar (D-SRCH-13) |

**Installation:** none — no new npm/crates.io packages. [Package Legitimacy: N/A]

## Package Legitimacy Audit

| Package | Registry | Status | Notes |
|---------|----------|--------|-------|
| _(none)_ | — | — | Phase uses in-tree git CLI + sqlx only |

## Architecture Patterns

### Data flow

```
Browser /search?q&type
  → TanStack Query → RPC repo.search
    → resolve_repo_for_read (ACL)
    → parse_qualifiers(q)
    → match type:
         code    → GitBackend::grep(ref, path?, terms) → CodeHit[]
         commits → GitBackend::log_search(ref, author?, grep) → CommitHit[]
         issues  → db search issues (state/author/q)
         pulls   → db search pull_requests (state/author/q)
    → { hits, truncated?, total? }
```

### `git grep` notes (Gitea-aligned)

- Run against bare repo with tree-ish: `git --git-dir=<bare> grep -n -I -e <pat> <treeish> -- <pathspec?>`
- `-I` skips binary; exit code 1 = no matches → empty vec, not error
- Context lines: optional `-C 1` for snippet UX (discretion: ship 1 line context)
- Kill/timeout on large repos — return truncated + `search.timeout` only if the process was aborted mid-flight; otherwise `truncated: true` when hitting max matches

### Commit search

- Prefer `git log <ref> --grep=<q> --regexp-ignore-case --pretty=…` with optional `--author=`
- Page with `--skip` / `-n` aligned to existing `log` paging

### Issues / PRs

- Issues: reuse `list_for_repo` filters or a dedicated `search_for_repo` that always applies `q` + state from `is:`
- PRs: mirror issue filter shape on Phase 12 tables; **exclude** issue rows from pulls tab and vice versa (D-SRCH-11). If Phase 12 stores PRs in a separate `pull_requests` table, query that; if shared `issues` with `kind`, filter `kind=pr` — follow Phase 12 schema at execute time (precondition).

### UI

- Route: `apps/web/src/routes/$owner.$repo.search.tsrx`
- Chrome: search control → `/${owner}/${name}/search`
- Tabs update `type` query param without losing `q`
- Hits link to existing blob (`/blob/...`), commit, issue, PR routes (PR routes from Phase 12)

## Common Pitfalls

| Pitfall | Mitigation |
|---------|------------|
| Enumerating private repos via search errors | Soft `repo.not_found` identical to `repo.get` |
| Hanging RPC on huge trees | Timeout + max matches (D-SRCH-08) |
| Treating git grep exit 1 as failure | Map to empty results |
| Path injection / option injection in grep args | Validate ref/path like existing `validate_treeish` / `validate_repo_rel_path` |
| Wiring GlobalSearch accidentally | Explicit prohibition D-SRCH-01 |
| Mixing issues+PRs in one query | Separate types D-SRCH-11 |
| Hand-editing api-client | Always `make rpc-gen` |
| Executing before Phase 12 PR tables exist | Plan precondition; parallel track says execute after 12 |

## Validation Architecture

| Req | Behavior | Command |
|-----|----------|---------|
| GIT-18 code | Read user finds seeded file content via `repo.search` type=code | `cargo nextest run -p oxidean-api -E 'test(repo_search_code)'` |
| GIT-18 commits | Message/author hit via type=commits | `… test(repo_search_commits)` |
| GIT-18 issues | Title/body hit via type=issues | `… test(repo_search_issues)` |
| GIT-18 PRs | Title/body hit via type=pulls | `… test(repo_search_pulls)` |
| GIT-18 ACL | Private unauthorized → soft not_found | `… test(repo_search_acl)` |
| UI | Search route + tabs | Vitest `$owner.$repo.search.integration.test.ts` |

## Open Questions (RESOLVED)

| Question | Resolution |
|----------|------------|
| Indexer vs git grep? | **git grep** (D-SRCH-06); Gitea builtin path |
| Include issue comments? | **Title/body only** Phase 16 |
| Unknown qualifiers? | **Strip** |
| Default timeout/caps? | **8000 ms / 100 matches / 50 files** |
| `RepoChromeActive` for search? | Add **`"search"`** highlight when on `/search` |
| Phase 12 PR schema? | **ASSUME** at execute: follow landed Phase 12 migrations/RPC; shared `#N` (D-PR-02) |

## Sources

- Gitea docs: repository indexer + builtin git grep — https://docs.gitea.com/administration/repo-indexer/
- Gitea PR #29998 — repo code search without indexer
- GitHub search REST overview — https://docs.github.com/en/rest/search/search
- In-tree: `crates/oxidean-git`, `crates/oxidean-db/src/issues.rs`, `apps/web/src/components/global-search.tsrx`, `12-CONTEXT.md`

**Research date:** 2026-09-16  
Research complete — ready for planning.
