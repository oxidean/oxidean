---
phase: "13"
slug: "branch-protection"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 13: Branch Protection — Validation

**Nyquist / Wave 0:** Discoverable RED stubs before implementation plans turn green.
Retroactively reconciled to the standard contract on 2026-10-02 (ROADMAP DEBT-08); original checklist content preserved below.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) + Compose smoke (`make smoke-protection`) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(branch_protect) \| test(commit_status)'` + `cargo test -p oxidean-db --test dialect_branch_protection` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted; Compose protection smoke longer |

---

## Sampling Rate

- **Per task commit:** focused nextest filter + relevant Vitest file
- **Per wave merge:** `make test` + `make rpc-sync-check` after RPC/client changes
- **Phase gate:** Full automated gate green before `/gsd-verify-work`; `make smoke-protection` when Docker available (packaged denial added by Phase 22.1)

---

## Requirements checklist

| ID | Wave 0 stub surfaces |
|----|----------------------|
| ORG-05 | `branch_protection_rpc` CRUD stubs; Settings branches Vitest stub |
| ORG-06 | `branch_protect_push` hook deny stub; dialect migration stub |
| PR-08 | `branch_protect_merge` merge-blocked stub; PR blockers Vitest stub |

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 13-00-T1 | 00 | 0 | ORG-05, ORG-06, PR-08 | T-13-01 | RED stubs discoverable for protection + statuses | stubs | `cargo nextest list -p oxidean-api -E 'test(branch_protect) \| test(commit_status)'` | ✅ | ✅ green |
| 13-00-T2 | 00 | 0 | ORG-06 | — | dialect migration parity stub + VALIDATION refresh | integration | `test -f crates/oxidean-db/tests/dialect_branch_protection.rs` | ✅ | ✅ green |
| 13-01-T1 | 01 | 0 | ORG-05 | T-13-01 | Settings branches Vitest stub | component | `test -f apps/web/src/routes/$owner.$repo.settings.branches.integration.test.ts` | ✅ | ✅ green |
| 13-01-T2 | 01 | 0 | PR-08 | — | PR protection blockers Vitest stub | component | `test -f apps/web/src/routes/$owner.$repo.pull.protection.integration.test.ts` | ✅ | ✅ green |
| 13-02-T1 | 02 | 1 | ORG-06, PR-08 | T-13-03, T-13-04 | End-to-end protect main — push deny + merge gate | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_push) \| test(branch_protect_merge)'` + `dialect_branch_protection` | ✅ | ✅ green |
| 13-02-T2 | 02 | 1 | ORG-06 | T-13-03 | Soft-protect still applies beside rules | integration | `cargo nextest run -p oxidean-api -E 'test(repo_branch_soft_protect)'` | ✅ | ✅ green |
| 13-03-T1 | 03 | 2 | ORG-05 | T-13-05 | Pattern matcher + multi-rule union | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect)'` | ✅ | ✅ green |
| 13-03-T2 | 03 | 2 | ORG-05 | T-13-02, T-13-06 | Full branchProtection CRUD + rpc-gen | integration/codegen | `cargo nextest run -p oxidean-api -E 'test(branch_protection_rpc)'` + `make rpc-sync-check` | ✅ | ✅ green |
| 13-04-T1 | 04 | 3 | PR-08 | T-13-07, T-13-08 | commit_statuses table + repo.commitStatus RPC | integration | `cargo nextest run -p oxidean-api -E 'test(commit_status)'` + `make rpc-sync-check` | ✅ | ✅ green |
| 13-04-T2 | 04 | 3 | PR-08 | T-13-09 | Required contexts + strict in merge evaluate | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_merge)'` | ✅ | ✅ green |
| 13-05-T1 | 05 | 4 | ORG-06 | T-13-12 | Force-push, delete, lock_branch push intents | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_push) \| test(repo_branch_soft_protect)'` | ✅ | ✅ green |
| 13-05-T2 | 05 | 4 | ORG-06 | T-13-10, T-13-11 | Hook install on create + reconcile existing | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_push)'` | ✅ | ✅ green |
| 13-06-T1 | 06 | 5 | PR-08 | T-13-14, T-13-15 | Review extras on merge path | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_merge)'` | ✅ | ✅ green |
| 13-06-T2 | 06 | 5 | ORG-05, PR-08 | T-13-13 | enforce_admins + linear history | integration | `cargo nextest run -p oxidean-api -E 'test(branch_protect_merge) \| test(branch_protect_push)'` | ✅ | ✅ green |
| 13-07-T1 | 07 | 6 | ORG-05 | — | Branch protection settings panel | component/build | `make web-lint && make web-format-check` | ✅ | ✅ green |
| 13-07-T2 | 07 | 6 | ORG-05 | — | Settings branches Vitest green | component | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.settings.branches.integration.test.ts` | ✅ | ✅ green |
| 13-08-T1 | 08 | 7 | PR-08 | — | PR merge blockers UI | component | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.pull.protection.integration.test.ts` + web lint/format | ✅ | ✅ green |
| 13-08-T2 | 08 | 7 | ORG-05/06, PR-08 | — | API + architecture docs | docs | `rg -n "branchProtection\|commitStatus\|branch_protection\|merge_blocked" docs/API.md docs/ARCHITECTURE.md` | ✅ | ✅ green |
| 22.1-03/04 (post-close) | — | — | ORG-06 (packaging) | T-22.1-01 | Helper binary in API image; Compose `OXIDEAN_PROTECTION_HELPER`; boot sweep overwrites hooks | smoke/integration | `make smoke-protection`; `test(sweep_protection_hooks_*)`; `test(repo_fork_installs_protection_hooks)` | ✅ | ✅ green (recorded `tmp/22.1-03-smoke-protection.out`, 2026-09-19) |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Suggested filters

