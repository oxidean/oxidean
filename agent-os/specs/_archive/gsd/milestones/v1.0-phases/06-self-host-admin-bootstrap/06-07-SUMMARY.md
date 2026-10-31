---
phase: 06-self-host-admin-bootstrap
plan: "07"
subsystem: docs
tags: [docs, REQUIREMENTS, rpc-gen, ALLOW_SIGNUP, empty-instance, D-02, AUTH-06, AUTH-07]

requires:
  - phase: 06-self-host-admin-bootstrap
    provides: ENV seed + wizard + confirm_admin + allow_signup RPC/UI (06-02…06-09)
provides:
  - "AUTH-06/07 reframed as empty-instance (not On self-host)"
  - "CONFIGURATION/.env.example document OXIDEAN_ALLOW_SIGNUP default false + ADMIN seed"
  - "ARCHITECTURE/API describe ENV seed, wizard, forced credentials, RPC allowlist, allow_signup"
  - "rpc-gen synced api-client; flicker todo closed"
  - "06-COVERAGE.md no external API declaration"
affects:
  - phase 06 UAT / verify-work
  - operator onboarding docs

actuals:
  tokens: 4820
  tasks: 2
  commits: 4

plan_head_before: 79287d55c4906cda8ed67342e8f2eccd6c3ef04f

tech-stack:
  added: []
  patterns:
    - "Empty-instance product docs — no cloud/self-host documentation forks"
    - "OXIDEAN_ALLOW_SIGNUP fail-closed default documented beside ADMIN seed"

key-files:
  created: []
  modified:
    - docs/CONFIGURATION.md
    - docs/ARCHITECTURE.md
    - docs/API.md
    - .env.example
    - .planning/REQUIREMENTS.md
    - .planning/phases/06-self-host-admin-bootstrap/06-COVERAGE.md
    - packages/api-client/src/index.ts
    - .planning/todos/completed/2026-09-11-fix-signed-in-home-flicker-on-load.md

key-decisions:
  - "AUTH-05 keeps checkbox; v1 note clarifies allow_signup supersedes always-open cloud signup"
  - "Document cloud OXIDEAN_ALLOW_SIGNUP=true in manifests — no Compose file change (Open Q2)"

patterns-established:
  - "Operator docs describe one empty-instance bootstrap path for cloud and self-host"

requirements-completed: [AUTH-06, AUTH-07]

coverage:
  - id: D1
    description: "REQUIREMENTS AUTH-06/07 empty-instance wording; AUTH-05 allow_signup v1 note"
    requirement: AUTH-06
    verification:
      - kind: other
        ref: "rg empty.instance .planning/REQUIREMENTS.md"
        status: pass
    human_judgment: false
  - id: D2
    description: "CONFIGURATION + .env.example document OXIDEAN_ALLOW_SIGNUP default false and ADMIN seed / system-administrator"
    requirement: AUTH-07
    verification:
      - kind: other
        ref: "rg OXIDEAN_ALLOW_SIGNUP docs/CONFIGURATION.md .env.example"
        status: pass
    human_judgment: false
  - id: D3
    description: "ARCHITECTURE/API describe ENV seed, wizard, confirm_admin, allow_signup, must_change"
    requirement: AUTH-06
    verification:
      - kind: other
        ref: "docs/ARCHITECTURE.md + docs/API.md procedure table"
        status: pass
    human_judgment: false
  - id: D4
    description: "rpc-gen client includes bootstrapSetup/confirmAdminCredentials/allow_signup/must_change_credentials; smoke green"
    requirement: AUTH-07
    verification:
      - kind: other
        ref: "cargo run -p oxidean-api --bin rpc-gen; cargo nextest …bootstrap|seeded_admin|signup; bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Folded flicker todo moved to completed; COVERAGE.md no external API"
    verification:
      - kind: other
        ref: ".planning/todos/completed/2026-09-11-fix-signed-in-home-flicker-on-load.md; 06-COVERAGE.md"
        status: pass
    human_judgment: false

duration: 2min
completed: 2026-09-11
status: complete
---

# Phase 06 Plan 07: Docs, REQUIREMENTS, rpc-gen, COVERAGE Summary

**Empty-instance bootstrap documented end-to-end; AUTH-06/07 reframed; generated client + phase smoke green; flicker todo closed.**

## Performance

- **Duration:** 2min
- **Started:** 2026-09-11T21:36:09Z
- **Completed:** 2026-09-11T21:38:20Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Reframed AUTH-06/07 as empty-instance (dropped “On self-host”); AUTH-05 v1 note ties open signup to `allow_signup` / `OXIDEAN_ALLOW_SIGNUP`
- Documented ADMIN seed (`system-administrator`, fail-closed), wizard path, forced credentials, RPC allowlist, and `OXIDEAN_ALLOW_SIGNUP` default false in CONFIGURATION / ARCHITECTURE / API / `.env.example`
- Regenerated `@oxidean/api-client` via `rpc-gen`; closed folded signed-in home flicker todo; `06-COVERAGE.md` declares no external API this phase

## Task Commits

Each task was committed atomically:

1. **Task 1: Docs + REQUIREMENTS reframe + COVERAGE** - `e4565ef` (docs)
2. **Task 2: rpc-gen + flicker todo close + phase smoke** - `85fe356` (chore)

**Plan metadata:** `8b0cd03` (docs: complete plan)

## Files Created/Modified

- `docs/CONFIGURATION.md` — ADMIN + ALLOW_SIGNUP + fail-closed seed defaults
- `docs/ARCHITECTURE.md` — empty-instance bootstrap / allow_signup narrative
- `docs/API.md` — bootstrap/confirm/provider_config/admin allow_signup procedures
- `.env.example` — commented `OXIDEAN_ALLOW_SIGNUP=false` beside ADMIN vars
- `.planning/REQUIREMENTS.md` — AUTH-05/06/07 + out-of-scope invite note
- `.planning/phases/06-self-host-admin-bootstrap/06-COVERAGE.md` — no external API one-liner
- `packages/api-client/src/index.ts` — rpc-gen sync (comment trim on `allow_signup`)
- `.planning/todos/completed/2026-09-11-fix-signed-in-home-flicker-on-load.md` — moved from pending

## Decisions Made

- Keep AUTH-05 checked; clarify via v1 note that instance `allow_signup` (default closed) governs local signup after bootstrap; cloud manifests set `OXIDEAN_ALLOW_SIGNUP=true` when open signup is desired
- No Compose overlay change for ALLOW_SIGNUP (Open Q2 resolved: docs/manifest note sufficient)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `bun --cwd apps/web run build` is invalid on this Bun CLI**
- **Found during:** Task 2 (smoke verify)
- **Issue:** `bun --cwd apps/web run build` prints usage and does not run the package script
- **Fix:** Ran `cd apps/web && bun run build` instead (exit 0)
- **Files modified:** none
- **Commit:** n/a (verify-only)

## Auth Gates Encountered

None.

## Known Stubs

None.

## Threat Flags

None — docs/rpc-gen only; placeholders in `.env.example`; no new trust-boundary surface beyond plan threat model T-06-15 / T-06-SC.

## Self-Check: PASSED

- FOUND: docs/CONFIGURATION.md, docs/ARCHITECTURE.md, docs/API.md, .env.example, REQUIREMENTS.md, 06-COVERAGE.md, api-client index, completed flicker todo
- FOUND: commits e4565ef, 85fe356
