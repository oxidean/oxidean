# Phase 13: Branch Protection - Research

**Researched:** 2026-09-16  
**Domain:** Classic GitHub-style branch protection; git pre-receive/update hooks; commit statuses; PR merge gates  
**Confidence:** HIGH (GitHub parity surface + in-repo git/ACL seams); MEDIUM (exact hook packaging — discretion)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
#### A — Rule model
- **D-01:** Classic branch protection rules (not Rulesets)
- **D-02:** Branch patterns with `*`/`?`; multi-rule **union** (max review count; union contexts; OR flags)
- **D-03:** Admin-only CRUD
- **D-04:** Soft-protect default branch coexists

#### B — Reviews
- **D-05:** required_pull_request_reviews with count 1..6 (default 1)
- **D-06:** dismiss_stale_reviews (REST default false)
- **D-07:** require_conversation_resolution
- **D-08:** require_last_push_approval
- **D-09:** CODEOWNERS out of scope

#### C — Status checks
- **D-10:** required_status_checks contexts + strict
- **D-11:** Classic commit-status store + RPC in Phase 13
- **D-12:** pending/success/failure/error; success (+ neutral/skipped aliases) pass
- **D-13:** Write+ create status; Read+ list

#### D — Push enforcement
- **D-14:** Required reviews ⇒ block direct pushes
- **D-15:** allow_force_pushes / allow_deletions default false
- **D-16:** enforce_admins default false (Admin bypass when false)
- **D-17:** required_linear_history
- **D-18:** lock_branch
- **D-19:** Bare-repo pre-receive/update hooks for HTTPS+SSH; install on create + reconcile
- **D-20:** branchDelete/rename honor allow_deletions + rules
- **D-21:** Push restrictions deferred

#### E — Merge enforcement
- **D-22:** Shared evaluator before PR merge; structured reasons
- **D-23:** Draft PRs cannot merge under protection
- **D-24:** Surface unmet requirements to UI/RPC

#### F — UI / RPC
- **D-25:** Settings Branches / Branch protection Admin section
- **D-26:** `repo.branchProtection.*` + `repo.commitStatus.*`

### Claude's Discretion
- Migration number; hook packaging; exact error code string; settings sub-route vs panel; empty contexts semantics; dismiss-on-push vs merge-time-only

### Deferred Ideas (OUT OF SCOPE)
- CODEOWNERS, Rulesets, merge queues, signed commits, push allowlists/Teams, Checks API UI, bypass allowlists beyond Admin, protection notifications
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ORG-05 | Repo admin configures branch protection (reviews and/or status checks) | Rule CRUD RPC + Settings UI; status contexts + review count fields |
| ORG-06 | Rules enforced on direct pushes and PR merges | pre-receive/update hooks + PR merge evaluator |
| PR-08 | PR merge blocked when rules unsatisfied | Merge gate calling shared evaluator; structured reasons |
</phase_requirements>

## Summary

Phase 13 adds GitHub-classic **branch protection** to Oxidean. Admins define pattern-matched rules requiring PR reviews and/or named commit-status contexts. Enforcement must work on **direct pushes** (Smart HTTP `git-receive-pack` and SSH receive-pack both run git hooks in the bare repo) and on **PR merges** (application-level gate before Phase 12 merge strategies).

Because Phase 19 Actions is later, ORG-05’s “status checks” cannot wait on workflow runs. The standard forge approach (GitHub classic statuses, Gitea/Forgejo status tables) is a **commit_statuses** table keyed by `(repository_id, sha, context)` with latest state wins, plus RPC for CI to report. Required-check evaluation reads that store (and later Check Runs map into the same pass set).

**Primary recommendation:** Central `protection::evaluate(repo, ref, actor, intent)` used by (1) bare-repo hooks for push/delete/force-push and (2) PR merge RPC. Install hooks at `init_bare` and reconcile. No new crates.io packages — use existing git CLI + fnmatch-style matching in Rust (`wildmatch` already common, or hand-roll `*`/`?` to avoid deps if not present).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Rule + status persistence | Database / Storage | API / Backend | Tri-dialect migrations; API owns RPC |
| Protection evaluation | API / Backend | — | Single Rust module; hooks/merge call it |
| Git push enforcement | API / Backend | CDN/Static (ops) | Hooks inside bare repos under OXIDEAN_REPOS_DIR |
| Commit status ingest | API / Backend | — | RPC/PAT Write; no SPA auth for CI bots preferred |
| Settings CRUD UI | Browser / Client | API / Backend | Admin Octane panel |
| PR merge blockers UI | Browser / Client | API / Backend | Consumes structured reasons from merge/preview RPC |
| Soft-protect coexistence | API / Backend | — | Existing branch mutate path |

## Project Constraints (from `.cursor/rules/`)

- One Bun + Cargo product; Octane `.tsrx` for UI [VERIFIED]
- RPC: Rust → `make rpc-gen` [VERIFIED]
- Dialect SQL only in `oxidean-db` [VERIFIED]
- Prefer extending Smart HTTP / SSH / ACL patterns [VERIFIED: AGENTS.md]

## Standard Stack

### Core

