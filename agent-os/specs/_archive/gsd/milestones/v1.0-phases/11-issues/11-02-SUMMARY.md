---
phase: 11-issues
plan: "02"
subsystem: database
tags: [issues, schema, migrations, issue-counters, d-iss-01, iss-01, iss-02, iss-03, iss-04]

requires:
  - phase: 11-issues
    provides: Wave 0 dialect_issues RED stub + 0011_issues migration id locked
  - phase: 10-orgs-permissions
    provides: 0010_orgs_acl repositories/orgs CASCADE patterns + Database dialect helpers
provides:
  - "Tri-dialect 0011_issues (issues, counters, comments, revisions, labels, assignees, reactions, links)"
  - "issue_counters monotonic #N allocate + insert_issue in same txn; hard-delete does not reclaim"
  - "issue_types DTOs/enums for later issue.* / label.* RPC"
  - "Minimal label/assignee DB helpers for later plans"
affects:
  - 11-03 issue.create/list/get tracer
  - 11-04+ lifecycle/comments/labels/reactions/links
  - 11-12 factory_reset_issues cascade proof

actuals:
  tokens: 16498
  tasks: 2
  commits: 5

plan_head_before: 85e1260adcfd295d207c7266dd61d8ad6b0dd174

tech-stack:
  added: []
  patterns:
    - "Gitea-style issue_counters(repo_id PK, max_number) upserted in insert txn"
    - "Dual-scope labels XOR org_id/repo_id + repo_hidden_labels for inherit+hide+local-only"
    - "GitHub eight reaction content CHECK; issue_links kind issue|pr_stub"

key-files:
  created:
    - crates/oxidean-db/migrations/postgres/0011_issues.sql
    - crates/oxidean-db/migrations/mysql/0011_issues.sql
    - crates/oxidean-db/migrations/sqlite/0011_issues.sql
    - crates/oxidean-db/src/issues.rs
    - crates/oxidean-db/src/issue_labels.rs
    - crates/oxidean-core/src/issue_types.rs
  modified:
    - crates/oxidean-db/src/lib.rs
    - crates/oxidean-core/src/lib.rs
    - crates/oxidean-db/tests/dialect_issues.rs

key-decisions:
  - "T0 proceed_0011 — honor D-ISS-01 with issue_counters (executor confirmed from orchestrator instructions)"
  - "Label hide overrides live in repo_hidden_labels (not duplicate org+repo name pairs)"
  - "issue_links kinds: issue | pr_stub with opaque id + optional target_number/title"
  - "No issue.* RPC or UI in this plan (deferred to 11-03+)"

patterns-established:
  - "Per-repo #N: allocate_next_number / insert_issue never decrement counters on delete"
  - "Issue domain DTOs in oxidean-core::issue_types; dialect SQL only in oxidean-db"

requirements-completed: [ISS-01, ISS-02, ISS-03, ISS-04]

coverage:
  - id: D1
    description: "Tri-dialect 0011_issues files include issues, counters, comments, revisions, labels, assignees, reactions, links"
    requirement: ISS-01
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-db --lib migration_parity"
        status: pass
      - kind: integration
        ref: "cargo test -p oxidean-db --test dialect_issues -- dialect_issues_tri_dialect_files"
        status: pass
    human_judgment: false
  - id: D2
    description: "Sqlite migrate 0011 + insert_issue allocates #1,#2; after hard-delete next is #3 (no reclaim)"
    requirement: ISS-01
    verification:
      - kind: integration
        ref: "cargo test -p oxidean-db --test dialect_issues -- dialect_issues_migrate_0011_schema_presence"
        status: pass
    human_judgment: false
  - id: D3
    description: "issue_types enums compile (state, GitHub eight reactions, pr_stub link kind)"
    requirement: ISS-04
    verification:
      - kind: unit
        ref: "cargo test -p oxidean-core --lib issue_types"
        status: pass
    human_judgment: false

duration: 7min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 02: Schema door (0011_issues + counters) Summary

**Tri-dialect `0011_issues` with Gitea-style `issue_counters` for one-way per-repo `#N`, plus DB helpers and `issue_types` — no RPC/UI yet.**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-09-14T14:28:14Z
- **Completed:** 2026-09-14T14:34:45Z
- **Tasks:** 2 (T0 decision + T1 implementation)
- **Files modified:** 9

## Accomplishments

- Confirmed schema door `proceed_0011` (D-ISS-01 counter table; public `#N` URLs permanent once issues exist).
- Landed postgres/mysql/sqlite `0011_issues` covering issues, counters, comments, issue/comment revisions, dual-scope labels + hide overrides, assignees, reactions (GitHub eight), and `issue_links` (`issue` | `pr_stub`) with CASCADE from repositories/orgs.
- Wired `Database::insert_issue` / `delete_issue` / label+assignee helpers; greened `dialect_issues` including non-reclaim after hard-delete.

## Task Commits

1. **Task 0: Confirm D-ISS-01 numbering + 0011 schema door** — decision only (`proceed_0011` from executor instructions; no code commit)
2. **Task 1: 0011_issues + DB helpers + issue_types** — `044a081` (feat)

**Plan metadata:** `87e3ad6` (+ follow-up docs for actuals; STATE/ROADMAP untouched)

## Files Created/Modified

- `crates/oxidean-db/migrations/{postgres,mysql,sqlite}/0011_issues.sql` — schema
- `crates/oxidean-db/src/issues.rs` — counter allocate + insert/delete
- `crates/oxidean-db/src/issue_labels.rs` — label insert + set labels/assignees
- `crates/oxidean-core/src/issue_types.rs` — DTOs/enums
- `crates/oxidean-db/tests/dialect_issues.rs` — Wave 0 RED → green behavioral assertions

## Decisions Made

- Proceed with `0011_issues` + `issue_counters` (recommended RESEARCH path).
- Hide model: `repo_hidden_labels(repo_id, label_id)` for org labels; repo-local labels via `labels.repo_id`.
- Reaction content enum locked to GitHub eight; link stubs use `pr_stub`.

## Deviations from Plan

None - plan executed exactly as written (T0 confirmed via orchestrator “honor D-ISS-01 … issue_counters” rather than interactive checkpoint reply).

## Threat Flags

None beyond plan register — mitigations T-11-04 (monotonic counters + UNIQUE) and T-11-05 (CASCADE) implemented in migration/helpers. No new packages (T-11-SC).

## Known Stubs

None for this plan’s goal (schema door). RPC handlers and UI remain intentionally deferred to 11-03+.

## Verification Results

- `cargo test -p oxidean-db --lib migration_parity` — pass
- `cargo test -p oxidean-db --test dialect_issues` — pass
- `cargo test -p oxidean-core --lib issue_types` — pass

## Self-Check: PASSED

- Found: tri-dialect `0011_issues.sql`, `issues.rs`, `issue_labels.rs`, `issue_types.rs`, `11-02-SUMMARY.md`
- Found: task commit `044a081`
