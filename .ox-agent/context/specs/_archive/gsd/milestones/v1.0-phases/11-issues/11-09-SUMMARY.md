---
phase: 11-issues
plan: "09"
subsystem: api
tags: [issue-links, pr-stub, linked-prs, octane, rpc, iss-04]

requires:
  - phase: 11-issues
    provides: "0011_issues issue_links table + issue detail sidebar shell + api-client IssueLinkKind"
provides:
  - "issue.links.list|add|remove for stub-capable rows"
  - "Linked PRs sidebar with stubs + manual Link issue/PR control"
affects: [12-pull-requests, 11-12, web-issues-ui]

actuals:
  tokens: 12872
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "issue_links rows with kind=pr_stub|issue; opaque id + target_number + optional title"
    - "Write+ mutate / Read+ list on accessible issues (D-ISS-20)"
    - "Octane Linked PRs panel with manual link form (no closing-keyword enforcement)"

key-files:
  created:
    - apps/web/src/components/repo/issue-linked-prs.tsrx
  modified:
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - crates/oxidean-api/tests/issue_links.rs
    - packages/api-client/src/index.ts
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "pr_stub and issue kinds only; targetNumber required; optional title soft-capped like issue titles"
  - "Same-repo issue links may soft-fill target_opaque_id when the target exists; missing target still allowed"
  - "Detail route file is $owner.$repo.issues.$n.tsrx (plan named $number)"
  - "Closing keywords remain deferred to Phase 12 (D-ISS-15) — negative test proves no auto-close / no auto-link"

patterns-established:
  - "issue.links.* nested RPC + issueLinks{List,Add,Remove} client helpers from rpc-gen"
  - "IssueLinkedPrsPanel queries links.list; Write+ form posts links.add / Unlink → links.remove"

requirements-completed: [ISS-04]

coverage:
  - id: D1
    description: "Write+ can add/remove stub links; Read+ can list stubs with opaque id + target_number + optional title"
    requirement: ISS-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_links)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Linked PRs panel shows stubs and manual Link issue/PR control"
    requirement: ISS-04
    verification:
      - kind: automated_ui
        ref: "apps/web/src/routes/$owner.$repo.issues.integration.test.ts#Linked PRs panel lists stub rows and Link control"
        status: pass
    human_judgment: false
  - id: D3
    description: "Closing keywords do not auto-close or auto-create links in Phase 11"
    requirement: ISS-04
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_links_no_closing_keyword_enforcement)'"
        status: pass
    human_judgment: false

plan_head_before: 51090295ca385bcc7457293658fb7dc5d3784475
duration: 11min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 09: Linked PR Stubs + Manual Link Control Summary

**issue.links CRUD plus Linked PRs sidebar with pr_stub placeholders and manual Link issue/PR (ISS-04); closing keywords stay deferred.**

## Performance

- **Duration:** 11 min
- **Started:** 2026-09-14T16:17:22Z
- **Completed:** 2026-09-14T16:28:25Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments

- Shipped `issue.links.list|add|remove` with dialect SQL in `oxidean-db` and regenerated `@oxidean/api-client`
- Linked PRs panel lists stubs and exposes Write+ manual link/unlink
- Negative coverage proves keyword comments do not auto-close or auto-link (D-ISS-15)

## Task Commits

1. **Task 1 RED: issue.links failing tests** - `6742637` (test)
2. **Task 1 GREEN: issue.links list/add/remove** - `26da87d` (feat)
3. **Task 2: Linked PRs panel + manual link UI** - `e69e61b` (feat)

## Files Created/Modified

- `crates/oxidean-api/tests/issue_links.rs` — integration coverage for add/remove/list + no keyword enforcement
- `crates/oxidean-core/src/issue_types.rs` — Add/Remove/List link DTOs
- `crates/oxidean-db/src/issues.rs` / `lib.rs` — insert/list/delete `issue_links`
- `crates/oxidean-api/src/issue/mod.rs` / `rpc.rs` / `bin/rpc_gen.rs` — handlers + client codegen
- `packages/api-client/src/index.ts` — generated client
- `apps/web/src/components/repo/issue-linked-prs.tsrx` — Linked PRs sidebar
- `apps/web/src/routes/$owner.$repo.issues.$n.tsrx` — wire panel
- `apps/web/src/routes/$owner.$repo.issues.integration.test.ts` — UI stubs green

## Decisions Made

- Stub shape matches research: opaque link `id`, `kind`, `target_number`, optional `title`
- Issue-kind links default `target_repo_id` to the source repo; PR stubs may omit repo id
- Closing-keyword enforcement explicitly out of scope (Phase 12)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan path used `$number` but route file is `$n`**
- **Found during:** Task 2
- **Issue:** Plan listed `apps/web/src/routes/$owner.$repo.issues.$number.tsrx`; repo uses `$n`
- **Fix:** Edited the existing `$n` route (same pattern as 11-08)
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`
- **Commit:** `e69e61b`

## Known Stubs

| File | Line | Stub | Reason |
|------|------|------|--------|
| `apps/web/src/components/repo/issue-linked-prs.tsrx` | — | `kind=pr_stub` placeholder copy until real PR objects | Intentional D-ISS-13; Phase 12 replaces kind |

## Threat Flags

None — link ACL stays soft not-found for Write; list requires Read+; stubs carry opaque ids (T-11-14 / T-11-SC).

## Self-Check: PASSED

- Created/modified key files present
- Commits `6742637`, `26da87d`, `e69e61b` present on `feat/forge-core`
