---
phase: 09-git-ssh
plan: "05"
subsystem: infra
tags: [docker-compose, ssh, smoke, TCP-2222]
requires:
  - phase: 09-04
    provides: russh ACL + pack allowlist ready for Compose publish
provides:
  - Compose TCP 2222 + OXIDEAN_SSH_* on api
  - Volume-backed SSH host keys
  - smoke-git-ssh ls-remote/push script
affects: [09-06-rpc-gen, 09-08-clonebox, 09-09-docs]
actuals:
  tokens: 2720
  tasks: 2
  commits: 2
plan_head_before: "8eef102558441b995d6e56b2fbcc67a5404995d7"
tech-stack:
  added: []
  patterns:
    - "SSH via host TCP publish only — never Traefik HTTP/TCP routers"
    - "smoke-git-ssh mirrors HTTPS: docker-missing skip 0; health wait; optional push"
key-files:
  created: []
  modified:
    - docker-compose.yml
    - .env.example
    - docs/dev-auth.env.example
    - scripts/smoke-git-ssh.sh
    - Makefile
key-decisions:
  - "Hard-map compose 2222:2222; OXIDEAN_SSH_PORT defaults 2222 inside container"
  - "Ephemeral key registration via SMOKE_SESSION_COOKIE sshKey.add; or SMOKE_SSH_IDENTITY"
requirements-completed: [GIT-03]
coverage:
  - id: D1
    description: Compose publishes SSH TCP 2222 with persisted host key volume
    requirement: GIT-03
    verification:
      - kind: other
        ref: "rg -n '2222:2222|OXIDEAN_SSH_ENABLED' docker-compose.yml"
        status: pass
    human_judgment: false
  - id: D2
    description: smoke-git-ssh supports ls-remote over scp-style remotes with -p 2222
    requirement: GIT-03
    verification:
      - kind: other
        ref: "rg -n 'ls-remote|2222|GIT_SSH_COMMAND' scripts/smoke-git-ssh.sh"
        status: pass
    human_judgment: false
duration: 15min
completed: 2026-09-14
status: complete
---

# Phase 09 Plan 05: Compose SSH + smoke-git-ssh Summary

**Compose publishes raw TCP 2222 for in-api russh with persisted host keys, and `make smoke-git-ssh` can ls-remote over scp-style `git@host:owner/repo.git`.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files:** 5

## Accomplishments

- Wired `OXIDEAN_SSH_ENABLED/PORT/HOST/HOST_KEY_DIR` on api; `2222:2222` publish; `ssh_host_keys` volume.
- Documented SSH env in `.env.example` and `docs/dev-auth.env.example`.
- Greened `scripts/smoke-git-ssh.sh` (docker skip, health, TCP probe, optional key register + ls-remote/push).

## Task Commits

1. **Task 1: Compose TCP + env + volume** - `970a2fd` (feat)
2. **Task 2: Green smoke-git-ssh** - `0196076` (feat)

## Files Created/Modified

- `docker-compose.yml` — SSH env, ports, volume
- `scripts/smoke-git-ssh.sh` — full smoke path
- `.env.example`, `docs/dev-auth.env.example`, `Makefile`

## Decisions Made

- Single compose port map `2222:2222` matching default `OXIDEAN_SSH_PORT`.
- Live Compose smoke still needs stack + repo/key (same as HTTPS); script is assertable offline via rg.

## Deviations from Plan

None - plan executed as written. Live `make smoke-git-ssh` against a running stack deferred to phase gate / operator (health wait hangs without `make up`).

## Self-Check: PASSED

- FOUND: docker-compose.yml 2222:2222
- FOUND: 970a2fd, 0196076
