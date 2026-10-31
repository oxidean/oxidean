---
phase: 09-git-ssh
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Passed with documented caveats (see report body). — Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 2/2 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/09-git-ssh/09-00-PLAN.md
  - .planning/phases/09-git-ssh/09-00-SUMMARY.md
  - .planning/phases/09-git-ssh/09-01-PLAN.md
  - .planning/phases/09-git-ssh/09-01-SUMMARY.md
  - .planning/phases/09-git-ssh/09-02-PLAN.md
  - .planning/phases/09-git-ssh/09-02-SUMMARY.md
  - .planning/phases/09-git-ssh/09-03-PLAN.md
  - .planning/phases/09-git-ssh/09-03-SUMMARY.md
  - .planning/phases/09-git-ssh/09-04-PLAN.md
  - .planning/phases/09-git-ssh/09-04-SUMMARY.md
  - .planning/phases/09-git-ssh/09-05-PLAN.md
  - .planning/phases/09-git-ssh/09-05-SUMMARY.md
  - .planning/phases/09-git-ssh/09-06-PLAN.md
  - .planning/phases/09-git-ssh/09-06-SUMMARY.md
  - .planning/phases/09-git-ssh/09-07-PLAN.md
  - .planning/phases/09-git-ssh/09-07-SUMMARY.md
  - .planning/phases/09-git-ssh/09-08-PLAN.md
  - .planning/phases/09-git-ssh/09-08-SUMMARY.md
  - .planning/phases/09-git-ssh/09-09-PLAN.md
  - .planning/phases/09-git-ssh/09-09-SUMMARY.md
  - .planning/phases/09-git-ssh/09-CONTEXT.md
  - .planning/phases/09-git-ssh/09-VALIDATION.md
  - apps/web/src/components/repo/clone-box.ssh.integration.test.ts
  - apps/web/src/components/repo/clone-box.tsrx
  - apps/web/src/components/repo/ssh-how-to.tsrx
  - apps/web/src/components/settings/settings-nav.tsrx
  - apps/web/src/components/settings/ssh-key-add-form.tsrx
  - apps/web/src/components/settings/ssh-key-list.tsrx
  - apps/web/src/components/settings/ssh-key-revoke-dialog.tsrx
  - apps/web/src/lib/public-origin.ts
  - apps/web/src/routes/settings/ssh-keys.integration.test.ts
  - apps/web/src/routes/settings/ssh-keys.tsrx
  - crates/oxidean-api/src/ssh/mod.rs
  - crates/oxidean-api/src/ssh/server.rs
  - crates/oxidean-api/src/ssh_keys/mod.rs
  - crates/oxidean-api/tests/git_ssh.rs
  - crates/oxidean-api/tests/ssh_key_rpc.rs
  - crates/oxidean-core/src/ssh_key_types.rs
  - crates/oxidean-db/migrations/mysql/0009_ssh_keys.sql
  - crates/oxidean-db/migrations/postgres/0009_ssh_keys.sql
  - crates/oxidean-db/migrations/sqlite/0009_ssh_keys.sql
  - crates/oxidean-db/src/ssh_keys.rs
  - crates/oxidean-db/tests/dialect_ssh_keys.rs
  - docs/ARCHITECTURE.md
  - docs/CONFIGURATION.md
  - packages/api-client/src/index.ts
  - scripts/smoke-git-ssh.sh
covered_digest: "v1:sha256:757e24c4ddd1568fb4645a694dad87f6b70205cded6274295d02a23717fbb57a"
behavior_unverified: 0
overrides_applied: 0
decision_coverage: "{'honored': 7, 'total': 7, 'not_honored': []}"
nyquist_complete: true
caveats: "['clonebox_live_ls_remote_optional_client_depth', 'smoke_git_ssh_ci_routing_tcp_default_skip_ls_remote']"
human_verification: "[{'test': 'Register ed25519 key at /settings/ssh-keys; CloneBox shows git@…:owner/repo.git', 'expected': 'Add/list/revoke works in a live browser; SSH URL + Port hint match env', 'why_human': 'Stack-browser covers add/list (11.1-04); revoke UX + CloneBox visual polish remain human-friendly'}, {'test': 'make up then make smoke-git-ssh (ls-remote/push on TCP 2222) with fixtures', 'expected': 'Compose SSH listener accepts registered key; push/fetch succeed', 'why_human': 'CI smoke-protocol asserts TCP fail-closed; full ls-remote/push needs seeded key+repo (SMOKE_SKIP_LS_REMOTE=0)'}]"
---

# Phase 09: Git SSH Verification Report

