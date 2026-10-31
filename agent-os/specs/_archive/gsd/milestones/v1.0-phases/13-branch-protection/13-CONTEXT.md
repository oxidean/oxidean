# Phase 13: Branch Protection - Context

**Gathered:** 2026-09-16  
**Status:** Ready for planning  
**Discuss mode:** Remaining gray areas locked to **GitHub parity** per campaign defaults (same approach as Phase 12 CONTEXT). Auto-decided without human wait (2026-09-16).

<domain>
## Phase Boundary

Repo admins configure classic branch protection rules (required reviews and/or status checks). Rules are enforced on direct pushes (HTTPS + SSH) and on PR merges. Delivers ORG-05, ORG-06, PR-08.

**Requirements:** ORG-05, ORG-06, PR-08

**Success criteria (from ROADMAP):**
1. Repo admin can configure branch protection rules (e.g. require reviews and/or status checks)
2. Protected rules block non-compliant direct pushes
3. PR merge is blocked when applicable branch protection rules are not satisfied

**Out of scope:**
- CODEOWNERS-enforced reviews (COLLAB-V2-03)
- GitHub Repository Rulesets (v2) — classic branch protection only
- Merge queues / auto-merge (COLLAB-V2-04)
- Signed-commit requirement (needs GPG/SSH signing product surface)
- Push allowlists by users/teams/apps (`restrictions`) — no Teams yet
- Full Checks API / Actions check-runs UI (Phase 19 consumes status store)
- Notifications for protection failures (Phase 17)

**UI hint:** yes — Repo Settings → Branches / Branch protection; PR detail merge blockers when unsatisfied.

**Depends on:** Phase 12 Pull Requests (merge + reviews + conversation resolution primitives). Plan now; execute after Phase 12 lands.

</domain>

<decisions>
## Implementation Decisions

### A — Rule model (GitHub classic protection)
- **D-01:** Ship **classic branch protection rules** (pattern + settings), **not** Repository Rulesets — **Reversibility:** costly — schema + API shape
- **D-02:** Each rule has a **branch name pattern** with GitHub-style `*` / `?` wildcards (fnmatch); multiple rules allowed; when several match a ref, apply the **union** of restrictions (OR of blocks; **max** of `required_approving_review_count`; **union** of required status contexts; any matching rule with a flag set wins for that flag) — **Reversibility:** costly — evaluation semantics
- **D-03:** Only actors with repo **Admin** capability may create/update/delete rules (ORG-05) — **Reversibility:** reversible
- **D-04:** Phase 7 **soft-protect** of default-branch rename/delete (`repo.default_branch_protected`) remains as a **floor** and coexists with protection rules — **Reversibility:** reversible

### B — Required reviews (GitHub parity; unlocks D-PR-10)
- **D-05:** Optional **`required_pull_request_reviews`**: when enabled, set **`required_approving_review_count`** in **1..6** (default **1**); effective approvals use Phase 12 latest-per-user review state (D-PR-07); author self-approve still invalid (D-PR-06) — **Reversibility:** reversible
- **D-06:** **`dismiss_stale_reviews`** boolean (GitHub REST default **false**); when true, new head pushes dismiss prior Approvals to pending — **Reversibility:** reversible
- **D-07:** **`require_conversation_resolution`** — all PR review threads must be Resolved before merge (GitHub; Phase 12 D-PR-14 threads) — **Reversibility:** reversible
- **D-08:** **`require_last_push_approval`** — after the latest head push, an approving review must come from someone **other than** the pusher (GitHub) — **Reversibility:** reversible
- **D-09:** **`require_code_owner_reviews`** is **out of scope** (COLLAB-V2-03) — store may omit the field or ignore it if present — **Reversibility:** reversible (deferral)

