# Phase 11: Issues - Research

**Researched:** 2026-09-14
**Domain:** Repo-scoped issues (CRUD lifecycle, comments, labels, assignees, reactions, markdown autolink, PR link stubs) on Oxidean Rust RPC + Octane UI + multi-dialect DB
**Confidence:** HIGH (codebase patterns) / MEDIUM (forge numbering & label-merge discretion)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-ISS-01:** **Per-repo sequential `#N`** issue numbers (GitHub-style URLs `/{owner}/{repo}/issues/{n}`) — **Reversibility:** one-way — public URLs + DB sequence per repo
- **D-ISS-02:** Lifecycle is **open ↔ closed** (reopen allowed). **Repo Admin** may **hard-delete** an issue (GitHub/Gitea parity; confirm in UI) — **Reversibility:** costly — delete cascades comments/reactions/links
- **D-ISS-03:** **Author + Write+** may edit title/body anytime after create — **Reversibility:** reversible
- **D-ISS-04:** **Full edit history** for issue title/body (diff trail, not timestamp-only) — **Reversibility:** costly — history tables + UI
- **D-ISS-05:** Labels are **org defaults + per-repo overrides** (org-level catalog with repo-local customize) — **Reversibility:** costly — dual-scope schema + settings UI
- **D-ISS-06:** **Multiple assignees** per issue — **Reversibility:** reversible
- **D-ISS-07:** **Write+** can assign labels/assignees on an issue; only **Admin** can create/edit/delete **label definitions** — **Reversibility:** reversible
- **D-ISS-08:** Assignee eligibility = anyone with **Read+** on the repo (collaborators + org members with access) — **Reversibility:** reversible
- **D-ISS-09:** Author can edit/delete own comments; **Write+** can delete others’ comments (moderation) — **Reversibility:** reversible
- **D-ISS-10:** Markdown authoring uses **Write | Preview** tabs; reuse existing `renderGfm` + sanitize pipeline — **Reversibility:** reversible
- **D-ISS-11:** **GitHub-style emoji reactions** on issues and comments in Phase 11 — **Reversibility:** costly — reaction tables + RPC
- **D-ISS-12:** **Full edit history** on comments (match issues) — **Reversibility:** costly — history tables + UI
- **D-ISS-13:** Phase 11 ships **`#N` and `owner/repo#N` autolink** in markdown plus a **Linked PRs** sidebar/panel that may hold **stubs** until Phase 12 PR objects exist — **Reversibility:** costly — link table + stub UX
- **D-ISS-14:** Also ship a **manual “Link issue/PR”** control that writes stub (or real) links — **Reversibility:** reversible
- **D-ISS-15:** **Closing keywords** (`fixes` / `closes` `#N`) **wait for Phase 12** merge — do not enforce auto-close from commits in Phase 11 — **Reversibility:** reversible (deferral)
- **D-ISS-16:** Issues list defaults to **Open**; **Closed** and **All** available — **Reversibility:** reversible
- **D-ISS-17:** Phase 11 filters: **author, label, assignee, + text search** (not full GitHub search grammar) — **Reversibility:** reversible
- **D-ISS-18:** Sort **newest-updated first**; **offset pagination** (not infinite scroll) — **Reversibility:** reversible
- **D-ISS-19:** Add **Issues** tab to repo chrome; **New issue** from list and empty state — **Reversibility:** reversible
- **D-ISS-20:** Reuse Phase 10 `Capability::{Read,Write,Admin}`: **Read** → list/view; **Read + verified sign-in** → create issues, comment, react (git-forge parity — public participation does not require collaborator status); **author or Write+** → edit title/body, close/reopen, delete own comment; **Write+** → assign, label-on-issue, links, moderate-delete others' comments; **Admin** → label definition CRUD + hard-delete issues. Keep web **`repo.not_found`** anti-enumeration for private repos.

### Claude's Discretion
- Exact reaction emoji set (match a small GitHub subset vs full)
- Exact org-label vs repo-label override merge rules (inherit + hide + local-only)
- Exact stub shape for Linked PRs before Phase 12 (opaque id + display placeholder vs empty state copy only)
- Whether hard-delete requires typing the issue number (GitHub-style confirm)
- Exact RPC naming (`issue.*` vs `repo.issues.*`) and migration number after orgs/ACL
- Milestone/project fields: omit (out of scope)

