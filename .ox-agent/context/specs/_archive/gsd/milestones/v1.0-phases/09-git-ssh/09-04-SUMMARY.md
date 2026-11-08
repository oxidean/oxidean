---
phase: 09-git-ssh
plan: "04"
subsystem: api
tags: [ssh, acl, rate-limit, git-receive-pack, FailedAuthLimiter]
requires:
  - phase: 09-03
    provides: russh tracer + public upload-pack + sshKey RPC
provides:
  - Smart HTTP ACL parity for SSH pack commands
  - git-receive-pack with verified-email gate
  - IP + fingerprint failed-auth rate limits (D-SSH-07)
affects: [09-05-compose, 09-06-rpc-gen, 09-07-ui]
actuals:
  tokens: 6170
  tasks: 2
  commits: 1
plan_head_before: "ec73a427c8525f28c5204c5984bc1d91293f6560"
tech-stack:
  added: []
  patterns:
    - "SSH ACL reuses can_read_as_owner / is_private_visibility; denials via git stderr"
    - "SshAuthLimiter aliases PAT FailedAuthLimiter; fingerprint as user bucket"
key-files:
  created:
    - crates/oxidean-api/src/ssh/rate_limit.rs
  modified:
    - crates/oxidean-api/src/ssh/pack.rs
    - crates/oxidean-api/src/ssh/server.rs
    - crates/oxidean-api/src/ssh/mod.rs
    - crates/oxidean-api/tests/git_ssh.rs
key-decisions:
  - "Count failures only on auth_publickey Reject (not offered); clear fingerprint on Accept"
  - "Single feat commit for ACL + rate-limit (inseparable dirty tree after green suite)"
requirements-completed: [GIT-03, GIT-04]
coverage:
  - id: D1
    description: Private non-owner fetch denied; unverified receive-pack denied via git stderr
    requirement: GIT-03
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(git_ssh)'
        status: pass
    human_judgment: false
  - id: D2
    description: Failed pubkey spray rate-limited; max-25 and duplicate fingerprint already enforced
    requirement: GIT-04
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(ssh_key) | test(git_ssh)'
        status: pass
    human_judgment: false
duration: 20min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 04: SSH ACL + Rate Limit Summary

**SSH pack commands now match Smart HTTP owner/verify ACL, receive-pack is gated on verified email, and failed pubkey auth is rate-limited per IP and fingerprint.**

## Performance

- **Duration:** ~20 min
- **Tasks:** 2
- **Files:** 5

## Accomplishments

- Extended pack handler for upload-pack + receive-pack with `can_read_as_owner` / `is_private_visibility` and forge-shaped git stderr denials.
- Wired `SshAuthLimiter` (PAT `FailedAuthLimiter`) on reject/accept paths; touch `last_used` on successful auth.
- Greened private deny, unverified push, and rate-limit `git_ssh` cases; all 14 `ssh_key`/`git_ssh` tests pass.

## Task Commits

1. **Task 1+2: ACL parity + rate limit** - `93474fd` (feat)

## Files Created/Modified

- `crates/oxidean-api/src/ssh/rate_limit.rs` — SshAuthLimiter alias
- `crates/oxidean-api/src/ssh/pack.rs` — ACL + receive-pack + stderr denials
- `crates/oxidean-api/src/ssh/server.rs` — rate-limit + last_used wiring
- `crates/oxidean-api/tests/git_ssh.rs` — greened ACL/rate-limit cases

## Decisions Made

- Reuse PAT limiter buckets (20/IP, 10/fingerprint, 15m) with fingerprint as user key.
- Max-25 / duplicate fingerprint already greened in 09-03; no ssh_keys changes required.

## Deviations from Plan

### Auto-fixed Issues

None beyond combining Task 1+2 into one commit because implementation and tests landed together after the green suite.

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/ssh/rate_limit.rs
- FOUND: 93474fd
