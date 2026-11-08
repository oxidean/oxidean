---
phase: 05-cloud-verify-reset
plan: "06"
subsystem: auth
tags: [auth, verify, input-otp, rpc-gen, verify-banner, email-verification]

requires:
  - phase: 05-cloud-verify-reset
    provides: auth.verify / resend_verify / request_verify RPCs + email_verified on UserPublic (Rust)
provides:
  - Regenerated api-client with email_verified + typed verify/reset RPCs
  - /verify route with InputOtp (input-otp@1.5.0) and AuthShell UX
  - Persistent VerifyBanner under SiteHeader for unverified sessions
affects:
  - AUTH-04 user-visible verification experience
  - 05-07 /reset-password (reuses InputOtp + reset client methods)

actuals:
  tokens: 7041
  tasks: 3
  commits: 2

plan_head_before: 3570592321adee2762f0251c02c25f74d259e4ab

tech-stack:
  added: [input-otp@1.5.0]
  patterns:
    - "rpc-gen is sole writer of packages/api-client — never hand-edit"
    - "Thin local InputOtp wrapper around npm input-otp (not shadcn registry)"
    - "VerifyBanner mounts between SiteHeader and main; gated on auth.me.email_verified === false"

key-files:
  created:
    - apps/web/src/components/ui/input-otp.tsrx
    - apps/web/src/routes/verify.tsrx
    - apps/web/src/components/verify-banner.tsrx
  modified:
    - crates/oxidean-api/src/bin/rpc_gen.rs
    - packages/api-client/src/index.ts
    - apps/web/package.json
    - apps/web/src/routes/__root.tsrx
    - apps/web/src/styles.css
    - apps/web/src/routeTree.gen.ts

key-decisions:
  - "Human approved input-otp@1.5.0 legitimacy gate (Task 1) — install proceeded"
  - "Token auto-consume invalid_token while signed-in maps to wrong-user copy; code failures map to invalid/expired"
  - "Client-side 60s resend cooldown mirrors server MIN_ISSUE_INTERVAL_SECS"

patterns-established:
  - "InputOtp: controlled value + onComplete auto-submit + explicit primary button"
  - "Referrer-Policy no-referrer via /verify head meta (T-05-15)"
  - "Banner appear uses .oct-verify-banner opacity animation; reduced-motion instant"

requirements-completed: [AUTH-04]

coverage:
  - id: D1
    description: "Generated api-client UserPublic includes email_verified; verify/resend/request_verify/reset RPCs typed"
    requirement: AUTH-04
    verification:
      - kind: other
        ref: "cargo run -p oxidean-api --bin rpc-gen && grep email_verified packages/api-client/src/index.ts"
        status: pass
      - kind: other
        ref: "bun run build (apps/web)"
        status: pass
    human_judgment: false
  - id: D2
    description: "/verify with InputOtp 8 slots, AuthShell copy, token/code flows, Referrer-Policy no-referrer"
    requirement: AUTH-04
    verification:
      - kind: other
        ref: "bun run build (apps/web) — verify chunk emitted"
        status: pass
    human_judgment: true
    rationale: "OTP interaction, wrong-user vs expired copy, and layout wrap need visual UAT"
  - id: D3
    description: "VerifyBanner under SiteHeader for signed-in unverified sessions with Resend + Enter code"
    requirement: AUTH-04
    verification:
      - kind: other
        ref: "bun run build (apps/web) — VerifyBanner mounted in __root"
        status: pass
    human_judgment: true
    rationale: "Banner visibility after verify and light/dark chrome need human-check per plan"

duration: 4min
completed: 2026-09-10
status: complete
---

# Phase 5 Plan 06: Verify UI (rpc-gen, InputOtp, /verify, VerifyBanner) Summary

**Typed api-client with `email_verified`, `/verify` OTP+magic UX via `input-otp@1.5.0`, and persistent VerifyBanner under chrome**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-10T22:42:57Z
- **Completed:** 2026-09-10T22:46:55Z
- **Tasks:** 3 (1 checkpoint approved + 2 auto)
- **Files modified:** 10

## Accomplishments

- Regenerated `packages/api-client` via `rpc-gen` with `email_verified` and verify/reset client methods
- Shipped `/verify` (AuthShell + InputOtp auto-submit + Verify email + resend cooldown + token query flow)
- Mounted `VerifyBanner` under `SiteHeader` when `auth.me.email_verified === false`

## Task Commits

Each task was committed atomically:

1. **Task 1: Confirm input-otp@1.5.0 legitimacy** — checkpoint (no code commit); **human approved** in orchestrator session
2. **Task 2: rpc-gen + InputOtp + /verify page** — `caabb2f` (feat)
3. **Task 3: Persistent VerifyBanner under header** — `0c5a7c1` (feat)

**Plan metadata:** `4639d88` (docs: complete plan)

## Files Created/Modified

- `crates/oxidean-api/src/bin/rpc_gen.rs` — UserPublic.email_verified + verify/reset/privilegedPing client surface
- `packages/api-client/src/index.ts` — regenerated (do not hand-edit)
- `apps/web/package.json` / `bun.lock` — `input-otp@1.5.0`
- `apps/web/src/components/ui/input-otp.tsrx` — thin OTP wrapper (8 numeric slots)
- `apps/web/src/routes/verify.tsrx` — `/verify` page per UI-SPEC
- `apps/web/src/components/verify-banner.tsrx` — chrome verify strip
- `apps/web/src/routes/__root.tsrx` — mount VerifyBanner between header and main
- `apps/web/src/styles.css` — banner appear motion
- `apps/web/src/routeTree.gen.ts` — `/verify` route registration

## Decisions Made

- Human approved `input-otp@1.5.0` legitimacy gate before install (Task 1)
- Signed-in magic-link `auth.invalid_token` → wrong-user copy; code form failures → invalid/expired copy
- Client 60s resend cooldown mirrors server soft rate limit interval

## Deviations from Plan

None - plan executed exactly as written.

## Authentication Gates

None

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for 05-07 (`/reset-password`, login forgot link, privileged CTA pattern). InputOtp and reset client methods are already available.

---
*Phase: 05-cloud-verify-reset*
*Completed: 2026-09-10*

## Self-Check: PASSED
