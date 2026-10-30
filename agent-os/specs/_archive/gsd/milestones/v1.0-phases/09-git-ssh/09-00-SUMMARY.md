---
phase: 09-git-ssh
plan: "00"
subsystem: testing
tags: [ssh, git, nextest, wave0, nyquist, russh-precursor]

requires:
  - phase: 08-git-https-pats
    provides: Wave 0 RED stub pattern (assert!(false)); dialect_pats / smoke-git-https discoverability
  - phase: 07-git-repos-browse
    provides: GitBackend bare repos + owner-only ACL helpers SSH will reuse
provides:
  - "Wave 0 RED ssh_key_* RPC stubs (add/list/revoke, email_unverified, title_required, fingerprint, max 25)"
  - "Wave 0 RED git_ssh* transport stubs (user git, upload-pack, ACL stderr, receive-pack verify, pack allowlist, rate-limit)"
  - "Wave 0 dialect_ssh_keys stub for 0009_ssh_keys + ssh_public_keys UNIQUE fingerprint"
  - "Wave 0 smoke-git-ssh.sh + make smoke-git-ssh (D-SSH-07)"
affects: [09-02-ssh-keys-schema, 09-03-ssh-key-rpc, 09-04-russh-listener, 09-05-compose-smoke]

actuals:
  tokens: 2822
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Wave 0 intentional RED stubs with assert!(false) until SSH keys / russh land"
    - "ssh_key / git_ssh / dialect_ssh_keys nextest filters mirror VALIDATION.md Wave 0 checklist"
    - "smoke-git-ssh docker-missing skip exit 0; Wave 0 exit 1 when Docker present until 09-05"

key-files:
  created:
    - crates/oxidean-api/tests/ssh_key_rpc.rs
    - crates/oxidean-api/tests/git_ssh.rs
    - crates/oxidean-db/tests/dialect_ssh_keys.rs
    - scripts/smoke-git-ssh.sh
  modified:
    - Makefile

key-decisions:
  - "Wave 0 is RED-only — no russh listener, 0009 migrations, or sshKey RPC handlers"
  - "Migration number locked to 0009_ssh_keys (Phase 10 owns 0010)"
  - "Smoke stub exits 1 when Docker present with Wave 0 message until 09-05 greens TCP 2222"

patterns-established:
  - "Nyquist Wave 0 for Phase 9: failing nextest + Make smoke paths exist before SSH implementation waves"

requirements-completed: []  # Wave 0 scaffolds only; greens land in later 09-xx plans

coverage:
  - id: D1
    description: "API Wave 0 stubs for sshKey.add/list/revoke, email_unverified, title_required, fingerprint unique, max 25"
    requirement: GIT-04
    verification:
      - kind: unit
        ref: "cargo nextest list -p oxidean-api -E 'test(ssh_key)'"
        status: pass
    human_judgment: false
  - id: D2
    description: "API Wave 0 stubs for git-over-SSH user git, pack ACL, unverified push, shell reject, rate-limit"
    requirement: GIT-03
    verification:
      - kind: unit
        ref: "cargo nextest list -p oxidean-api -E 'test(git_ssh)'"
        status: pass
    human_judgment: false
  - id: D3
    description: "DB Wave 0 dialect_ssh_keys stub expecting tri-dialect 0009_ssh_keys + UNIQUE fingerprint"
    requirement: GIT-04
    verification:
      - kind: unit
        ref: "cargo nextest list -p oxidean-db -E 'test(dialect_ssh_keys)'"
        status: pass
    human_judgment: false
  - id: D4
    description: "Compose smoke-git-ssh stub + Makefile target discoverable (D-SSH-07)"
    requirement: GIT-03
    verification:
      - kind: other
        ref: "test -x scripts/smoke-git-ssh.sh && rg -n smoke-git-ssh Makefile"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-13
status: complete
plan_head_before: f86dcf2eb936ea61cb69874a0d349d70a4341160
commits: 2
---

