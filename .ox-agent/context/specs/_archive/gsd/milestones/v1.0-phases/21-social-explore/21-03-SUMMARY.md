---
phase: 21-social-explore
plan: "03"
subsystem: ui
tags: [profiles, getPublicProfile, owner-index]
requires:
  - phase: 21-01
    provides: social migration
provides:
  - user.getPublicProfile
  - /$owner user vs org branch
affects: [21-07]
actuals:
  tokens: 15000
  tasks: 3
  commits: 1
requirements-completed: [SOC-02]
coverage:
  - id: D1
    description: Public profile without email + ACL repos
    requirement: SOC-02
    verification:
      - kind: integration
        ref: "test(user_public_profile)"
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-16
status: complete
---

# Phase 21 Plan 03: Public Profiles Summary

**user.getPublicProfile (no email) and $owner.index branching for users vs orgs with Stars tab for self.**

## Self-Check: PASSED
