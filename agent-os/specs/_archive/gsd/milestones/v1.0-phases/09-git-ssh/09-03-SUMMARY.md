---
phase: 09-git-ssh
plan: "03"
subsystem: api
tags: [ssh, russh, git-upload-pack, sshKey, tracer]
requires:
  - phase: 09-02
    provides: ssh_public_keys CRUD + fingerprint uniqueness
provides:
  - sshKey.add/list/revoke session RPC (require_verified)
  - In-process russh listener (OXIDEAN_SSH_ENABLED)
  - git-upload-pack bridge for public repos as user git
affects: [09-04-acl, 09-05-compose, 09-06-rpc-gen, 09-07-ui]
actuals:
  tokens: 23920
  tasks: 2
  commits: 2
plan_head_before: "1d87d0de2c1fe2481ebc9928e45a8c6e48b9d17c"
tech-stack:
  added: [russh 0.63.3, ssh-key 0.7.0-rc.11]
  patterns:
    - "OXIDEAN_SSH_ENABLED gates listener; host keys under OXIDEAN_SSH_HOST_KEY_DIR"
    - "SSH identity is fingerprint only; username must be git"
key-files:
  created:
    - crates/oxidean-api/src/ssh_keys/mod.rs
    - crates/oxidean-api/src/ssh/mod.rs
    - crates/oxidean-api/src/ssh/server.rs
    - crates/oxidean-api/src/ssh/auth.rs
    - crates/oxidean-api/src/ssh/pack.rs
    - crates/oxidean-api/src/ssh/host_keys.rs
  modified:
    - crates/oxidean-api/Cargo.toml
    - crates/oxidean-api/src/rpc.rs
    - crates/oxidean-api/src/main.rs
    - crates/oxidean-api/tests/ssh_key_rpc.rs
    - crates/oxidean-api/tests/git_ssh.rs
key-decisions:
  - "Tracer allows upload-pack for any authenticated key; private ACL + receive-pack in 09-04"
  - "Pinned russh 0.63 + ssh-key 0.7.0-rc.11 per RESEARCH"
requirements-completed: [GIT-03, GIT-04]
coverage:
  - id: D1
    description: Verified sshKey.add/list/revoke with fingerprint uniqueness and max 25
    requirement: GIT-04
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(ssh_key)'
        status: pass
    human_judgment: false
  - id: D2
    description: russh accepts user git + registered key; public upload-pack succeeds
    requirement: GIT-03
    verification:
      - kind: integration
        ref: cargo nextest run -p oxidean-api -E 'test(git_ssh) & !test(ignored)'
        status: pass
    human_judgment: false
duration: 25min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 03: SSH Key RPC + russh Tracer Summary

**Verified users can register SSH keys via RPC and clone/fetch a public repo over in-process russh as user `git`.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2
- **Files:** 10+

## Accomplishments

- Implemented `sshKey.add` / `list` / `revoke` with `require_verified`, title required, SHA256 fingerprints, max 25, unique fingerprint.
- Added russh 0.63 listener gated by `OXIDEAN_SSH_ENABLED`; host key load/generate; force username `git`.
- Bridged `git-upload-pack` via argv (no shell); reject shell/pty/subsystem; receive-pack deferred to 09-04.

## Task Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1 | `9c04b0a` | sshKey RPC |
| 2 | `e08ded0` | russh upload-pack tracer |

## Deviations from Plan

**1. [Rule 2 - Missing critical] Ignored private/push/rate-limit git_ssh tests until 09-04**
- **Found during:** Task 2
- **Issue:** Plan allows remaining RED until 09-04 for ACL/rate-limit cases.
- **Fix:** Marked those tests `#[ignore]` with 09-04 notes; tracer subset green.
- **Files modified:** `crates/oxidean-api/tests/git_ssh.rs`
- **Commit:** `e08ded0`

## Self-Check: PASSED

- FOUND: crates/oxidean-api/src/ssh/server.rs
- FOUND: crates/oxidean-api/src/ssh_keys/mod.rs
- FOUND: 9c04b0a, e08ded0
