---
phase: 21-social-explore
plan: "00"
subsystem: testing
tags: [stars, forks, explore, profiles, wave0, nextest, vitest]
requires:
  - phase: 12-pull-requests
    provides: repo.fork + forked_from_repo_id + clone_bare
provides:
  - Wave 0 discoverable stubs for SOC-01…04
affects: [21-01, 21-02, 21-03, 21-04, 21-05, 21-06]
actuals:
  tokens: 4500
  tasks: 2
  commits: 1
plan_head_before: e7fc68e8260950fd81352e2e8bc76e9ec06a6b1f
tech-stack:
  added: []
  patterns: ["Wave 0 #[ignore] stubs documenting RPC contracts"]
key-files:
  created:
    - crates/oxidean-api/tests/repo_stars.rs
    - crates/oxidean-api/tests/repo_fork.rs
    - crates/oxidean-api/tests/repo_explore.rs
    - crates/oxidean-api/tests/user_public_profile.rs
    - crates/oxidean-db/tests/dialect_social.rs
    - apps/web/src/routes/explore.integration.test.ts
    - apps/web/src/components/repo/repo-chrome.social.integration.test.ts
  modified: []
key-decisions:
  - "Migration target is 0017_social (0016 already used by Phase 12 pull_requests)"
  - "Extend Phase 12 forked_from_repo_id; do not invent parallel fork API"
requirements-completed: [SOC-01, SOC-02, SOC-03, SOC-04]
coverage:
  - id: D1
    description: Wave 0 nextest/vitest stub paths for SOC-* exist
    requirement: SOC-01
    verification:
      - kind: other
        ref: "cargo nextest list -p oxidean-api -E 'test(repo_stars)' --run-ignored all"
        status: pass
    human_judgment: false
duration: 15min
completed: 2026-09-16
status: complete
---

# Phase 21 Plan 00: Wave 0 Social Stubs Summary

**Nyquist stubs for stars, forks, explore, public profiles, dialect social, and web chrome/explore contracts.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2/2

## Accomplishments

- Added ignored nextest integration stubs for `repo_stars`, `repo_fork`, `repo_explore`, `user_public_profile`
- Added `dialect_social` expecting `0017_social` (stars + `fork_network_id`)
- Added Vitest stubs for `/explore` and RepoChrome Star/Fork affordances

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Critical] Migration id 0017 not 0016**
- **Found during:** Task 1
- **Issue:** Plan assumed `0016_social`; filesystem already has `0016_pull_requests`
- **Fix:** Stubs target `0017_social`
- **Files modified:** `dialect_social.rs`
- **Commit:** 81c38ce

## Self-Check: PASSED

- FOUND: all seven Wave 0 stub files
- FOUND: 81c38ce
