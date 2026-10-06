# Phase 12: Pull Requests - Research

**Researched:** 2026-09-16  
**Domain:** Pull requests (open/review/comment/merge) on Oxidean Rust RPC + Octane UI + multi-dialect DB + GitBackend CLI  
**Confidence:** HIGH (codebase seams) / MEDIUM (fork-minimal vs Phase 21 SOC-04 boundary; bare-repo merge worktree details)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-PR-01:** Heads may be same-repo branches or forks of this repo — costly
- **D-PR-02:** PR numbers share per-repo `#N` sequence with issues — one-way
- **D-PR-03:** Head = fork of this repo or same-repo branch; base always on this repo — costly
- **D-PR-04:** After open: force-push head + change base; mark outdated line comments — costly
- **D-PR-05:** Write+ may submit Approve / Request changes / Comment reviews
- **D-PR-06:** Author cannot Approve own PR; Comment only
- **D-PR-07:** Latest review per user is effective
- **D-PR-08:** Write+ may dismiss a review
- **D-PR-09:** Optional requested reviewers (UX only; no merge block)
- **D-PR-10:** No N-approvals required to merge in Phase 12 (→ Phase 13)
- **D-PR-11:** Tabs Conversation | Commits | Files changed
- **D-PR-12:** Unified and split diff views
- **D-PR-13:** Line comments (single + multi-line) + general comments
- **D-PR-14:** Threads Resolved / Unresolved
- **D-PR-15:** Outdated line comments on force-push/retarget
- **D-PR-16:** Markdown Write|Preview + renderGfm sanitize
- **D-PR-17:** Merge / squash / rebase
- **D-PR-18:** Repo settings enable/disable each method; defaults all on
- **D-PR-19:** Merge requires Write+ on base; close/reopen Write+
- **D-PR-20:** Optional delete head branch after merge
- **D-PR-21:** No branch-protection merge blocks (→ Phase 13)
- **D-PR-22:** Closing keywords on merge to default branch (body + commits)
- **D-PR-23:** Keywords do not fire on close-without-merge or non-default base
- **D-PR-24:** Replace `pr_stub` with real PR links; keep manual link control
- **D-PR-25:** Pulls tab in layout-owned RepoChrome (`active: "pulls"`)
- **D-PR-26:** List Open default; Closed/All; filters author/label/assignee/review; sort updated; offset pagination
- **D-PR-27:** New PR + Compare `base...head` create flow
- **D-PR-28:** Compare/create for same-repo and fork heads
- **D-PR-29:** Capability Read/Write/Admin; private `repo.not_found`

### Claude's Discretion
- URLs: prefer `/pulls` + `/pull/{n}` (GitHub)
- Draft PR boolean if cheap
- Review-state filter values
- RPC namespace `pull.*` (lock) vs `pr.*`
- Migration `0016_pull_requests` after `0015_packages`
- Dismiss reason optional string
- Squash message default = PR title
- Minimal fork parent column in Phase 12 (full fork UX = SOC-04 Phase 21)

### Deferred (OUT OF SCOPE)
- PR-08 / branch protection (Phase 13)
- CODEOWNERS (COLLAB-V2-03)
- Notifications (Phase 17)
- Merge queue / auto-merge
- Cross-instance PRs
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PR-01 | Open PR from branch (same repo or fork) | Shared `#N` counter; head/base refs; minimal `forked_from_repo_id` |
| PR-02 | View diff, commits, conversation | Reuse `GitBackend::diff` / `log`; conversation = comments+reviews |
| PR-03 | General + line comments | Anchor schema path/side/line/range/commit; resolve threads; outdated |
| PR-04 | Request changes / approve | `pull_reviews` latest-per-user; author cannot Approve |
| PR-05 | Merge merge/squash/rebase | New GitBackend merge ops + Write+ ACL |
| PR-06 | Close / reopen | State machine open↔closed; merged terminal for reopen→open only if not merged |
| PR-07 | Enable/disable merge strategies | Columns on `repositories`; Admin settings UI |
</phase_requirements>

## Summary

Phase 12 adds first-class pull requests atop Issues numbering, RepoChrome IA (11.1), Capability ACL (10), and `GitBackend` CLI (7). There is **no** PR domain today — only `issue_links.kind='pr_stub'`, `repo.compare` unified diff, and compare route `$owner.$repo.compare.$.tsrx`. Next migration is **`0016_pull_requests`**.

