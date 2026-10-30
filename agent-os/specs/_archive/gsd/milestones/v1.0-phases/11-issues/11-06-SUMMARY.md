---
phase: 11-issues
plan: "06"
subsystem: api
tags: [issues, labels, rpc, acl, octane, org-settings, iss-03, d-iss-05, d-iss-07, d-iss-20]

requires:
  - phase: 11-issues
    provides: "0011_issues schema with labels + repo_hidden_labels + issue_labels (11-02)"
  - phase: 11-issues
    provides: "issue lifecycle CRUD + detail UI (11-04)"
  - phase: 11-issues
    provides: "Wave 0 issue_labels RED stubs (11-00)"
provides:
  - "label.listForRepo|listForOrg|create|update|delete RPC"
  - "Effective label set = org (− hidden) ∪ repo-local (D-ISS-05)"
  - "issue.labels.set Write+ from effective ids (D-ISS-07 / D-ISS-20)"
  - "Org settings Labels + repo Issues→Labels Admin UI + issue sidebar picker"
affects:
  - 11-07 assignees (ISS-03 remainder)
  - 11-09 list filters by label

actuals:
  tokens: 26978
  tasks: 3
  commits: 3

plan_head_before: 7eba7337d15d54eaeba7b10aab2dd7deae57060b

tech-stack:
  added: []
  patterns:
    - "Dual-scope labels: inherit org + hide override + repo-local; assign by id"
    - "Admin defs via org owner/admin or repo can_admin; Write+ issue.labels.set"
    - "label.listForRepo includeHidden=true for Admin settings catalog"

key-files:
  created:
    - crates/oxidean-api/src/label/mod.rs
    - apps/web/src/routes/$owner.settings.labels.tsrx
    - apps/web/src/routes/$owner.$repo.issues.labels.tsrx
    - apps/web/src/components/repo/issue-labels-panel.tsrx
  modified:
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/issue/mod.rs
    - crates/oxidean-db/src/issue_labels.rs
    - crates/oxidean-core/src/issue_types.rs
    - crates/oxidean-api/tests/issue_labels.rs
    - packages/api-client/src/index.ts
    - apps/web/src/components/org/org-settings-nav.tsrx
    - apps/web/src/components/repo/issues-list.tsrx
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.integration.test.ts

key-decisions:
  - "Effective set = inherited org labels (− hidden) ∪ repo-local; unique name within scope (D-ISS-05 discretion)"
  - "Hide org labels via label.update { hidden, repo }; only org labels can be hidden"
  - "Write without Admin gets repo.not_found on def mutations (anti-enumeration)"

patterns-established:
  - "Top-level label.* for definitions; issue.labels.set for assignment"
  - "Org settings Labels tab + /$owner/$repo/issues/labels Admin page"
  - "IssueLabelsPanel checkbox save for Write+; read-only chips otherwise"

requirements-completed: [ISS-03]

coverage:
  - id: D1
    description: "Admin CRUD org + repo label definitions; non-Admin Write denied"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_labels)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "Effective listForRepo merges org (− hidden) ∪ repo-local"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_labels_effective)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "Write+ issue.labels.set; Read denied; hidden ids rejected"
    requirement: ISS-03
    verification:
      - kind: integration
        ref: "cargo nextest run -p oxidean-api -E 'test(issue_labels_write_assign)'"
        status: pass
    human_judgment: false
  - id: D4
    description: "Org/repo Labels settings + issue picker UI; Admin Labels link gated"
    requirement: ISS-03
    verification:
      - kind: automated_ui
        ref: "bunx vitest run src/routes/$owner.$repo.issues.integration.test.ts"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false

duration: 14min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 06: Org/repo labels + assignment Summary

**Dual-scope label catalog (org defaults + repo hide/local) with Admin defs and Write+ issue assignment (ISS-03 / D-ISS-05 / D-ISS-07).**

## Performance

- **Duration:** 14 min
- **Started:** 2026-09-14T15:33:06Z
- **Completed:** 2026-09-14T15:47:12Z
- **Tasks:** 3
- **Files modified:** 18

## Accomplishments

- Shipped `label.*` RPC for org catalog + repo local/hide overrides with Admin gates
- Effective `listForRepo` merge + `issue.labels.set` validating against non-hidden effective ids
- Org settings Labels page, repo Issues→Labels Admin UI, and issue sidebar picker

## Task Commits

1. **Task 1: label.* defs + effective listForRepo** - `f690be6` (feat)
2. **Task 2: issue.labels.set Write+** - `8338a18` (feat)
3. **Task 3: Org/repo Labels settings + issue picker UI** - `fefdcf9` (feat)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed map_label collect into Result**
- **Found during:** Task 1
- **Issue:** `rows.iter().map(|r| map_label!(r)).collect()` failed because `?` inside the macro cannot return from a non-Result closure
- **Fix:** Use for-loop `out.push(map_label!(r))` matching `issues.rs` patterns
- **Files modified:** `crates/oxidean-db/src/issue_labels.rs`
- **Commit:** `f690be6`

**2. [Rule 1 - Bug] Raw-string `#` color broke issue_labels test compile**
- **Found during:** Task 1
- **Issue:** `r#"..."#d73a4a..."#` terminated early on `"#`
- **Fix:** Pass bare hex `d73a4a` (API already normalizes)
- **Files modified:** `crates/oxidean-api/tests/issue_labels.rs`
- **Commit:** `f690be6`

**3. [Rule 1 - Bug] History assertion raced after Labels panel query**
- **Found during:** Task 3
- **Issue:** `getByText(/History|Edit history/i)` matched both "Edit history" heading and "Loading history…"
- **Fix:** Assert `getByRole("heading", { name: /^Edit history$/i })`
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.integration.test.ts`
- **Commit:** `fefdcf9`

## Threat Flags

None — label definition Admin gates and assignment Write+ match plan threat model (T-11-02, T-11-11). No new packages (T-11-SC).

## Self-Check: PASSED

- FOUND: `crates/oxidean-api/src/label/mod.rs`
- FOUND: `apps/web/src/routes/$owner.settings.labels.tsrx`
- FOUND: `apps/web/src/routes/$owner.$repo.issues.labels.tsrx`
- FOUND: `apps/web/src/components/repo/issue-labels-panel.tsrx`
- FOUND: commits `f690be6`, `8338a18`, `fefdcf9`