### Deferred Ideas (OUT OF SCOPE)
- Closing keywords on merge → Phase 12
- Full PR objects / review / merge → Phase 12
- Issue activity notifications → Phase 17
- Full GitHub search grammar (`is:open label:bug`) — later polish / Phase 16 adjacency
- Projects, milestones, issue templates — not in current v1 phase set (note if product later wants them)
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ISS-01 | User can create, edit, close, and reopen issues | Per-repo `#N` counter + `issues` table; ACL Write for mutate / Admin hard-delete; open↔closed lifecycle; edit history tables |
| ISS-02 | User can comment on issues | `issue_comments` + comment revisions; author edit/delete; Write+ moderate delete; `renderGfm` Write\|Preview |
| ISS-03 | User can assign labels and assignees to issues | Dual-scope labels (org+repo); M:N assignees; Write+ assign / Admin label defs; assignee eligibility = Read+ |
| ISS-04 | User can link issues and PRs by reference | Markdown `#N` / `owner/repo#N` autolink; `issue_links` with PR stubs; manual link control; no closing-keyword enforcement |
</phase_requirements>

## Summary

Phase 11 adds the first collaboration surface on top of Phase 10 ACL: repo-scoped issues with GitHub-shaped numbers, conversation, labels/assignees, reactions, and forward-compatible PR links. Implementation should mirror existing Oxidean seams — `resolve_repo_for_read` / `meets(Capability)`, nested RPC dispatch in `rpc.rs`, dialect SQL only in `oxidean-db`, Octane `.tsrx` + TanStack Query, and `renderGfm` with **sanitize last**.

No new backend frameworks are required. The only likely new npm dependency is `remark-github` (for `#N` / `owner/repo#N` autolinks with a custom `buildUrl` into Oxidean paths); existing remark/rehype packages already power README rendering. Schema work is a new `0011_issues` migration (after `0010_orgs_acl`) covering issues, counters, comments, revisions, labels, assignees, reactions, and link stubs — wired so factory reset continues to wipe via `DELETE FROM repositories` cascades.

**Primary recommendation:** Ship a dedicated `issue` API module (`issue.*` RPCs), Gitea-style `issue_counters` for non-reusing `#N`, org∪repo label effective-set with hide/local-only overrides, GitHub’s full 8-reaction content enum, typed `#N` confirm for hard-delete, and opaque PR stub rows consumed by a Linked PRs panel until Phase 12.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Issue CRUD / close / reopen / hard-delete | API / Backend | Database / Storage | ACL + numbering + history must be authoritative server-side |
| Comments + comment history | API / Backend | Browser / Client | Persist + authorize on API; Write\|Preview and render in browser |
| Label definitions (org + repo) | API / Backend | Browser / Client | Admin-only defs; settings UI under org/repo |
| Label/assignee assignment | API / Backend | Browser / Client | Write+ mutations; pickers use Query + MemberLookup-like UX |
| Reactions | API / Backend | Browser / Client | Toggle rows server-side; emoji UI client |
| Markdown render + `#N` autolink | Browser / Client | CDN / Static | Reuse `renderGfm` pipeline; no server HTML store |
| Linked PR stubs / manual links | API / Backend | Browser / Client | Link table owns truth; sidebar displays stubs until Phase 12 |
| Issues list filters / pagination | API / Backend | Database / Storage | Offset queries + dialect text search in `oxidean-db` |
| Repo chrome Issues tab / routes | Browser / Client | Frontend Server (SSR) | Extend `repo-chrome`; SSR cookie-forward like other repo pages |
| Anti-enumeration private ACL | API / Backend | — | Identical `repo.not_found` / soft issue not-found |

## Project Constraints (from `.cursor/rules/`)

| Rule | Directive |
|------|-----------|
| `oxidean-core.mdc` | One product; Bun + Cargo; Octane `.tsrx` not React; `make rpc-gen` for client; dialect SQL only in `oxidean-db`; no secrets; prefer existing patterns |
| `octane-ui.mdc` | `.tsrx` with `@{` / `@if`/`@else` (no `@else if`) / `@for`; `onInput` for text; TanStack Query via session helpers; no `react`→Octane alias |
| `rpc-codegen.mdc` | Rust types authoritative; regenerate api-client; `make rpc-sync-check` clean; stable error codes |
| `rust-crates.mdc` | core = types; db = SQL; api = handlers; `Result` not unwrap in prod; preserve auth gates; destructive RPCs need explicit confirmation |

## Standard Stack

### Core