**Primary recommendation:** Ship `pull.*` RPC module + `0016_pull_requests` (PRs, comments, reviews, review_requests, labels/assignees junctions, merge-strategy + `forked_from_repo_id` columns). Reuse `allocate_next_issue_number` for shared `#N`. Extend `GitBackend` with merge/squash/rebase (+ optional fetch-from-fork). Tracer: same-repo open → list/get → Pulls tab before reviews/merge/forks.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary | Rationale |
|------------|--------------|-----------|-----------|
| PR CRUD / close / reopen / retarget | API | DB | Numbering + ACL authoritative |
| Diff / commits | API + GitBackend | Browser | Diff from bare repo; UI renders |
| Line/general comments + resolve | API | Browser | Anchors + outdated server-side |
| Reviews / dismiss / requests | API | Browser | Latest-per-user + ACL |
| Merge strategies | GitBackend + API | DB settings | Three git paths + Admin toggles |
| Closing keywords | API | Issues DB | Side effect on merge to default |
| Pulls chrome / routes | Browser | SSR | D-QH-01 layout chrome |
| Fork head validation | API + DB | Git | Minimal parent FK; SOC-04 UX later |

## Codebase Facts (verified)

| Fact | Evidence |
|------|----------|
| Latest migration `0015_packages` | `crates/oxidean-db/migrations/sqlite/` |
| Shared counter `issue_counters` + `allocate_next_issue_number` | `crates/oxidean-db/src/issues.rs` |
| `pr_stub` in links CHECK | `0011_issues.sql` issue_links |
| No fork columns on repositories | `0010_orgs_acl` repositories_new |
| `GitBackend` has diff/log/branch_*; **no merge** | `crates/oxidean-git/src/backend.rs` |
| `repo.compare` same-repo only | `repo/mod.rs` compare |
| Compare UI exists | `apps/web/src/routes/$owner.$repo.compare.$.tsrx` |
| RepoChromeActive lacks `pulls` | `apps/web/src/lib/repo-chrome-active.ts` |
| Issue RPC pattern | `crates/oxidean-api/src/issue/mod.rs` |
| DiffPatch component | `apps/web/src/components/repo/diff-patch.tsrx` |

## Schema sketch (`0016_pull_requests`)

1. `repositories`: `allow_merge_commit`, `allow_squash_merge`, `allow_rebase_merge` (default 1/true); `forked_from_repo_id` NULL FK → repositories ON DELETE SET NULL  
2. `pull_requests`: id, repo_id (base), number (from issue_counters), title, body, state (`open`/`closed`/`merged`), draft bool, author_id, base_ref/base_sha, head_repo_id/head_ref/head_sha, merged_*, closed_*, timestamps; UNIQUE(repo_id, number)  
3. `pull_comments`: general + line (path, side `LEFT`/`RIGHT`, line, start_line nullable, commit_sha, outdated, thread_id, resolved)  
4. `pull_reviews`: pr_id, author_id, state (`approved`/`changes_requested`/`commented`/`dismissed`), body, submitted_at; effective = latest non-dismissed per user  
5. `pull_review_requests`: pr_id, user_id (UX only)  
6. `pull_labels` / `pull_assignees`: mirror issues for list filters  
7. `issue_links.kind`: expand CHECK to `'issue'|'pr_stub'|'pr'`; app prefers `pr` when target PR exists  

## GitBackend extensions

Bare-repo merges via temp worktree (same pattern as `seed_commit`):

- `merge_commit(base_repo, base_ref, head_sha, message) -> merge_sha`
- `squash_merge(...) -> squash_sha`
- `rebase_merge(...) -> rebase_tip_sha` (or equivalent replay)
- `fetch_objects(dest, source_repo_path, ref)` for fork heads
- Soft-fail conflicts → API error `pull.merge_conflict`

## RPC namespace (discretion lock)

`pull.create|get|list|update|close|reopen|merge|compare|commits|files`  
`pull.comment.*` / `pull.review.*` / `pull.review_request.*`  
`repo.settings.merge` (or fields on existing repo update) — Admin

## UI routes

- `/{owner}/{repo}/pulls` list  
- `/{owner}/{repo}/pulls/new` + compare deep-link  
- `/{owner}/{repo}/pull/{n}` detail tabs  
- RepoChrome `active: "pulls"`; settings merge strategy toggles  

## Validation architecture

Mirror Phase 11 Wave 0: nextest `pull_*`, `dialect_pulls`, `factory_reset_pulls`; Vitest `$owner.$repo.pulls.integration.test.ts`; green gates per plan.

## Risks

| Risk | Mitigation |
|------|------------|
| Fork UX vs SOC-04 | Minimal parent FK + test fork helper; no Explore UI |
| Number collision issue/PR | Only allocate via `issue_counters` |
| Merge on bare repos | Temp worktree; conflict tests |
| Split+unified diff cost | Reuse DiffPatch; split as second layout |
| Outdated comments | Recompute anchors on head SHA change |

## Don't reinvent

- Do not new UI framework / Zustand  
- Do not hand-edit api-client — `make rpc-gen`  
- Do not remount RepoChrome in leaves  
- Do not enforce protection / required reviews  
- Dialect SQL only in `oxidean-db`