```bash
cargo nextest run -p oxidean-api -E 'test(branch_protect) | test(commit_status)'
cargo test -p oxidean-db --test dialect_branch_protection
bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.settings.branches.integration.test.ts
bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.pull.protection.integration.test.ts
```

## Wave 0 files (created by 13-00 / 13-01)

- `crates/oxidean-api/tests/branch_protection_rpc.rs`
- `crates/oxidean-api/tests/branch_protect_push.rs`
- `crates/oxidean-api/tests/branch_protect_merge.rs`
- `crates/oxidean-api/tests/commit_status_rpc.rs`
- `crates/oxidean-db/tests/dialect_branch_protection.rs`
- `apps/web/src/routes/$owner.$repo.settings.branches.integration.test.ts`
- `apps/web/src/routes/$owner.$repo.pull.protection.integration.test.ts`

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live Compose protected-push denial (HTTPS + SSH) | ORG-06 (packaged) | Needs Docker stack up | `make up` then `make smoke-protection`; recorded run `tmp/22.1-03-smoke-protection.out` (2026-09-19) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s (Compose smoke excepted, operator-run)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `13-VERIFICATION.md` passed 2026-09-19 post-22.1 re-verify)

---

## Gate status (integrate honesty pass)

**Gate status: GREEN** — Wave 0 stubs greened; phase shipped on integrate `cursor/gsd-remaining-integrate-c82f` (2026-09-16).

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: add standard frontmatter + per-task map, confirm every mapped command still names real files/tests on disk, and confirm recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (draft-era doc upgraded: frontmatter added, Wave 0 checklist mapped to real task rows, packaged-denial row appended for Phase 22.1 scope) |
| Escalated | 0 |
| Manual-only | 1 (Compose protected-push smoke — recorded green 2026-09-19) |

| Check | Result |
|-------|--------|
| `branch_protection_rpc.rs` / `branch_protect_push.rs` / `branch_protect_merge.rs` / `commit_status_rpc.rs` | ✅ all present; named `branch_protect*` / `commit_status*` tests exist (incl. `sweep_protection_hooks_*` added by 22.1) |
| `crates/oxidean-db/tests/dialect_branch_protection.rs` | ✅ present (`dialect_branch_protection_migrate_schema_presence`) |
| Web: `settings.branches.integration.test.ts` + `pull.protection.integration.test.ts` | ✅ present |
| Packaged enforcement (22.1) | ✅ `crates/oxidean-api/Dockerfile` builds+COPYs `oxidean-protection-hook`; `docker-compose.yml` sets `OXIDEAN_PROTECTION_HELPER`; `scripts/compose-smoke-protection.sh` + `make smoke-protection` wired |
| Recorded green runs | `13-VERIFICATION.md`: 23/23 filter PASS + smoke denial cited (verified 2026-09-19), `status: passed`; `22.1-VERIFICATION.md` truth 1 smoke OK |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Previously unbucketed because the file had no frontmatter at all; content already matched disk.