| Library / Component | Version | Purpose | Why Standard |
|---------------------|---------|---------|--------------|
| Axum RPC + `oxidean-api` | in-repo | `issue.*` procedures | Existing dispatch pattern `[VERIFIED: crates/oxidean-api/src/rpc.rs:66-355]` |
| `Capability` ACL | in-repo | Read/Write/Admin gates | Phase 10 ladder `[VERIFIED: crates/oxidean-api/src/repo/acl.rs:24-28]` quote: `Read = 1, Write = 2, Admin = 3` |
| `oxidean-db` migrations | `0011_issues` (next) | Schema + dialect SQL | Latest shipped is `0010_orgs_acl` `[VERIFIED: crates/oxidean-db/migrations/sqlite/0010_orgs_acl.sql:1-2]` |
| Octane `.tsrx` + `@octanejs/tanstack-query` | in-repo | Issues UI | Project UI stack `[VERIFIED: AGENTS.md]` |
| `renderGfm` (unified + remark-gfm + rehype-sanitize) | remark-gfm **4.0.1**, rehype-sanitize **6.0.0** | Body/comment HTML | Already shipped D-18 pipeline `[VERIFIED: apps/web/src/lib/markdown.ts:12-20]` `[VERIFIED: npm registry via apps/web/package.json]` |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `remark-github` | **12.0.0** | Autolink `#N`, `owner/repo#N` (and related refs) with custom `buildUrl` | Extend `renderGfm` for ISS-04 markdown links — keep **rehype-sanitize last** `[CITED: github.com/remarkjs/remark-gfm]` `[VERIFIED: npm registry]` legitimacy **OK** |
| Base UI AlertDialog / Input / Textarea / Button / Select | in-repo | Forms, confirm delete, pickers | Match collaborators / soft-delete UX |
| `MemberLookup` | in-repo | Username autocomplete base | Assignees — **server must still enforce Read+ eligibility** `[VERIFIED: apps/web/src/components/org/member-lookup.tsrx:19-22]` |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `remark-github` + custom `buildUrl` | Hand-rolled rehype text rewriter for `#N` only | Fewer deps, but reimplements edge cases remarkjs already solved — avoid |
| `issue.*` RPC module | `repo.issues.*` nested under repo | Nesting matches collaborators naming; dedicated `issue.*` keeps `rpc.rs`/`repo/mod.rs` from ballooning — **prefer `issue.*`** |
| Counter table (`issue_counters`) | `MAX(number)+1` only | Race-prone under concurrency; hard-delete would risk reuse — counter table is forge-standard `[CITED: gitea PR #15599 / models/db/index.go]` |
| Full Gitea org+repo duplicate labels | Strict name-unique across scopes | CONTEXT wants overrides; recommend inherit+hide+local-only rather than duplicate-name pairs |

**Installation (only if planner accepts new dep):**

```bash
cd apps/web && bun add remark-github@12.0.0
```

**Version verification:** `remark-gfm@4.0.1`, `rehype-sanitize@6.0.0`, `remark-github@12.0.0` confirmed via `npm view` this session. No other new packages recommended.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| remark-gfm | npm | multi-year | ~29M/wk | github.com/remarkjs/remark-gfm | OK | Already installed — Approved |
| rehype-sanitize | npm | multi-year | ~7.7M/wk | github.com/rehypejs/rehype-sanitize | OK | Already installed — Approved |
| unified | npm | multi-year | ~40M/wk | github.com/unifiedjs/unified | OK | Already installed — Approved |
| remark-parse | npm | multi-year | ~37M/wk | github.com/remarkjs/remark | OK | Already installed — Approved |
| remark-rehype | npm | multi-year | ~32M/wk | github.com/remarkjs/remark-rehype | OK | Already installed — Approved |
| rehype-stringify | npm | multi-year | ~6.1M/wk | github.com/rehypejs/rehype | OK | Already installed — Approved |
| remark-github | npm | since 2015 | ~530k/wk | github.com/remarkjs/remark-github | OK | Approved for install (no postinstall) |

**Packages removed due to [SLOP] verdict:** none  
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
Browser (Octane .tsrx)
  │  Issues tab / list / detail / new / labels settings
  │  Write|Preview → renderGfm(+remark-github) → rehype-sanitize
  │  TanStack Query mutations (create/comment/assign/react/link)
  ▼
SSR (TanStack Start cookie-forward)
  │  fetchRepoGet → then issue.list / issue.get
  ▼
