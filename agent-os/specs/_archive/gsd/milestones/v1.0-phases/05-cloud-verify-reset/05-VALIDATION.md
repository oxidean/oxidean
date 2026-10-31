---
phase: "5"
slug: "cloud-verify-reset"
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-10"
updated: "2026-09-11"
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `05-RESEARCH.md` § Validation Architecture.
> Audited 2026-09-11 via `/gsd-validate-phase 5` — all automated tasks green.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: `cargo test` (workspace); TS: Vitest in `apps/web` + `@oxidean/api-client` |
| **Config file** | crates’ `[[test]]` / apps/web vitest projects |
| **Quick run command** | `cargo test -p oxidean-api --test auth_verify_reset --test auth_verify_gate` |
| **Full suite command** | `make test` + `make db-matrix` (when DB touched) + `bun run test` in apps/web + api-client vitest if client changed |
| **Estimated runtime** | ~30–60s quick / ~3–8m full + dialect matrix |

---

## Sampling Rate

- **After every task commit:** Targeted `cargo test -p oxidean-api --test …` / `bun --cwd apps/web run build` for the touched area
- **After every plan wave:** Workspace Rust tests + web build (+ dialect_auth when migrations/token CRUD touched)
- **Before `$gsd-verify-work`:** Full suite green + human UAT per UI-SPEC (banner, verify/reset copy, anti-enumeration)
- **Max feedback latency:** 60 seconds (quick path)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01-T1 | 01 | 1 | AUTH-04 | T-05-03 | 0003 tokens + CRUD + UserPublic.email_verified field | unit + parity | `cargo test -p oxidean-core --lib` | ✅ | ✅ green |
| 05-01-T2 | 01 | 1 | AUTH-04 / PLAT-08 | — | Token migrate + CRUD + verified helpers on dialects | integration | `cargo test -p oxidean-db --test dialect_auth` | ✅ | ✅ green |
| 05-02-T1 | 02 | 2 | AUTH-04 | T-05-01…T-05-04 | Unverified → `auth.email_unverified`+403; OTP verify → me.email_verified + ping ok | integration | `cargo test -p oxidean-api --test auth_verify_gate --test auth_verify_reset` | ✅ | ✅ green |
| 05-02-T2 | 02 | 2 | AUTH-04 | T-05-04 | privileged_ping unknown outside allowlist | integration | `cargo test -p oxidean-api --test auth_verify_gate` | ✅ | ✅ green |
| 05-03-T1 | 03 | 3 | AUTH-04 | T-05-05…T-05-08 | Issue/resend/rate-limit/magic+OTP; PUBLIC_ORIGIN links | integration | `cargo test -p oxidean-api --test auth_verify_reset` | ✅ | ✅ green |
| 05-03-T2 | 03 | 3 | AUTH-04, AUTH-05 | T-05-08 | Signup auto-send verify; admin seed verified; open signup | integration | `cargo test -p oxidean-api --test auth_signup --test auth_verify_reset` | ✅ | ✅ green |
| 05-03-T3 | 03 | 3 | AUTH-04 | — | Reserved usernames `verify`, `reset-password`, `setup` | unit | `cargo test -p oxidean-core --lib` | ✅ | ✅ green |
| 05-04-T1 | 04 | 4 | AUTH-12 | T-05-09, T-05-12 | Reset request anti-enumeration; mail only local-password | integration | `cargo test -p oxidean-api --test auth_verify_reset` | ✅ | ✅ green |
| 05-04-T2 | 04 | 4 | AUTH-12 | T-05-10, T-05-11 | Redeem sets password, revokes others, Set-Cookie | integration | `cargo test -p oxidean-api --test auth_verify_reset` | ✅ | ✅ green |
| 05-05-T1 | 05 | 5 | AUTH-04 | T-05-13 | IdP email_verified=true sets verified_at | unit/integration | `cargo test -p oxidean-api --lib` + `--test auth_verify_gate` | ✅ | ✅ green |
| 05-05-T2 | 05 | 5 | AUTH-04 | T-05-14 | clear_email_verification helper + unit test | unit | `cargo test -p oxidean-api --lib clear_email_verification` | ✅ | ✅ green |
| 05-06-T1 | 06 | 6 | — | T-05-SC | OTP via `@octanejs/base-ui/otp-field` (no raw `input-otp` dep) | design | `apps/web/src/components/ui/input-otp.tsrx` | ✅ | ✅ green |
| 05-06-T2 | 06 | 6 | AUTH-04 | T-05-SC, T-05-15, T-05-16 | rpc-gen + InputOtp + /verify; returnTo unit tests | unit + integration | `bun --cwd apps/web run test:unit` (`return-to.unit.test.ts`) | ✅ | ✅ green |
| 05-06-T3 | 06 | 6 | AUTH-04 | — | VerifyBanner under header for unverified | human UAT | `05-UAT.md` test 1 pass | ✅ UAT | ✅ green |
| 05-07-T1 | 07 | 7 | AUTH-12, AUTH-05 | T-05-17, T-05-18, T-05-20 | /reset-password anti-enum + redeem + SSO; no invite UI | integration | `bun --cwd apps/web run test:integration` (`reset-password`, `signup`) | ✅ | ✅ green |
| 05-07-T2 | 07 | 7 | AUTH-04 | T-05-19 | Disabled New repository CTA hints | integration | `bun --cwd apps/web run test:integration` (`signed-in-home`) | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/auth_verify_reset.rs`
- [x] `crates/oxidean-api/tests/auth_verify_gate.rs`
- [x] `crates/oxidean-db/tests/dialect_auth.rs` for `0003` tokens + verified helpers
- [x] Web OTP/verify/reset/CTA coverage via Vitest integration + UAT
- [x] rpc-gen / api-client regeneration after DTO/RPC surface complete

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Verify banner chrome / theme | AUTH-04 | Visual / light-dark | Covered by UAT test 1 (pass); optional re-spot-check |
| `/verify` OTP chrome a11y | AUTH-04 | Browser judgment | Covered by UAT test 2 (pass) |
| Live SMTP/Resend E2E | AUTH-12 | Requires operator secrets | Only when keys configured; default CI uses log-sink |
| UI backstops (overflow/long-text) | UI | Held-out visual | max-w-md wrap on narrow viewports |

*Promoted to automated (2026-09-11): anti-enumeration reset panel, redeem/SSO UI, New repository CTA hints, signup no-invite UI.*

---

## Validation Audit 2026-09-11

| Metric | Count |
|--------|-------|
| Gaps found | 0 (map was draft/pending; all tasks green on re-run) |
| Resolved | 16 task rows marked ✅ |
| Escalated | 0 |

**Commands verified green:**
- `cargo test -p oxidean-core --lib`
- `cargo test -p oxidean-api --test auth_verify_gate --test auth_verify_reset --test auth_signup --test auth_bootstrap` (28)
- `cargo test -p oxidean-api --lib clear_email_verification`
- `cargo test -p oxidean-db --test dialect_auth`
- `bun --cwd apps/web run test:integration` (reset-password, signup, signed-in-home — 9)
- `bun --cwd apps/web run test:unit` (reset-password-copy, return-to — 5)

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 / UAT dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-09-11
