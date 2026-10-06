# Phase 12: Pull Requests - Context

**Gathered:** 2026-09-15  
**Status:** Ready for planning  
**Discuss mode:** Remaining gray areas locked to **GitHub parity** per user (2026-09-15). Area 1 (PR identity & forks) locked earlier.

<domain>
## Phase Boundary

Users can open, review, comment, and merge pull requests with configurable merge strategies. Delivers PR-01 … PR-07.

**Requirements:** PR-01, PR-02, PR-03, PR-04, PR-05, PR-06, PR-07  
**Explicitly Phase 13:** PR-08 (merge blocked by branch protection)

**Success criteria (from ROADMAP):**
1. User can open a PR from a branch (same repo or fork) and view diff, commits, and conversation
2. User can leave general and line comments and request changes or approve
3. User with permission can merge choosing merge commit, squash, or rebase; can close or reopen a PR
4. Repo settings can enable/disable each merge strategy independently

**Out of scope:**
- Branch protection rules / required checks / required review count (Phase 13 — PR-08)
- CODEOWNERS-enforced reviews (COLLAB-V2-03)
- Notifications for PR activity (Phase 17)
- Merge queues, auto-merge, draft→ready as a separate product surface beyond a simple draft flag if needed for GitHub-ish UX (prefer simple **draft PR** boolean if cheap; otherwise open-only in v1)
- Cross-instance / external PR links

**UI hint:** yes — **Pulls** tab in repo chrome (layout-owned `RepoChrome` from Phase 11.1); routes under `/{owner}/{repo}/pulls` (or `/pull` — planner picks GitHub-like `/pulls` list + `/pull/{n}` detail).

**Depends on:** Phase 11.1 Quality Hardening for chrome/IA before PR UI plans land on the same shell.

</domain>

<decisions>
## Implementation Decisions

### A — PR identity & forks (locked 2026-09-14)
- **D-PR-01:** Heads may be **same-repo branches or forks** of this repo (GitHub/Gitea parity) — **Reversibility:** costly — fork graph + cross-repo refs
- **D-PR-02:** PR numbers share the **per-repo `#N` sequence with issues** (GitHub-style) — **Reversibility:** one-way — shared counter; Phase 11 already uses this sequence
- **D-PR-03:** Head must be a **fork of this repo or a same-repo branch**; **base** is always on this repo — **Reversibility:** costly — validation rules
- **D-PR-04:** After open: allow **force-push to head** and **change base**; mark **outdated** line comments (GitHub-like) — **Reversibility:** costly — comment outdated state + retarget

### B — Review model (GitHub parity)
- **D-PR-05:** Anyone with **Write+** on the repo may submit a review (**Approve** / **Request changes** / **Comment**) — classic GitHub; not limited to requested reviewers — **Reversibility:** reversible
- **D-PR-06:** **PR author cannot Approve their own PR**; author may leave **Comment** reviews only (GitHub) — **Reversibility:** reversible
- **D-PR-07:** **Latest review per user** is the effective state (new submission supersedes prior) — **Reversibility:** reversible
- **D-PR-08:** **Write+** may **dismiss** a review (GitHub) — **Reversibility:** reversible
- **D-PR-09:** **Optional requested reviewers** (Write+ can request); requests are UX only in Phase 12 — **do not** block merge until Phase 13 protection — **Reversibility:** reversible
- **D-PR-10:** Phase 12 does **not** require N approvals to merge; unprotected repos merge with Write+ on base (GitHub without protection). Enforcement waits for Phase 13 — **Reversibility:** reversible (deferral)

### C — Line comments & diff UX (GitHub parity)
- **D-PR-11:** PR detail tabs: **Conversation | Commits | Files changed** (GitHub) — **Reversibility:** reversible
- **D-PR-12:** Support **unified and split** diff views in Files changed — **Reversibility:** costly — two render paths
- **D-PR-13:** **Line comments** on diff hunks (single-line and multi-line ranges); general PR comments in Conversation — **Reversibility:** costly — anchor schema (path, side, line/range, commit SHA)
- **D-PR-14:** Comment threads can be **Resolved / Unresolved** (GitHub) — **Reversibility:** reversible
- **D-PR-15:** On force-push / retarget: mark line comments **outdated** when anchors no longer apply; keep visible in Conversation with outdated badge — **Reversibility:** reversible (pairs with D-PR-04)
- **D-PR-16:** Markdown authoring: reuse Issues **Write | Preview** + `renderGfm` sanitize (D-ISS-10) — **Reversibility:** reversible

