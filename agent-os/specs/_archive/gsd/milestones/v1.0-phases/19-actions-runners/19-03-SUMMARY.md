---
phase: 19-actions-runners
plan: "03"
subsystem: api
tags: [actions, workflow, yaml, serde_yaml]

requires:
  - phase: 19-actions-runners
    provides: Actions schema + Wave 0 parse stubs
provides:
  - Workflow discovery under .github/workflows
  - GHA-compatible YAML subset parser (WorkflowDocument / JobSpec)
affects: [19-04, 19-06]

actuals:
  tokens: 4500
  tasks: 2
  commits: 2

tech-stack:
  added: [serde_yaml 0.9]
  patterns: [ls_tree+cat_blob discovery, ParseError for failed-run messages]

key-files:
  created:
    - crates/oxidean-api/src/actions/mod.rs
    - crates/oxidean-api/src/actions/parse.rs
    - crates/oxidean-api/src/actions/workflow.rs
  modified:
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/lib.rs
    - crates/oxidean-api/tests/actions_workflow_parse.rs

key-decisions:
  - "Flat files only under .github/workflows; path escape rejected"
  - "MAX_WORKFLOW_BYTES = 1 MiB"

patterns-established:
  - "actions::discover_workflows + parse_workflow_yaml; no in-process step execution"

requirements-completed: [ACT-01]

coverage:
  - id: D1
    description: Discover and parse workflows from commit tree
    requirement: ACT-01
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(actions_workflow_parse)'
        status: pass
    human_judgment: false

plan_head_before: 2ff0828
duration: 8min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 03: Workflow discover + parse Summary

**GitHub Actions–compatible workflow discovery via `ls_tree`/`cat_blob` and YAML subset parsing into `WorkflowDocument` / `JobSpec` — no in-process job execution.**

## Performance

- **Duration:** ~8 min
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Added `serde_yaml` 0.9 (VERIFIED)
- Implemented `actions::parse` + `actions::workflow` with size/path confinement
- Greened `actions_workflow_parse` (4 tests)

## Task Commits

1. **Task 1: Add serde_yaml** - `1df0735` (chore)
2. **Task 2: Discover + parse workflow subset** - `2a08f1d` (feat)

## Deviations from Plan

None material — db/actions.rs unchanged (persist helpers already from 19-02).

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/actions/parse.rs
- FOUND: crates/oxidean-api/src/actions/workflow.rs
- FOUND: actions_workflow_parse tests green