RPC dispatch (rpc.rs) ──► issue::* handlers
  │                         │
  │                         ├─ resolve_repo_for_read / meets(Read|Write|Admin)
  │                         ├─ allocate #N via issue_counters (txn)
  │                         ├─ revisions / reactions / links
  │                         └─ soft not_found for private denials
  ▼
oxidean-db (dialect SQL only)
  issues, issue_counters, issue_comments, *_revisions,
  labels (+ org/repo scope), issue_labels, issue_assignees,
  issue_reactions, comment_reactions, issue_links
  │
  └─ ON DELETE CASCADE from repositories → factory_reset safe
```

### Recommended Project Structure

```
crates/oxidean-db/migrations/{sqlite,postgres,mysql}/0011_issues.sql
crates/oxidean-db/src/issues.rs          # CRUD + list filters + counters
crates/oxidean-db/src/issue_labels.rs
crates/oxidean-core/src/issue_types.rs   # DTOs / enums for rpc-gen
crates/oxidean-api/src/issue/mod.rs      # RPC handlers
crates/oxidean-api/src/issue/acl.rs      # thin helpers wrapping repo::acl
crates/oxidean-api/tests/issue_*.rs
apps/web/src/routes/$owner.$repo.issues*.tsrx
apps/web/src/components/repo/issues-*.tsrx
apps/web/src/lib/markdown.ts              # extend renderGfm
```

### Pattern 1: ACL gate before every issue mutation
**What:** Resolve repo via `resolve_repo_for_read`, then `meets(capability, Need)`.
**When to use:** All `issue.*` RPCs.
**Example:**

```rust
// Source: crates/oxidean-api/src/repo/acl.rs (Capability + meets + resolve_repo_for_read)
let accessible = resolve_repo_for_read(ctx, &owner, &name).await?;
if !meets(accessible.capability, Capability::Write) {
    return Err(not_found()); // or issue.forbidden mapped to soft not_found for private
}
```

Verbatim Capability enum `[VERIFIED: crates/oxidean-api/src/repo/acl.rs:24-28]`:

```rust
pub enum Capability {
    Read = 1,
    Write = 2,
    Admin = 3,
}
```

### Pattern 2: Per-repo sequential `#N` via counter table
**What:** `issue_counters(repo_id PK, max_number)` upserted in the same transaction as insert; `issues.number` UNIQUE `(repo_id, number)`; hard-delete does **not** decrement (avoids URL reuse).
**When to use:** `issue.create` only.
**Why:** Matches Gitea `issue_index` / `ResourceIndex` approach `[CITED: github.com/go-gitea/gitea models/db/index.go + PR #15599]`.

### Pattern 3: Markdown Write | Preview + sanitize last
**What:** Local `useState` for draft; Preview calls `renderGfm`; never store HTML.
**When to use:** New/edit issue, comments.
**Pipeline:** `remarkParse → remarkGfm → remarkGithub({buildUrl}) → remarkRehype → rehypeSanitize → rehypeStringify` `[VERIFIED: apps/web/src/lib/markdown.ts:12-20]` plus recommended remark-github.

### Pattern 4: Query list + mutation invalidate (collaborators style)
**What:** `useQuery` key `["issue", owner, name, filters…]`; mutations invalidate list + detail.
**When to use:** All issue panels. Mirror `collaborators-panel.tsrx` `[VERIFIED: apps/web/src/components/repo/collaborators-panel.tsrx:49-51]`.

### Pattern 5: Typed confirm for destructive Admin actions
**What:** Hard-delete requires `confirmNumber` matching issue `#N` (same spirit as `confirmName` on `repo.softDelete`).
**When to use:** Admin delete issue.
**Existing pattern** `[VERIFIED: crates/oxidean-api/src/repo/mod.rs:712-724]` — softDelete checks `confirm_name` against repo name.