### D — Merge strategies (GitHub parity)
- **D-PR-17:** Merge methods: **create a merge commit**, **squash and merge**, **rebase and merge** (PR-05) — **Reversibility:** costly — three git merge paths
- **D-PR-18:** Repo settings can **enable/disable each method independently** (PR-07); **defaults: all enabled** (GitHub default) — **Reversibility:** reversible
- **D-PR-19:** Merge requires **Write+** on the base repo (and head readable); close/reopen same ACL as Issues Write+ (PR-06) — **Reversibility:** reversible
- **D-PR-20:** Optional **delete head branch after merge** checkbox (default off or follow GitHub default for the actor) — **Reversibility:** reversible
- **D-PR-21:** Do **not** enforce branch-protection merge blocks here (PR-08 → Phase 13) — **Reversibility:** reversible (deferral)

### E — Closing keywords (GitHub parity; unlocks D-ISS-15)
- **D-PR-22:** On merge into the **default branch**, parse **`fixes` / `closes` / `resolves` `#N`** (and `owner/repo#N`) in the **PR body** and in **commits being merged**; close matching open issues (GitHub) — **Reversibility:** costly — keyword parser + issue close side effects
- **D-PR-23:** Closing keywords do **not** fire on close-without-merge or on merge to a **non-default** base (GitHub default-branch rule) — **Reversibility:** reversible
- **D-PR-24:** Replace Phase 11 **`pr_stub`** link rows with **real PR** links when PR domain exists; keep manual link control — **Reversibility:** costly — migrate stub kind → PR

### F — UI chrome & list (GitHub parity + 11.1 shell)
- **D-PR-25:** Add **Pulls** tab to layout-owned `RepoChrome` (`active: "pulls"`); do **not** remount chrome in leaves (Phase 11.1 D-QH-01) — **Reversibility:** reversible
- **D-PR-26:** List defaults to **Open**; **Closed** and **All**; filters: **author, label, assignee, review state** (basic); sort newest-updated; **offset pagination** (align Issues D-ISS-16…18) — **Reversibility:** reversible
- **D-PR-27:** Entry points: **New pull request** from list/empty state; **Compare** flow (`base...head`) to open PR — **Reversibility:** reversible
- **D-PR-28:** Compare / PR create should work for **same-repo and fork** heads per D-PR-01…03 — **Reversibility:** reversible

### ACL (carry forward)
- **D-PR-29:** Reuse Phase 10 `Capability::{Read,Write,Admin}`: **Read** → list/view PR + diff; **Read + verified sign-in** → comment on the conversation; **author or Write+** → edit title/body, close/reopen; **fork head** → open PR needs Read on base + Write on head repo; **Write** → same-repo open, review, resolve threads, merge; **Admin** → merge-strategy settings. Private repos keep **`repo.not_found`** anti-enumeration.

### Claude's Discretion
- Exact URL path (`/pulls` vs `/pull`) — prefer GitHub `/pulls` + `/pull/{n}`
- Whether draft PRs ship in Phase 12 (boolean `draft`) or open-only
- Exact review-state filter values on the list
- RPC naming (`pull.*` vs `pr.*`) and migration number after packages
- Whether dismiss-review requires a reason string
- Squash commit message defaults (PR title vs commit list)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning / requirements
- `.planning/ROADMAP.md` — Phase 12 goal, success criteria, depends on 11.1
- `.planning/REQUIREMENTS.md` — PR-01…PR-07 (PR-08 → Phase 13)
- `.planning/phases/12-pull-requests/12-DISCUSS-CHECKPOINT.json` — Area 1 lock history
- `.planning/phases/11-issues/11-CONTEXT.md` — shared `#N`, stub links, closing-keyword deferral (D-ISS-13…15)
- `.planning/phases/10-orgs-permissions/10-CONTEXT.md` — Capability ACL
- `.planning/phases/11.1-quality-hardening/11.1-CONTEXT.md` — RepoChrome layout ownership (D-QH-01)

### Product docs
- `docs/API.md` — issue links / `pr_stub` notes until replaced
- `docs/ARCHITECTURE.md` — git / ACL surfaces
- `.agents/skills/octane/SKILL.md` — Octane `.tsrx` UI (mandatory for UI plans)

### Prior verify honesty
- `.planning/phases/11-issues/11-VERIFICATION.md` — Known stubs (`pr_stub`, closing keywords)

</canonical_refs>

<deferred>
## Deferred Ideas

- CODEOWNERS required reviews (COLLAB-V2-03)
- Required approval count / conversation resolution required (Phase 13 protection)
- Merge queue / auto-merge
- Sticky review requests across pushes beyond GitHub basics
</deferred>