### C — Status checks (GitHub parity without waiting for Actions)
- **D-10:** Optional **`required_status_checks`**: list of **context** names + **`strict`** (head must be up to date with base before merge) — **Reversibility:** costly — eval + git ancestry check
- **D-11:** Phase 13 ships a **classic commit-status store** + RPC (`create` / `list` by SHA) so external CI and later Actions can report contexts; Check Runs (Phase 19) map into the same pass/fail evaluation — **Reversibility:** costly — statuses schema
- **D-12:** Status states: **`pending` | `success` | `failure` | `error`**; evaluation treats **`success`** as pass (and **`neutral`/`skipped`** if/when Checks land — treat as pass aliases in evaluator). Missing or `pending`/`failure`/`error` → unsatisfied — **Reversibility:** reversible
- **D-13:** Creating a status requires **Write+** on the repo (session or PAT with contents/repo write); listing requires **Read+** — **Reversibility:** reversible

### D — Direct-push enforcement (ORG-06)
- **D-14:** When **`required_pull_request_reviews`** is enabled on a matching rule, **block direct pushes** to matching branch refs (non-bypass actors must use a PR) — **Reversibility:** reversible
- **D-15:** **`allow_force_pushes`** default **false**; **`allow_deletions`** default **false** (GitHub defaults) — **Reversibility:** reversible
- **D-16:** **`enforce_admins`** / “do not allow bypassing” default **false** (GitHub): when false, **Admin** capability may bypass push and merge restrictions; when true, **everyone** including Admin must satisfy rules — **Reversibility:** reversible
- **D-17:** **`required_linear_history`** boolean — when true, reject merge commits on the protected ref / block merge-commit merge method when that would create a merge commit on the base — **Reversibility:** reversible
- **D-18:** **`lock_branch`** boolean — when true, matching branch is read-only (no pushes, no merges) except bypass per D-16 — **Reversibility:** reversible
- **D-19:** Enforce via **bare-repo `pre-receive`/`update` hooks** shared by Smart HTTP and SSH; hooks call a shared Rust evaluator (same code as merge gate). Install hooks on **repo create** and **reconcile** existing bare repos that lack them — **Reversibility:** costly — hook install path
- **D-20:** `repo.branchDelete` / rename paths honor **`allow_deletions`** and protection matches in addition to soft-protect (D-04) — **Reversibility:** reversible
- **D-21:** User/team/app **push restrictions** (`restrictions`) deferred — no Teams — **Reversibility:** reversible (deferral)

### E — PR merge enforcement (PR-08)
- **D-22:** PR merge RPC must call the **same evaluator** against the **base branch** before performing any merge strategy (D-PR-17); on failure return a stable error (e.g. `pull.merge_blocked` / `repo.branch_protection`) with **structured unsatisfied reasons** — **Reversibility:** reversible
- **D-23:** Draft PRs cannot merge while protection (or draft rules) apply — follow Phase 12 draft semantics if present; otherwise open PRs only — **Reversibility:** reversible
- **D-24:** Merge UI/RPC surfaces which requirements are unmet (reviews, checks, conversations, up-to-date, locked) for Admin/Write callers who can see the PR — **Reversibility:** reversible

### F — Settings & IA (GitHub parity + existing shell)
- **D-25:** Repo Settings gains a **Branches / Branch protection** Admin section (Octane `.tsrx`); list rules, add/edit/delete; do not remount `RepoChrome` (Phase 11.1) — **Reversibility:** reversible
- **D-26:** RPC namespace preference: **`repo.branchProtection.*`** and **`repo.commitStatus.*`** (planner may refine exact procedure names; regenerate client via `make rpc-gen`) — **Reversibility:** reversible

