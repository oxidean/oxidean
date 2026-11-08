---
phase: 09-git-ssh
plan: "08"
subsystem: ui
tags: [clone-box, ssh, scp-style, public-origin]
requires:
  - phase: 09-07
    provides: /settings/ssh-keys for add-key CTA
provides:
  - sshCloneUrl + Port hint helpers
  - CloneBox live SSH panel + SshHowTo
affects: [09-09-docs]
actuals:
  tokens: 4200
  tasks: 2
  commits: 1
plan_head_before: "7954b4d7510db1d5db948a07e7fcabff27f6c85d"
tech-stack:
  added: []
  patterns:
    - "scp-style primary URL; Port via ~/.ssh/config when OXIDEAN_SSH_PORT ≠ 22"
key-files:
  created:
    - apps/web/src/components/repo/ssh-how-to.tsrx
  modified:
    - apps/web/src/lib/public-origin.ts
    - apps/web/src/lib/public-origin.unit.test.ts
    - apps/web/src/components/repo/clone-box.tsrx
    - apps/web/src/components/repo/clone-box.integration.test.ts
key-decisions:
  - "Default advertised SSH port 2222; host from OXIDEAN_SSH_HOST or public origin hostname"
requirements-completed: [GIT-03, GIT-04]
coverage:
  - id: D1
    description: CloneBox scp-style SSH URL + Port hint + Add an SSH key CTA
    requirement: GIT-03
    verification:
      - kind: automated_ui
        ref: bunx vitest run src/components/repo/clone-box.ssh.integration.test.ts
        status: pass
    human_judgment: false
duration: 15min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 08: CloneBox SSH Summary

**CloneBox now shows a copyable scp-style `git@host:owner/repo.git`, a Port hint when ≠ 22, and an Add an SSH key CTA — Phase 7/8 placeholder removed.**

## Accomplishments

- Added `sshCloneUrl` / `sshNeedsPortHint` / `resolveSshHost` / `resolveSshPort`.
- Live SSH panel + `SshHowTo` documenting login user `git`.

## Task Commits

1. **Task 1+2: helpers + CloneBox** - `07b828b` (feat)

## Deviations from Plan

Updated `clone-box.integration.test.ts` to expect Clone with SSH instead of placeholder (Rule 1).

## Self-Check: PASSED

- FOUND: sshCloneUrl
- FOUND: 07b828b