### Anti-Patterns to Avoid
- **Hand-editing `@oxidean/api-client`:** always `make rpc-gen`.
- **Dialect SQL in `oxidean-api`:** list/search/LIKE belongs in `oxidean-db`.
- **Storing rendered HTML:** XSS and sanitize drift; store markdown only.
- **Global `user.lookup` as sole assignee source:** violates D-ISS-08; validate Read+ server-side.
- **Enforcing `fixes #N` on push/merge:** deferred to Phase 12 (D-ISS-15).
- **Mixing `return (` JSX with Rivet `@if`:** breaks Octane exports.
- **Reusing issue numbers after hard-delete:** breaks permalinks / autolinks.
- **Gating Issues tab on personal owner only:** chrome currently uses owner equality for Settings `[VERIFIED: apps/web/src/components/repo/repo-chrome.tsrx:20-22]`; Issues tab must use repo visibility/Read (and New issue via `repo.can_write`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GFM markdown → safe HTML | Custom markdown parser | Existing `renderGfm` + rehype-sanitize | XSS edge cases already solved (D-18) |
| `#N` / `owner/repo#N` autolink | Ad-hoc regex HTML post-process | `remark-github` + custom `buildUrl` | Official remark companion; keeps AST transforms before sanitize |
| Per-repo `#N` under concurrency | `SELECT MAX(number)+1` alone | `issue_counters` upsert in txn | Race + delete-reuse hazards |
| ACL decisions | New permission matrix | `Capability` + `meets` + `resolve_repo_for_read` | Phase 10 single source of truth |
| Username autocomplete UI | New combobox | `MemberLookup` (+ eligibility RPC) | Already rate-limited / no-email |
| Destructive confirm UX | Custom modal kit | Existing AlertDialog + typed confirm | Matches soft-delete / branch delete |
| RPC TypeScript types | Hand-written client | `make rpc-gen` | Drift is a CI failure mode |

**Key insight:** Issues are a thick domain module on top of existing ACL/markdown/Query seams — inventing a second ACL or markdown stack is the main rewrite risk.

## Common Pitfalls

### Pitfall 1: Number reuse after Admin hard-delete
**What goes wrong:** New issue gets an old `#N`; old links point at the wrong work item.
**Why it happens:** Using MAX(number) or decrementing counters on delete.
**How to avoid:** Monotonic `issue_counters.max_number`; UNIQUE(repo_id, number); never reclaim.
**Warning signs:** Tests that create → delete → create and assert same number.

### Pitfall 2: Private repo issue enumeration
**What goes wrong:** `issue.forbidden` vs `repo.not_found` reveals private repos.
**Why it happens:** Distinct error codes for ACL deny.
**How to avoid:** D-ISS-20 — soft not-found identical to Phase 7/10 for unauthorized private access.
**Warning signs:** Integration tests asserting different messages for missing vs unauthorized.

### Pitfall 3: Autolink before sanitize / raw HTML
**What goes wrong:** XSS via crafted markdown links or HTML.
**Why it happens:** Plugin order wrong or `dangerouslySetInnerHTML` without sanitize.
**How to avoid:** Keep **rehype-sanitize last**; only pass sanitized HTML into Readme-style panels.
**Warning signs:** Tests inserting `<script>` in issue body still execute.

### Pitfall 4: Assignee picker accepts any instance user
**What goes wrong:** Outside users assigned without Read+.
**Why it happens:** Reusing `user.lookup` without server eligibility check.
**How to avoid:** `issue.setAssignees` validates each user_id via effective_capability ≥ Read; prefer `issue.assigneeCandidates` list.
**Warning signs:** Assign succeeds for random username with no collab/org access.

### Pitfall 5: Label override ambiguity
**What goes wrong:** Duplicate “bug” org+repo labels confuse assignment and filters.
**Why it happens:** Copying Gitea’s allow-duplicates model while CONTEXT asked for overrides.
**How to avoid:** Effective set = inherited org labels (− hidden) ∪ repo-local; assign by label id; unique name per scope.
**Warning signs:** Filter by name matches two ids.

### Pitfall 6: Factory reset misses new tables
**What goes wrong:** Orphan issue rows after reset if FKs lack CASCADE / wipe order wrong.
**Why it happens:** New tables without `REFERENCES repositories(id) ON DELETE CASCADE` (or org labels without org cascade).
**How to avoid:** Cascade from repositories/organizations; extend `factory_reset_*` tests like `factory_reset_orgs.rs`.
**Warning signs:** dialect reset tests leave `issues` rows.

### Pitfall 7: Repo chrome Settings still owner-only while Issues uses can_write
**What goes wrong:** Org Admin sees Issues correctly but Settings still hidden (pre-existing chrome bug).
**Why it happens:** `settingsVisible = showSettings \|\| isOwner` `[VERIFIED: apps/web/src/components/repo/repo-chrome.tsrx:20-22]`.
**How to avoid:** For Phase 11, gate Issues on presence of repo; New issue on `repo.can_write`; label settings on `repo.can_admin`. Optionally fix Settings visibility to `can_admin` in the same chrome edit (small, related).

## Code Examples

### Extend markdown with instance-local issue URLs

```typescript
// Source: extend apps/web/src/lib/markdown.ts; remark-github API from npm readme
import remarkGithub from "remark-github";

export async function renderGfm(
  markdown: string,
  opts?: { owner?: string; repo?: string },
): Promise<string> {
  const file = await unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkGithub, {
      repository: `${opts?.owner ?? "owner"}/${opts?.repo ?? "repo"}`,
      buildUrl(values) {
        if (values.type === "issue") {
          const o = values.user;
          const r = values.project;
          return `/${o}/${r}/issues/${values.no}`;
        }
        // mentions/commits: return false or relative paths as product chooses
        return false;
      },
    })
    .use(remarkRehype)
    .use(rehypeSanitize)
    .use(rehypeStringify)
    .process(markdown);
  return String(file);
}
```

### Reaction content enum (locked recommendation)

GitHub documents these eight values for issue/comment reactions `[CITED: docs.github.com/en/rest/reactions/reactions]`:

| Content | Emoji |
|---------|-------|
| `+1` | 👍 |
| `-1` | 👎 |
| `laugh` | 😄 |
| `confused` | 😕 |
| `heart` | ❤️ |
| `hooray` | 🎉 |
| `rocket` | 🚀 |
| `eyes` | 👀 |

**Recommendation (discretion):** Use the **full eight** — matches GitHub parity and avoids a second migration later.

### Suggested RPC surface (discretion: `issue.*`)

```
issue.list / issue.get / issue.create / issue.update
issue.close / issue.reopen / issue.delete          # delete: Admin + confirmNumber
issue.comments.list|create|update|delete
issue.comments.history / issue.history
issue.labels.set / issue.assignees.set
issue.assigneeCandidates
issue.reactions.toggle (target: issue|comment)
issue.links.list|add|remove
label.listForRepo / label.create|update|delete    # Admin defs; org scope via org settings
```

Migration filename: **`0011_issues`** (after `0010_orgs_acl`) `[VERIFIED: migrations list]`.

### RepoPublic capability flags for UI gates

`[VERIFIED: crates/oxidean-core/src/repo_types.rs:92-97]`:

```rust
/// Caller has Admin capability (D-ORG-05 / settings UI).
#[serde(default)]
pub can_admin: bool,
/// Caller has Write capability (D-ORG-05).
#[serde(default)]
pub can_write: bool,
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Owner-only private ACL stub | `Capability` coalesce ladder | Phase 10 | Issues must call shared ACL, not owner_id equality |
| README-only `renderGfm` | Same pipeline + remark-github for issue bodies | Phase 11 | Autolink without second sanitizer |
| Gitea MAX/index races | Dedicated `issue_index` counter table | Gitea ~2021 (#15599) | Adopt counter table for Oxidean |
| No Issues tab | Chrome Code/Commits/Branches/Tags/(Settings) | Phase 7 | Add Issues; keep PRs omitted until Phase 12 |

**Deprecated/outdated:**
- Treating `user.lookup` alone as collaborator/assignee authority — Phase 10 already scoped lookup anti-enumeration; assignees need repo Read+.

## Discretion Recommendations (planner locks)

| Topic | Recommendation | Confidence |
|-------|----------------|------------|
| Reaction set | Full GitHub 8 (`+1`…`eyes`) | HIGH `[CITED: docs.github.com]` |
| Label merge | Inherit org labels; repo may **hide** org label ids; repo may add **local-only**; unique name within scope; assign by id | MEDIUM (CONTEXT intent + avoid Gitea duplicate-name footgun) `[CITED: docs.gitea.com/usage/issues-prs/labels/]` as negative example |
| PR stub shape | Row: `kind=pr_stub`, `target_number`, optional `title`, opaque `id`; UI “Linked PRs” shows placeholder until Phase 12 replaces kind | HIGH for planning clarity |
| Hard-delete confirm | Type issue number `#N` / digits (mirror `confirmName`) | HIGH (matches existing destructive pattern) |
| RPC naming | Top-level `issue.*` + `label.*` for defs | MEDIUM |
| Migration | `0011_issues` | HIGH |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `remark-github` `buildUrl` values shape (`type`/`user`/`project`/`no`) matches current 12.x API closely enough for the skeleton | Code Examples | Executor must confirm against installed types; adjust field names |
| A2 | Org label “hide” override is the right interpretation of D-ISS-05 vs Gitea side-by-side duplicates | Discretion | User may prefer Gitea-style dual visible labels — discuss if product disagrees |
| A3 | Offset page size ~25 is acceptable default | List IA | UX tweak only |
| A4 | Title/body length caps (~1k / ~64k chars) are fine starting points | Security / validation | Need product tweak if too tight/loose |

**If empty of blocking assumptions:** A1–A4 are non-blocking for planning; A2 is the only product-sense checkpoint.

## Open Questions (RESOLVED)

1. **Should `@mention` autolinks ship with remark-github or be disabled via `buildUrl → false`?**
   - What we know: remark-github also transforms mentions/commits.
   - What's unclear: Phase 11 CONTEXT only locks `#N` and `owner/repo#N`.
   - Recommendation: Enable issue refs only; return `false` for mention/commit until a later social phase.
   - RESOLVED: Disable mention/commit autolinks via `buildUrl → false`; ship issue refs only (`#N` / `owner/repo#N`). Locked in plan 11-10 (and Wave 0 stub note in 11-01).

2. **Org label settings UI placement**
   - What we know: Org settings routes exist under `/{owner}/settings`.
   - What's unclear: Exact nav entry copy.
   - Recommendation: “Labels” under org settings + repo Issues → Labels (Admin).
   - RESOLVED: Nav copy “Labels” under org settings plus repo Issues → Labels (Admin). Locked in plan 11-06.

3. **Cross-repo `owner/repo#N` when viewer lacks Read on target**
   - What we know: Autolink is client-side href generation.
   - What's unclear: Whether clicking should soft-404.
   - Recommendation: Link anyway; destination RPC returns soft not-found (no enumeration leak).
   - RESOLVED: Emit the href anyway; destination `issue.get` / list ACL returns soft not-found (no enumeration leak), per D-ISS-20. Locked in plans 11-03 tracer + 11-00 `repo_private_404` stubs / link ACL path.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Node.js | Vitest / web | ✓ | v24.5.0 | — |
| Bun | Workspace install | ✓ | 1.4.0 | npm |
| Cargo / Rust | API + nextest | ✓ | 1.100.0-nightly | — |
| Make | `rpc-gen` / `test` | ✓ | — | — |
| remark-github | Autolink | ✗ (not installed) | 12.0.0 on npm | Install in Wave that lands markdown autolink |
| Docker / dialects | dialect_* tests | assume CI/Compose as prior phases | — | sqlite local |

**Missing dependencies with no fallback:** none for planning  
**Missing dependencies with fallback:** `remark-github` — install when implementing ISS-04 markdown

Step 2.6: external tools beyond repo stack are not new; dialect DBs follow existing Phase 2/10 practices.

## Validation Architecture

> `workflow.nyquist_validation` is **true** in `.planning/config.json` — include this section.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo nextest (Rust) + Vitest 5 (apps/web) |
| Config file | `apps/web/vitest.config.ts`; Cargo nextest via Makefile |
| Quick run command | `cargo nextest run -p oxidean-api -E 'test(issue_)' ; cd apps/web && bun run test:unit` |
| Full suite command | `make test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ISS-01 | create/edit/close/reopen + ACL | API integration | `cargo nextest run -p oxidean-api -E 'test(issue_lifecycle)'` | ❌ Wave 0 |
| ISS-01 | per-repo `#N` monotonic / no reuse | dialect + API | `cargo nextest run -p oxidean-db -E 'test(dialect_issues)'` | ❌ Wave 0 |
| ISS-01 | Admin hard-delete + confirm | API | `cargo nextest run -p oxidean-api -E 'test(issue_delete)'` | ❌ Wave 0 |
| ISS-02 | comment CRUD + moderation delete | API | `cargo nextest run -p oxidean-api -E 'test(issue_comments)'` | ❌ Wave 0 |
| ISS-02 | Write\|Preview uses renderGfm sanitize | unit | `cd apps/web && bunx vitest run src/lib/markdown.test.ts` | ✅ extend |
| ISS-03 | labels assign Write+ / defs Admin | API | `cargo nextest run -p oxidean-api -E 'test(issue_labels)'` | ❌ Wave 0 |
| ISS-03 | assignees Read+ eligibility | API | `cargo nextest run -p oxidean-api -E 'test(issue_assignees)'` | ❌ Wave 0 |
| ISS-04 | markdown `#N` autolink | unit | `bunx vitest run src/lib/markdown.issues.test.ts` | ❌ Wave 0 |
| ISS-04 | link stubs CRUD | API | `cargo nextest run -p oxidean-api -E 'test(issue_links)'` | ❌ Wave 0 |
| ISS-01..04 | private ACL soft not-found | API | extend `repo_private_404` style | ❌ Wave 0 |
| UI | Issues tab + list/detail routes | web integration | `bunx vitest run --project integration issues` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** targeted nextest filter + relevant Vitest file
- **Per wave merge:** `make test` (or nextest workspace + `bun run test` in apps/web)
- **Phase gate:** Full suite green + `make rpc-sync-check` before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `crates/oxidean-api/tests/issue_lifecycle.rs` (and comments/labels/assignees/links/reactions) — RED stubs
- [ ] `crates/oxidean-db/tests/dialect_issues.rs` — migrate `0011_issues` presence on 3 dialects
- [ ] `apps/web` integration stubs for `/$owner/$repo/issues` routes + chrome Issues tab
- [ ] `apps/web/src/lib/markdown.issues.test.ts` — `#N` / `owner/repo#N` + sanitize regression
- [ ] Extend factory reset coverage for issue tables cascade

## Security Domain

> `security_enforcement` enabled (ASVS level 1).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (session) | Existing session cookie RPC; `require_verified` where create is privileged like repo create |
| V3 Session Management | yes | Existing SessionService — no new session types |
| V4 Access Control | **yes** | `Capability` Read/Write/Admin; soft `repo.not_found` / issue not-found |
| V5 Input Validation | **yes** | Length caps; enum checks for state/reaction/label color; RPC `rpc.bad_input` |
| V6 Cryptography | no | No new crypto in Phase 11 |
| V5 XSS (markdown) | **yes** | `rehype-sanitize` last; never trust client HTML |

### Known Threat Patterns for issues stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Private issue/repo enumeration | Information disclosure | Identical soft not-found (D-ISS-20) |
| Stored XSS via issue/comment body | Tampering | Store markdown; sanitize on render |
| Privilege escalation (label def / delete) | Elevation | Admin-only RPCs via `meets(Admin)` |
| Assign unauthorized user | Elevation | Server-side Read+ eligibility |
| Mass assignment / overpost | Tampering | Explicit DTOs; ignore unknown fields |
| Destructive delete without confirm | Elevation / misuse | Typed `confirmNumber` (discretion) |
| Cross-repo link probing | Information disclosure | Destination ACL on get; stubs carry opaque ids |

## Sources

### Primary (HIGH confidence)
- In-repo ACL / RPC / markdown / chrome / RepoPublic — Read this session with line cites above
- `.planning/phases/11-issues/11-CONTEXT.md` — locked decisions
- `.planning/REQUIREMENTS.md` ISS-01..04; `.planning/ROADMAP.md` Phase 11
- `npm view` + `package-legitimacy check` for remark-* / rehype-* / remark-github
- [docs.github.com/en/rest/reactions/reactions](https://docs.github.com/en/rest/reactions/reactions) — reaction content enum

### Secondary (MEDIUM confidence)
- [github.com/remarkjs/remark-gfm](https://github.com/remarkjs/remark-gfm) — points to remark-github for issue refs
- [github.com/remarkjs/remark-github](https://github.com/remarkjs/remark-github) — buildUrl / issue ref behavior (npm readme)
- Gitea `issue_index` / ResourceIndex — [PR #15599](https://github.com/go-gitea/gitea/pull/15599), [models/db/index.go](https://github.com/go-gitea/gitea/blob/v1.27.0/models/db/index.go)
- [docs.gitea.com/usage/issues-prs/labels/](https://docs.gitea.com/usage/issues-prs/labels/) — org+repo labels (used as contrast for override design)

### Tertiary (LOW confidence)
- Exact remark-github `buildUrl` field names in example skeleton (confirm at install) — Assumptions A1

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** — reuse in-repo stack; remark-github legitimacy OK
- Architecture: **HIGH** — mirrors Phases 7–10 seams; counter-table + ACL clear
- Pitfalls: **HIGH** — enumeration, XSS, number reuse, eligibility validated against prior phases
- Label override product rules: **MEDIUM** — discretion recommendation needs planner acceptance

**Research date:** 2026-09-14  
**Valid until:** ~2026-10-14 (stable forge patterns; re-check remark-github major if installing later)