| Library / component | Version | Purpose | Why Standard |
|---------------------|---------|---------|--------------|
| Existing Axum RPC | workspace | `repo.branchProtection.*`, `repo.commitStatus.*` | Matches forge RPC surface [VERIFIED] |
| git hooks (`pre-receive`/`update`) | system git | Enforce on receive-pack | git-http-backend + SSH both honor bare hooks [VERIFIED: http_backend CGI; ssh pack] |
| `oxidean-db` migrations | next free | Tables for rules + statuses | Tri-dialect pattern [VERIFIED: 0015_packages.sql present] |
| Octane + TanStack Query | workspace | Settings + PR blocker UI | Existing settings panels [VERIFIED: `$owner.$repo.settings.tsrx`] |

### Supporting

| Approach | Purpose | When |
|----------|---------|------|
| Shared `protection` Rust module | One eval for hook + merge | Always |
| Hook helper binary or script | Bridge stdin refs → evaluator | Install at init_bare |
| fnmatch / wildmatch | Pattern match branch names | Rule matching |

### Package Legitimacy Audit

| Package | Status | Notes |
|---------|--------|-------|
| (none new) | N/A | Prefer hand-rolled `*`/`?` matcher or existing workspace crate; **no** new crates.io/npm deps for this phase |

If a matcher crate is proposed later, re-run legitimacy gate before install.

## Architecture patterns

### Pattern 1 — Shared evaluator

```text
evaluate(ProtectionContext) -> Result<(), ProtectionDenial>
  intent: Push | ForcePush | Delete | Merge
  inputs: matching rules (union), actor capability, head/base SHAs,
          approvals, unresolved threads, status map, admin bypass
```

Call sites: hook (Push/ForcePush/Delete), `pull.merge` (Merge), optionally `pull.mergePreview`.

### Pattern 2 — Hook install

On `init_bare` success, write `hooks/update` (and/or `pre-receive`) executable that:
1. Reads oldrev/newrev/ref
2. Invokes oxidean protection check with repo identity from `GIT_DIR` / env
3. Exits non-zero with stderr message on deny

Reconcile job or lazy-on-push repair for older bare repos missing hooks.

### Pattern 3 — Multi-rule union (D-02)

For matching rules R1..Rn:
- `required_approving_review_count = max(count_i)` among rules with reviews enabled
- `required_contexts = union(contexts_i)`
- boolean flags: true if any matching rule sets them
- allow_force_pushes / allow_deletions: false if any matching rule disallows (most restrictive)

### Pattern 4 — Status latest-wins

`UNIQUE(repository_id, sha, context)`; create upserts state + description + target_url + creator; list by sha for PR UI.

## Don't hand-roll

| Problem | Use instead |
|---------|-------------|
| Custom receive-pack parser to reject pushes | Standard git hooks on bare repo |
| Separate HTTPS vs SSH protection logic | Same hooks / same evaluator |
| Hand-edited api-client types | `make rpc-gen` |
| Parallel soft-protect replacement | Keep D-28 floor; layer rules |

## Common pitfalls

| Pitfall | Avoidance |
|---------|-----------|
| Evaluating only merge, forgetting direct push | ORG-06 requires both; tracer must prove push deny |
| Admin silently always bypasses | Honor `enforce_admins` (D-16) |
| Status checks with empty contexts blocking forever | Discretion: only enforce named contexts when non-empty |
| Hook not installed on existing repos | Reconcile path (D-19) |
| Dismiss stale only at merge time | Prefer dismiss on head push for GitHub-like PR UI |
| Depending on Phase 19 for ORG-05 checks | Ship commit-status RPC now (D-11) |
| Soft-protect removed | Keep `repo.default_branch_protected` |

## Codebase seams (verified)

| Seam | Path | Notes |
|------|------|-------|
| Soft protect | `crates/oxidean-api/src/repo/mod.rs` | `soft_protect_err` |
| ACL | `crates/oxidean-api/src/repo/acl.rs` | Admin/Write |
| HTTPS receive-pack | `crates/oxidean-api/src/routes/git_smart_http.rs` | CGI; hooks run inside git |
| SSH receive-pack | `crates/oxidean-api/src/ssh/pack.rs` | Same bare hooks |
| Bare init | `crates/oxidean-git/src/cli.rs` `init_bare` | Hook install |
| Settings UI | `apps/web/src/routes/$owner.$repo.settings.tsrx` | Panel pattern |
| Migrations | `crates/oxidean-db/migrations/*/0015_packages.sql` | Next = 0016+ at execute |

## Phase 12 dependency

Phase 12 CONTEXT locks D-PR-10 / D-PR-21: no protection enforcement until Phase 13. Executor **precondition**: Phase 12 merge + review + conversation-resolution APIs exist on the integration branch before green merge-gate tests. Wave 0 stubs and schema/status/hook work can proceed against current main; merge-gate tasks halt if Phase 12 symbols are missing.

## Out of scope (research)

- Rulesets engine, CODEOWNERS parser, merge queue, GPG signed commits, Teams restrictions, Actions check-run UI

## Validation notes

Prefer nextest filters `test(branch_protect)` / `test(commit_status)` and Vitest `settings.branches` / `pull.protection` mirrors of Phase 14/15 Wave 0 style. Push denial tests should drive real `git push` against test bare repo with hooks installed (not only unit eval).
