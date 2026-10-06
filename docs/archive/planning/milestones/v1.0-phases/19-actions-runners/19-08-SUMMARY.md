---
phase: 19-actions-runners
plan: "08"
subsystem: ops
tags: [actions, runner-image, compose, docs]

requires:
  - phase: 19-actions-runners
    provides: runner protocol
provides:
  - docker/oxidean-runner official image
  - Compose profile actions + operator docs
affects: [19-11]

actuals:
  tokens: 3500
  tasks: 2
  commits: 2

tech-stack:
  added: [gitea/act_runner:0.2.11]
  patterns: [Compose profile opt-in, OXIDEAN_PUBLIC_ORIGIN for register]

key-files:
  created:
    - docker/oxidean-runner/Dockerfile
    - docker/oxidean-runner/entrypoint.sh
    - docker/oxidean-runner/README.md
  modified:
    - docker-compose.yml
    - docs/DEPLOYMENT.md
    - docs/CONFIGURATION.md
    - scripts/smoke-actions.sh

key-decisions:
  - "Pin act_runner 0.2.11; default ubuntu-latest:docker://node:20-bookworm"
  - "Docker build credential failure is skip-ok for smoke on WSL"

patterns-established:
  - "Never commit OXIDEAN_RUNNER_REGISTRATION_TOKEN"

requirements-completed: [ACT-04, ACT-05]

coverage:
  - id: D1
    description: Dockerfile + README + compose + smoke artifact checks
    requirement: ACT-04
    verification:
      - kind: smoke
        ref: bash scripts/smoke-actions.sh
        status: pass
    human_judgment: false

plan_head_before: f1f3cde
duration: 15min
completed: 2026-09-16
status: complete
---

# Phase 19 Plan 08: Official runner image Summary

**Official `docker/oxidean-runner` (act_runner) plus Compose `actions` profile and operator docs; smoke verifies artifacts (live stack optional).**

## Deviations from Plan

**1. [Rule 3] Docker build skipped on WSL credential helper**
- smoke still passes Dockerfile/compose/README gates via skip-ok.

## Self-Check: PASSED