### Claude's Discretion
- Exact migration number (next free after packages at execute time)
- Exact hook binary/script packaging (embedded oxidean-git-hook vs thin shell → RPC)
- Exact error-code string between `pull.merge_blocked` vs `repo.branch_protection` (pick one stable code + reasons array)
- Whether Settings is a sub-route (`…/settings/branches`) vs panel on main settings page
- Default empty required-contexts list meaning “require checks enabled but none named yet” vs treating as no check requirement until contexts added (prefer: checks block only when contexts non-empty)
- Whether dismiss-stale is implemented as auto-dismiss on push event vs recompute at merge time only (prefer persist dismiss on push for GitHub-like UX)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 13 goal, ORG-05, ORG-06, PR-08; Phase 19 depends on 13
- `.planning/REQUIREMENTS.md` — ORG-05, ORG-06, PR-08 wording
- `.planning/phases/12-pull-requests/12-CONTEXT.md` — D-PR-10 / D-PR-21 defer protection to Phase 13; review model D-PR-05…09; merge strategies D-PR-17…20
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Admin/Write Capability ACL (D-ORG-05)
- `.planning/phases/07-git-repos-browse/07-CONTEXT.md` — soft-protect D-28; branch CRUD
- `.planning/phases/08-git-https-pats/08-CONTEXT.md` — Smart HTTP receive-pack + PAT
- `.planning/phases/09-git-ssh/09-CONTEXT.md` — SSH receive-pack ACL

### Code mirrors
- `crates/oxidean-api/src/repo/mod.rs` — `soft_protect_err` / branch mutate
- `crates/oxidean-api/src/repo/acl.rs` — Capability gates
- `crates/oxidean-api/src/routes/git_smart_http.rs` — receive-pack CGI path
- `crates/oxidean-api/src/ssh/pack.rs` — SSH receive-pack
- `crates/oxidean-git/src/cli.rs` — `init_bare` hook-install seam
- `apps/web/src/routes/$owner.$repo.settings.tsrx` — Admin settings panels pattern
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` mandatory for UI

### External (parity)
- GitHub docs: About protected branches / Branch protection REST API (classic)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Soft-protect default branch rename/delete with `repo.default_branch_protected`
- `effective_capability` / `meets(Admin|Write|Read)` for settings and git ACL
- Smart HTTP + SSH receive-pack already authz’d; need protection after authz
- Repo Settings panels (collaborators, LFS, rename/transfer) as UI pattern
- Phase 12 (when landed): PR reviews, threads resolved, merge RPC — merge gate hooks here

### Established Patterns
- Tri-dialect SQL only in `oxidean-db`
- RPC types in Rust → `make rpc-gen` (never hand-edit api-client SoT)
- Private deny soft `repo.not_found` on web; git 401/403 mapping unchanged
- Wave 0 RED stubs + nextest/Vitest filters before implementation (Nyquist)

### Integration Points
- Bare repo create (`init_bare`) → install protection hooks
- receive-pack (HTTPS CGI + SSH) → hook evaluator
- PR merge RPC (Phase 12) → same evaluator before git merge
- Commit status RPC → DB rows consumed by required_status_checks
- Phase 19 Actions will write statuses/check conclusions into this store

### Gaps
- No branch_protection / commit_statuses tables yet
- No pre-receive enforcement beyond ACL
- No PR merge gate for reviews/checks (Phase 12 explicitly deferred)
- Phase 12 not executed on this branch yet — plans assume its APIs as **preconditions**

</code_context>

<specifics>
## Specific Ideas

- Match Phase 12 discuss style: lock gray areas to **GitHub classic branch protection** behavior
- Status checks must be configurable in ORG-05 **before** Phase 19 Actions — hence classic commit-status API in this phase
- Keep soft-protect; do not replace it with rules-only delete protection

</specifics>

<deferred>
## Deferred Ideas

- CODEOWNERS required reviews (COLLAB-V2-03)
- Repository Rulesets (GitHub v2)
- Merge queues / auto-merge
- Signed commits required
- Push actor allowlists / Teams restrictions
- Full Checks API + Actions check-run UI (Phase 19)
- Bypass actor allowlists beyond Admin capability when `enforce_admins=false`
- Notifications for blocked pushes/merges (Phase 17)

</deferred>

---

*Phase: 13-branch-protection*  
*Context gathered: 2026-09-16*
