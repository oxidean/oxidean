---
phase: "09"
slug: "git-ssh"
# Lifecycle (#2117): draft → validated. PARTIAL = validated + nyquist_compliant: false
# Wave 0 + API/Vitest + Phase 11.1 stack-browser SSH + CI smoke-protocol closed Nyquist sampling.
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-09-15"
verified_at: "2026-09-15"
---

# Phase 09 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `09-RESEARCH.md` Validation Architecture; refreshed after 09-00…09-09.
> **Honesty (2026-09-15 residual / 11.1-05):** Wave 0 + API/Vitest greened earlier; **`nyquist_compliant: true`** after Phase 11.1-04 stack-browser SSH keys (`forge-packages-ssh-orgs.stack.browser.test.tsx`) and Phase 11.1-05 CI `smoke-protocol` (`make smoke-git-ssh`, fail-closed). See `09-VERIFICATION.md`. CloneBox→live ls-remote remains smoke/ops depth (routing+TCP asserted in CI; full client needs seeded key+repo).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) + stack-browser Playwright + Compose smoke |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(ssh_key) \| test(git_ssh)'` + `cargo test -p oxidean-db --test dialect_ssh_keys` + `bunx vitest run src/routes/settings/ssh-keys.integration.test.ts src/components/repo/clone-box.ssh.integration.test.ts` (cwd `apps/web`) |
| **Full suite command** | `make test` + `make test-e2e-stack` + CI `smoke-protocol` |
| **Estimated runtime** | ~60–180 seconds targeted; full suite longer |
| **Missing for Nyquist** | *(cleared)* prior gaps: stack-browser SSH keys + CI-enforced `smoke-git-ssh` — closed in 11.1-04 / 11.1-05 |

---

## Sampling Rate

- **Per task commit:** targeted nextest filter + relevant vitest file
- **Per wave merge:** `make test`
- **Phase gate (automated):** nextest ssh filters + dialect_ssh_keys + `make rpc-sync-check` + vitest ssh-keys + clone-box.ssh + `bun run build` (apps/web) — greened in 09-09
- **Phase gate (browser):** `make test-e2e-stack` → `forge-packages-ssh-orgs.stack.browser.test.tsx` (SSH keys add/list)
- **Phase gate (protocol CI):** `smoke-protocol` job → `make smoke-protocol-ci` includes `smoke-git-ssh` (TCP; fail-closed; optional ls-remote when fixtures present)

---

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GIT-04 | add/list/revoke keys; max 25; require_verified; unique fingerprint | integration | `cargo nextest run -p oxidean-api -E 'test(ssh_key)'` | ✅ yes |
| GIT-04 | dialect `0009_ssh_keys` CRUD | integration | `cargo test -p oxidean-db --test dialect_ssh_keys` | ✅ yes |
| GIT-04 | `/settings/ssh-keys` list + confirm revoke | integration (Vitest DOM) | `bunx vitest run src/routes/settings/ssh-keys.integration.test.ts` | ✅ yes |
| GIT-04 | stack-browser add/list SSH keys | e2e / browser | `make test-e2e-stack` (`forge-packages-ssh-orgs.stack.browser.test.tsx`) | ✅ yes (11.1-04) |
| GIT-03 | force user `git`; reject other usernames | integration | `cargo nextest run -p oxidean-api -E 'test(git_ssh)'` | ✅ yes |
| GIT-03 | public fetch with key; private non-owner denied (git stderr) | integration | same `git_ssh` filter | ✅ yes |
| GIT-03 | push requires verified email | integration | same `git_ssh` filter | ✅ yes |
| GIT-03 | only upload/receive-pack; reject shell | integration | same `git_ssh` filter | ✅ yes |
| GIT-03 | failed pubkey rate limit | integration | same `git_ssh` filter | ✅ yes |
| GIT-03 | Compose TCP (+ optional ls-remote/push) | smoke / CI | `make smoke-git-ssh` via CI `smoke-protocol` | ✅ yes — CI fail-closed (11.1-05); client ls-remote needs fixtures |
| GIT-03/04 | CloneBox SSH URL + CTA | integration (Vitest DOM) | `bunx vitest run src/components/repo/clone-box.ssh.integration.test.ts` | ✅ yes |
| GIT-03/04 | browser CloneBox SSH + real git client | e2e / smoke | CI TCP + ops smoke with seeded key | ⚠ UI URL Vitest; live client = smoke/ops |

---

## Wave 0 Gaps

- [x] `crates/oxidean-api/tests/git_ssh.rs` — greened (auth, ACL, pack allowlist, rate-limit)
- [x] `crates/oxidean-db` dialect tests for `0009_ssh_keys`
- [x] `apps/web/src/routes/settings/ssh-keys.integration.test.ts` — greened
- [x] `scripts/smoke-git-ssh.sh` + Makefile target
- [x] `clone-box.integration.test.ts` / `clone-box.ssh.integration.test.ts` — live SSH panel
- [x] Stack-browser SSH keys add/list — `forge-packages-ssh-orgs.stack.browser.test.tsx` (11.1-04)
- [x] CI job runs `make smoke-git-ssh` fail-closed — `smoke-protocol` (11.1-05)

### Residual (non-blocking; Nyquist sampling closed)

- CloneBox advertised SSH URL ↔ working `git ls-remote` remains optional client depth (CI asserts TCP 2222; full ls-remote/push when `SMOKE_SKIP_LS_REMOTE=0` + registered key + public repo)
- Revoke dialog in stack-browser not required for Nyquist after add/list coverage

---

## Manual / UAT Backstops

- Register ed25519 key in settings; CloneBox shows `git@…:owner/repo.git`
- `ssh -p 2222` / git ls-remote against Compose stack (`make up` then `make smoke-git-ssh` with fixtures)
- Unverified user cannot push; private non-owner denied

## Phase gate notes (09-09 + 11.1 residual)

Automated gate: nextest ssh filters, dialect_ssh_keys, `make rpc-sync-check`, vitest ssh-keys + clone-box.ssh, `bun run build` (apps/web). Browser sampling: stack-browser SSH keys (11.1-04). Protocol sampling: CI `smoke-protocol` / `make smoke-protocol-ci` (11.1-05).

## Nyquist honesty

| Field | Value | Why |
|-------|-------|-----|
| `status` | `validated` | Plans executed; Wave 0 closed; req→test map accurate; VERIFICATION written |
| `nyquist_compliant` | `true` | Stack-browser SSH keys (11.1-04) + CI fail-closed `smoke-git-ssh` (11.1-05) close prior sampling gaps |
| `wave_0_complete` | `true` | RED stubs greened; infrastructure checklist done |

Prefer residual-risk documentation for optional client ls-remote depth — do not re-open a false red after e2e+CI evidence landed.