**Phase Goal:** Users can register SSH keys and clone/fetch/push over SSH like a normal forge remote  
**Verified:** 2026-09-15T15:39:42Z  
**Status:** passed_with_caveats  
**Re-verification:** Yes — honesty repair (missing VERIFICATION + draft VALIDATION; Phase 11.1 / issue #3); residual 11.1-05 Nyquist flip after stack-browser SSH + CI smoke-protocol  
**Plans:** 10/10 PLAN files have matching SUMMARY files (09-00 … 09-09).

## Goal Achievement

### Observable Truths

Roadmap success criteria (GIT-03, GIT-04). Evidence is artifact + named automated tests from phase SUMMARYs / gate notes — not a fresh full-suite re-run in this honesty pass.

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can add, list, and revoke SSH public keys on their account (GIT-04) | ✓ VERIFIED | Schema `0009_ssh_keys` tri-dialect + `ssh_keys.rs`; RPC `sshKey.add/list/revoke` in `ssh_keys/mod.rs` + generated client; UI `/settings/ssh-keys` + SettingsNav. Tests: `ssh_key_rpc.rs` (7 cases); `dialect_ssh_keys` (2); Vitest `ssh-keys.integration.test.ts`; stack-browser `forge-packages-ssh-orgs.stack.browser.test.tsx` (11.1-04 add/list). |
| 2 | User can clone, fetch, and push over SSH with a registered public key (GIT-03) | ✓ VERIFIED | In-process `russh` listener `crates/oxidean-api/src/ssh/`; CloneBox scp-style URL + CTA. Tests: `git_ssh.rs`; Vitest `clone-box.ssh.integration.test.ts`; `scripts/smoke-git-ssh.sh` + CI `smoke-protocol` / `make smoke-protocol-ci` (11.1-05; fail-closed TCP; optional ls-remote with fixtures). |

**Score:** 2/2 truths verified by API/DB/Vitest + stack-browser SSH keys + CI protocol smoke. **Caveat:** full client ls-remote/push in CI defaults to skipped (`SMOKE_SKIP_LS_REMOTE=1`) — TCP + routing are CI-gated; seeded-repo client remains ops/optional.

### Decision Coverage

CONTEXT decisions D-SSH-01…07 are reflected in shipped code/docs (russh in-api, scp-style URL + port 2222, force user `git`, ACL/verify parity, key caps + fingerprint, settings + CloneBox, TCP not Traefik + smoke target). Non-blocking.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/oxidean-db/migrations/*/0009_ssh_keys.sql` | Tri-dialect SSH keys schema | ✓ VERIFIED | postgres/mysql/sqlite present |
| `crates/oxidean-db/src/ssh_keys.rs` | Key CRUD helpers | ✓ VERIFIED | Used by RPC + dialect tests |
| `crates/oxidean-core/src/ssh_key_types.rs` | DTOs | ✓ VERIFIED | Exported from core |
| `crates/oxidean-api/src/ssh_keys/mod.rs` | `sshKey.*` RPC | ✓ VERIFIED | add/list/revoke; require_verified; max 25 |
| `crates/oxidean-api/src/ssh/` | russh listener + pack | ✓ VERIFIED | auth, host_keys, pack, rate_limit, server |
| `crates/oxidean-api/tests/ssh_key_rpc.rs` | GIT-04 behavioral | ✓ VERIFIED | 7 greened tests (09-09 gate: ssh_key\|git_ssh = 14 passed) |
| `crates/oxidean-api/tests/git_ssh.rs` | GIT-03 behavioral | ✓ VERIFIED | 7 greened tests |
| `crates/oxidean-db/tests/dialect_ssh_keys.rs` | Migration presence | ✓ VERIFIED | schema + tri-dialect files |
| `apps/web/.../ssh-keys.tsrx` + components | Settings UI | ✓ VERIFIED | list/add/revoke + nav |
| `apps/web/.../clone-box.tsrx` + `ssh-how-to.tsrx` | Live SSH URL | ✓ VERIFIED | placeholder replaced |
| `packages/api-client/src/index.ts` | Generated `sshKey.*` | ✓ VERIFIED | client + Query helpers |
| `scripts/smoke-git-ssh.sh` | Compose TCP smoke | ✓ VERIFIED | Makefile target; docker-missing skip |
| `docs/CONFIGURATION.md` / `ARCHITECTURE.md` | Ops + russh notes | ✓ VERIFIED | `OXIDEAN_SSH_*` documented |
| `09-VALIDATION.md` | Phase gate map | ✓ validated | Wave 0 complete; **`nyquist_compliant: true`** (11.1-04 e2e + 11.1-05 CI smoke) |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `ssh-keys.tsrx` | `sshKey.*` RPC | api-client Query/mutations | ✓ WIRED | list/add/revoke |
| `ssh_keys/mod.rs` | `ssh_keys.rs` DB | ctx.db helpers | ✓ WIRED | fingerprint unique + caps |
| `ssh/server.rs` | pack + ACL | upload/receive-pack spawn | ✓ WIRED | git user only; no shell |
| `clone-box.tsrx` | `public-origin` SSH helpers | `sshCloneUrl` | ✓ WIRED | scp-style + Port hint |
| Compose / Makefile | TCP 2222 | `smoke-git-ssh` | ✓ WIRED | not Traefik |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| SSH keys settings | `sshKey.list` | RPC → `ssh_public_keys` | Yes | ✓ FLOWING |
| Add key | form → `sshKey.add` | RPC → fingerprint store | Yes | ✓ FLOWING |
| CloneBox SSH panel | `sshCloneUrl(...)` | env / public origin | Yes (advertised URL) | ✓ FLOWING |
| Git-over-SSH | registered pubkey → account | russh auth → pack | Yes (integration tests) | ✓ FLOWING |

### Behavioral Spot-Checks

Evidence from **09-09-SUMMARY** phase gate (2026-09-14), cross-checked that test files still exist and contain no RED stubs:

| Behavior | Command (gate) | Result (SUMMARY) | Status |
| -------- | -------------- | ---------------- | ------ |
| sshKey + git_ssh | `cargo nextest … 'test(ssh_key)\|test(git_ssh)'` | 14 passed | ✓ PASS (recorded) |
| dialect | `cargo test -p oxidean-db --test dialect_ssh_keys` | 2 passed | ✓ PASS (recorded) |
| Vitest SSH UI | ssh-keys + clone-box.ssh | 9 passed | ✓ PASS (recorded) |
| RPC sync | `make rpc-sync-check` | ok | ✓ PASS (recorded) |
| Web build | `bun run build` (apps/web) | ok | ✓ PASS (recorded) |
| Compose smoke | `make smoke-git-ssh` | operator/UAT when stack up | ⚠ NOT RE-RUN here |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| Compose SSH smoke | `make smoke-git-ssh` | Script present; live run needs Docker stack | SKIP (honesty pass) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| GIT-04 | 00,01,02,03,06,07,09 | add/list/revoke keys; caps; verified | ✓ SATISFIED | RPC + UI + dialect + Vitest |
| GIT-03 | 00,01,03,04,05,08,09 | clone/fetch/push over SSH | ✓ SATISFIED | russh + git_ssh tests + CloneBox + smoke script |

No orphaned REQUIREMENTS.md IDs for Phase 09 (GIT-03/04).

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `ssh_key_rpc.rs` | GIT-04 | 7 | 0 | 0 | Behavioral | OK |
| `git_ssh.rs` | GIT-03 | 7 | 0 | 0 | Behavioral | OK |
| `dialect_ssh_keys.rs` | GIT-04 | 2 | 0 | 0 | Schema/value | OK |
| `ssh-keys.integration.test.ts` | GIT-04 UI | yes | 0 | 0 | DOM/integration | OK |
| `clone-box.ssh.integration.test.ts` | GIT-03/04 UI | yes | 0 | 0 | DOM/integration | OK |
| `apps/web/e2e` SSH | GIT-03/04 | **yes** | — | — | stack-browser | OK (11.1-04) |
| `smoke-git-ssh.sh` | GIT-03 | script | CI fail-closed | — | Live stack / CI | OK (11.1-05 `smoke-protocol`) |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Nyquist completeness:** **true** — stack-browser SSH keys (11.1-04) + CI fail-closed `smoke-git-ssh` (11.1-05). Optional client ls-remote/push remains fixture-gated.

### Anti-Patterns Found

| File | Pattern | Severity | Impact |
| ---- | ------- | -------- | ------ |
| — | No remaining Wave 0 `assert!(false)` in ssh_key/git_ssh/dialect tests | — | Clean |
| `apps/web/e2e` | SSH keys covered by `forge-packages-ssh-orgs.stack.browser.test.tsx` | ℹ️ Closed | 11.1-04 |
| `09-VALIDATION.md` (pre-fix) | `status: draft` while plans complete | ℹ️ Fixed | Honesty update this pass |

### Human Verification Required

### 1. Settings SSH keys in a real browser

**Test:** Register ed25519 key; list; revoke with confirm; verify wall when unverified  
**Expected:** CRUD matches Vitest/RPC behavior in live session  
**Why human:** Stack-browser covers add/list; revoke confirm + unverified wall remain useful human checks

### 2. Compose TCP git smoke (full client)

**Test:** `make up` then `make smoke-git-ssh` with `SMOKE_SKIP_LS_REMOTE=0` + registered key  
**Expected:** ls-remote/push with registered key on port 2222  
**Why human / ops:** CI asserts TCP fail-closed by default; full client needs fixtures

### Gaps Summary

**Phase goal is met in code, unit/integration, stack-browser SSH keys, and CI protocol smoke.** Residual (non-blocking):

1. **CloneBox ↔ live git client** optional depth — CI asserts SSH TCP; seeded ls-remote/push when fixtures present.
2. **`SMOKE_SKIP_LS_REMOTE=1` default in CI** — intentional (no seeded smokeowner/smokerepo in protocol job); do not treat as docker-missing skip-as-pass (that path fails closed under `CI` / `SMOKE_REQUIRE_STACK`).
3. **`nyquist_compliant` is true** in `09-VALIDATION.md` after 11.1-04 + 11.1-05 evidence.

**Non-blocking:** Phase 10+ ACL extensions for collaborators on SSH are intentional post-09 scope (CONTEXT); SSH must continue to call the shared ACL module.

---

_Verified: 2026-09-15T15:39:42Z_  
_Verifier: Claude (gsd-honesty / Phase 11.1)_  
_Residual Nyquist closeout: 2026-09-15 (11.1-05 — stack-browser SSH + CI smoke-protocol)_

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 49 files