# Phase 09 Plan 00: Wave 0 SSH Key + Git SSH Nyquist Stubs Summary

**Failing nextest + smoke stubs for sshKey RPC, git-over-SSH auth/ACL, 0009_ssh_keys dialect, and make smoke-git-ssh before russh/migration plans turn them green.**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-13T23:29:05Z
- **Completed:** 2026-09-13T23:33:59Z
- **Tasks:** 2/2
- **Files modified:** 5

## Accomplishments

- SSH key RPC Wave 0 stubs cover verified `sshKey.add` (ed25519 + SHA256 fingerprint, no secret reveal), list after add, revoke, `auth.email_unverified`, `sshKey.title_required`, duplicate fingerprint, and max 25 keys (GIT-04 / D-SSH-05 / T-09-02)
- Git SSH Wave 0 stubs cover force user `git`, registered-key accept, public `git-upload-pack`, private non-owner git stderr deny, unverified `git-receive-pack`, non-pack shell reject, and failed-pubkey rate-limit (GIT-03 / D-SSH-03..04 / D-SSH-07 / T-09-01)
- Dialect stub expects tri-dialect `0009_ssh_keys` with `ssh_public_keys` + UNIQUE fingerprint columns
- `scripts/smoke-git-ssh.sh` + `make smoke-git-ssh` discoverable; docker-missing skips exit 0

## Task Commits

Each task was committed atomically:

1. **Task 1: SSH key RPC + git_ssh + dialect Wave 0 stubs** - `5e82976` (test)
2. **Task 2: smoke-git-ssh stub + Makefile target** - `3d754fc` (test)

_Note: Wave 0 is RED-only by design — GREEN belongs to later 09-xx plans._

## Files Created/Modified

- `crates/oxidean-api/tests/ssh_key_rpc.rs` - GIT-04 Wave 0 `assert!(false)` stubs for sshKey.*
- `crates/oxidean-api/tests/git_ssh.rs` - GIT-03 Wave 0 stubs for SSH transport auth/ACL/pack
- `crates/oxidean-db/tests/dialect_ssh_keys.rs` - 0009_ssh_keys tri-dialect presence stub
- `scripts/smoke-git-ssh.sh` - Compose SSH smoke stub (D-SSH-02 / D-SSH-07)
- `Makefile` - `.PHONY` / help / `smoke-git-ssh` target

## Decisions Made

- Kept Wave 0 RED-only (`assert!(false)`); no production russh listener, migrations, or RPC handlers
- Migration id **0009_ssh_keys** (not 0010 — Phase 10 owns 0010)
- Smoke stub fails with Wave 0 message when Docker is present until 09-05 greens TCP 2222 ls-remote/push

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

Wave 0 intentional RED stubs (expected until later plans):

| File | Stub | Reason |
|------|------|--------|
| `crates/oxidean-api/tests/ssh_key_rpc.rs` | all 7 `assert!(false)` tests | RED until 09-03 sshKey RPC |
| `crates/oxidean-api/tests/git_ssh.rs` | all 7 `assert!(false)` tests | RED until 09-04 / 09-05 russh |
| `crates/oxidean-db/tests/dialect_ssh_keys.rs` | schema + tri-dialect asserts (fail until migration) | RED until 09-02 `0009_ssh_keys` |
| `scripts/smoke-git-ssh.sh` | exit 1 Wave 0 when Docker present | RED until 09-05 Compose TCP smoke |

## Threat Flags

None — threat surface encoded only as stub assertions (T-09-01, T-09-02); no new network endpoints or crates.io packages (T-09-SC).

## Self-Check: PASSED

- FOUND: crates/oxidean-api/tests/ssh_key_rpc.rs
- FOUND: crates/oxidean-api/tests/git_ssh.rs
- FOUND: crates/oxidean-db/tests/dialect_ssh_keys.rs
- FOUND: scripts/smoke-git-ssh.sh
- FOUND: 5e82976
- FOUND: 3d754fc
