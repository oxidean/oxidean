---
phase: 11-issues
plan: "08"
subsystem: api
tags: [reactions, issues, comments, octane, rpc, github-eight]

requires:
  - phase: 11-issues
    provides: "0011_issues reaction tables + IssuePublic/comments RPC + detail UI shell"
provides:
  - "issue.reactions.toggle for issue|comment with GitHub eight contents"
  - "Reaction summaries on IssuePublic / IssueCommentPublic"
  - "IssueReactionsBar on issue detail + comments (Write+ toggle)"
affects: [11-09, 11-12, web-issues-ui]

actuals:
  tokens: 13951
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "Toggle-on/off reaction rows keyed by (target, user, content)"
    - "Aggregated ReactionGroupPublic embedded on issue/comment payloads"
    - "Octane role=toolbar Reactions bar with Write+ buttons"

key-files:
  created:
    - apps/web/src/components/repo/issue-reactions.tsrx
  modified:
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/issue_reactions.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/components/repo/issue-comments.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Full GitHub eight content enum (+1,-1,laugh,confused,heart,hooray,rocket,eyes) — no subset"
  - "Toggle returns updated groups + reacted bool; issue.get / comments.list carry reactions"
  - "Detail route file is $owner.$repo.issues.$n.tsrx (plan named $number)"

patterns-established:
  - "ReactionChip child + IssueReactionsBar toolbar for issue and comment targets"
  - "Query invalidate issue.get and comments.list after toggle"

requirements-completed: [ISS-01, ISS-02]

coverage:
  - id: D1
    description: "Write+ can toggle GitHub eight reactions on issues and comments"
    requirement: ISS-02
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_reactions)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Reaction bar visible on issue detail and comments with Write+ toggle affordance"
    requirement: ISS-01
    verification:
      - kind: automated_ui
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#reaction bar visibility + Write+ toggle"
        status: pass
    human_judgment: false

plan_head_before: 686a779562fa672ed5fdbfb1cb61e1ec30245d93
duration: 10min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 08: GitHub Eight Emoji Reactions Summary

**Write+ can toggle GitHub’s eight emoji reactions on issues and comments via `issue.reactions.toggle`, with Octane reaction bars on detail and comment threads.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-09-14T16:06:02Z
- **Completed:** 2026-09-14T16:16:00Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments

- Implemented `issue.reactions.toggle` (target `issue`|`comment`) with strict GitHub-eight enum, Write+ gate, and soft private deny
- Embedded aggregated reaction groups on `IssuePublic` / `IssueCommentPublic` for UI counts and viewer state
- Shipped `IssueReactionsBar` on issue detail + each comment; greened Wave 0 integration assertions

## Task Commits

Each task was committed atomically:

1. **Task 1 (TDD RED):** `e9d646e` — test(11-08): add failing tests for reaction toggle RPC
2. **Task 1 (TDD GREEN):** `6529187` — feat(11-08): implement issue.reactions.toggle for GitHub eight
3. **Task 2:** `c209f0e` — feat(11-08): add reaction bar UI on issue detail and comments

## Files Created/Modified

- `crates/oxidean-api/tests/issue_reactions.rs` — integration coverage for eight contents, bad_input, Read deny, toggle-off
- `crates/oxidean-db/src/issues.rs` — dialect toggle + aggregate list for issue/comment reactions
- `crates/oxidean-api/src/issue/mod.rs` — `reactions_toggle` + reaction embedding in public mappers
- `apps/web/src/components/repo/issue-reactions.tsrx` — Octane toolbar / Write+ chips
- `apps/web/src/routes/$owner.$repo.issues.$n.tsrx` — issue-level bar under body
- `apps/web/src/components/repo/issue-comments.tsrx` — per-comment bar
- `packages/api-client/src/index.ts` — generated client (`make rpc-gen`)

## Decisions Made

- Full eight contents (discretion lock from RESEARCH) — avoids a second migration
- `viewerHasReacted` camelCase on wire for group DTO; content strings match GitHub REST
- Plan path `$number` maps to existing `$n` route file (no rename)

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written aside from path naming below.

### Other Deviations

**1. Detail route filename**
- **Found during:** Task 2
- **Issue:** Plan listed `$owner.$repo.issues.$number.tsrx`; repo uses `$owner.$repo.issues.$n.tsrx`
- **Action:** Wired reactions into `$n` route (existing Phase 11 convention)
- **Impact:** None on behavior

## Threat Mitigations

| Threat ID | Disposition | How addressed |
|-----------|-------------|-----------------|
| T-11-13 | mitigate | `ReactionContent` serde enum → `rpc.bad_input` for unknown |
| T-11-06 | mitigate | `resolve_for_write` / soft `repo.not_found` for Read-only private |
| T-11-SC | mitigate | No new packages |

## Known Stubs

None.

## Verification

- `make rpc-gen && make rpc-sync-check` — ok
- `cargo nextest run -p oxidean-api -E 'test(issue_reactions)'` — 3 passed
- `bun run build` (apps/web) — ok
- `bunx vitest run 'src/routes/$owner.$repo.issues.integration.test.ts'` — 12 passed

## Self-Check: PASSED

- FOUND: `apps/web/src/components/repo/issue-reactions.tsrx`
- FOUND: `crates/oxidean-api/tests/issue_reactions.rs`
- FOUND: commits `e9d646e`, `6529187`, `c209f0e`
