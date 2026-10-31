---
phase: "06"
slug: "self-host-admin-bootstrap"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-11"
validated: "2026-09-12"
---

# Phase 06 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo-nextest 0.9.x (API) + Vitest 5.x (web) |
| **Config file** | workspace Cargo; `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(bootstrap) \| test(seeded_admin) \| test(signup) \| test(confirm_admin) \| test(d14_fail_closed) \| test(admin_get)'` + `bun --cwd apps/web run test -- src/lib/ssr-auth.gate.test.ts src/lib/ssr-auth.cookie-forward.unit.test.ts src/routes/setup.integration.test.ts src/routes/setup.credentials.integration.test.ts src/routes/dashboard.integration.test.ts src/routes/index.integration.test.ts src/components/chrome.integration.test.ts src/routes/signup.integration.test.ts src/routes/admin/auth.integration.test.ts` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~60–180 seconds (full); ~15–40s quick |

---

## Sampling Rate

- **After every task commit:** Run quick API filter and/or `bun --cwd apps/web run test:unit`
- **After every plan wave:** `cargo nextest run -p oxidean-api` + web unit/integration
- **Before `$gsd-verify-work`:** Full suite (`make test`) must be green
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-00-T1 | 00 | 0 | AUTH-06/07 | T-06-00 | Wave 0 API stubs (partial ENV, seed, allowlist, credentials) | integration | `cargo nextest run -p oxidean-api -E 'test(bootstrap) \| test(seeded_admin) \| test(signup) \| test(confirm_admin)'` | ✅ | ✅ green |
| 06-00-T2 | 00 | 0 | AUTH-06/07 | — | Wave 0 web/dialect stubs incl. index SSR gate + `0006` columns | integration | `bun --cwd apps/web run test -- src/routes/setup.integration.test.ts src/routes/setup.credentials.integration.test.ts src/routes/dashboard.integration.test.ts src/routes/index.integration.test.ts src/components/chrome.integration.test.ts` | ✅ | ✅ green |
| 06-01-T1 | 01 | 1 | AUTH-06/07 | T-06-01 | `0006_bootstrap_flags` + DB helpers | unit | `cargo test -p oxidean-core --lib && cargo test -p oxidean-db --lib migration_parity` | ✅ | ✅ green |
| 06-01-T2 | 01 | 1 | AUTH-06/07 | T-06-02 | DTOs + dialect_auth column coverage | integration | `cargo test -p oxidean-db --lib migration_parity && cargo test -p oxidean-db --test dialect_auth && cargo test -p oxidean-core --lib` | ✅ | ✅ green |
| 06-02-T1 | 02 | 2 | AUTH-06 | T-06 D-14 | Fail-closed boot checkpoint | unit | `cargo nextest run -p oxidean-api -E 'test(d14_fail_closed)'` | ✅ | ✅ green |
| 06-02-T2 | 02 | 2 | AUTH-06 | T-06-03 | ENV seed → must_change → confirm | integration | `cargo nextest run -p oxidean-api -E 'test(seeded_admin) \| test(confirm_admin)'` | ✅ | ✅ green |
| 06-02-T3 | 02 | 2 | AUTH-06 | — | Partial ENV + seed idempotency | integration | `cargo nextest run -p oxidean-api -E 'test(bootstrap) \| test(seeded_admin) \| test(signup)'` | ✅ | ✅ green |
| 06-03-T1 | 03 | 3 | AUTH-07 | — | bootstrap_setup + allow_signup | integration | `cargo nextest run -p oxidean-api -E 'test(bootstrap)'` | ✅ | ✅ green |
| 06-03-T2 | 03 | 3 | AUTH-07 | T-06 D-11 | Strict RPC allowlist + SSO reject | integration | `cargo nextest run -p oxidean-api -E 'test(bootstrap) \| test(seeded_admin) \| test(signup)'` | ✅ | ✅ green |
| 06-04-T1 | 04 | 4 | AUTH-05/07 | — | provider_config.allow_signup + signup reject | integration | `cargo nextest run -p oxidean-api -E 'test(signup) \| test(bootstrap) \| test(seeded_admin)'` | ✅ | ✅ green |
| 06-04-T2 | 04 | 4 | AUTH-05 | — | admin.auth allow_signup round-trip | integration | `cargo nextest run -p oxidean-api -E 'test(admin_get) \| test(signup)'` | ✅ | ✅ green |
| 06-05-T1 | 05 | 5 | AUTH-06/07 | T-06-11 | ssr-auth Cookie-forward + resolveAppAccessRedirect | unit | `bun --cwd apps/web run test -- src/lib/ssr-auth.cookie-forward.unit.test.ts src/lib/ssr-auth.gate.test.ts` | ✅ | ✅ green |
| 06-05-T2 | 05 | 5 | AUTH-06/07 | T-06-15 | Shared root SSR gate (needs_setup + must_change) path matrix | unit | `bun --cwd apps/web run test -- src/lib/ssr-auth.gate.test.ts` | ✅ | ✅ green |
| 06-05-T3 | 05 | 5 | AUTH-06 | T-06 D-18/20 | `/` SSR tree selection (marketing vs SignedInHome) | integration | `bun --cwd apps/web run test -- src/routes/index.integration.test.ts` | ✅ | ✅ green |
| 06-06-T1 | 06 | 6 | AUTH-07 | — | Switch + /setup wizard | integration | `bun --cwd apps/web run test -- src/routes/setup.integration.test.ts` | ✅ | ✅ green |
| 06-06-T2 | 06 | 6 | AUTH-06 | T-06-14 | /setup/credentials forced change | integration | `bun --cwd apps/web run test -- src/routes/setup.credentials.integration.test.ts` | ✅ | ✅ green |
| 06-09-T1 | 09 | 6 | AUTH-06 | T-06-16 | `/dashboard` notFound | integration | `bun --cwd apps/web run test -- src/routes/dashboard.integration.test.ts` | ✅ | ✅ green |
| 06-09-T2 | 09 | 6 | AUTH-06/07 | — | `/signup` allow_signup false → notFound | integration | `bun --cwd apps/web run test -- src/routes/signup.integration.test.ts` | ✅ | ✅ green |
| 06-08-T1 | 08 | 7 | AUTH-05/07 | T-06-13 | Closed signup chrome/landing/login omit | integration | `bun --cwd apps/web run test -- src/components/chrome.integration.test.ts` | ✅ | ✅ green |
| 06-08-T2 | 08 | 7 | AUTH-05 | — | admin/auth Allow open signup Switch | integration | `bun --cwd apps/web run test -- src/routes/admin/auth.integration.test.ts` | ✅ | ✅ green |
| 06-07-T1 | 07 | 8 | AUTH-06/07 | — | Docs + REQUIREMENTS reframe + COVERAGE | grep | `rg -n 'OXIDEAN_ALLOW_SIGNUP\|empty.instance\|system-administrator' docs/CONFIGURATION.md .env.example .planning/REQUIREMENTS.md` | ✅ | ✅ green |
| 06-07-T2 | 07 | 8 | AUTH-06/07 | — | rpc-gen + phase smoke | integration | `cargo nextest run -p oxidean-api -E 'test(bootstrap) \| test(seeded_admin) \| test(signup)'` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] Extend `crates/oxidean-api/tests/auth_bootstrap.rs` — partial ENV, allow_signup on setup, strict RPC allowlist
- [x] Extend seed tests — username `system-administrator`, `must_change_credentials`, `OXIDEAN_ALLOW_SIGNUP`
- [x] New API tests — forced credential change RPC; signup blocked when `allow_signup=false`
- [x] Web: `/setup` Switch + `/setup/credentials` form integration tests
- [x] Web: signup/chrome omit + dashboard notFound tests
- [x] Web: `apps/web/src/routes/index.integration.test.ts` — D-18/D-20 home SSR tree gate (needs_setup vs SignedInHome vs marketing)
- [x] Migration `0006_bootstrap_flags` dialect triple + `dialect_auth` coverage for new columns

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| First-boot wizard UX on empty compose | AUTH-07 | Full browser + empty DB | Bring up empty self-host without ADMIN env; complete wizard; confirm admin session |
| ENV seed then forced credentials UX | AUTH-06 | Browser + seeded admin | Set both ADMIN env; boot; login; confirm redirect to `/setup/credentials` |
| Long-text wrap on setup surfaces | AUTH-06/07 | Held-out visual (verification:backstop) | Paste long helper/error on `/setup` and `/setup/credentials`; confirm wrap inside AuthShell `max-w-md` |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-09-12 (Nyquist audit)

---

## Validation Audit 2026-09-12

| Metric | Count |
|--------|-------|
| Gaps found | 2 |
| Resolved | 2 |
| Escalated | 0 |

### Gaps filled this audit

| Task ID | Gap | Resolution |
|---------|-----|------------|
| 06-05-T1 | Cookie-forward (T-06-11) had build-only verify | Added `ssr-auth.cookie-forward.unit.test.ts` (4 assertions) — green |
| 06-02-T1 | D-14 fail-closed was human/gate only | Added `boot_fail_closed.rs` (`d14_fail_closed_exits_on_admin_seed_err`) — green |

### Re-verified existing coverage

- API nextest filter (bootstrap / seeded_admin / signup / confirm_admin / admin): 22 passed
- Web vitest (8 phase-06 files): 21 passed
- `oxidean-core` lib + `migration_parity` + `dialect_auth` (incl. `migrate_0006_bootstrap_flags_columns`): green
- Docs grep (`OXIDEAN_ALLOW_SIGNUP` / empty-instance / `system-administrator`): 11 hits
