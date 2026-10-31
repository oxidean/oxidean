---
phase: "17"
slug: notifications
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 17 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (apps/web) |
| **Config file** | `Cargo.toml` workspace / `apps/web` vitest via package scripts |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(notification)'` |
| **Full suite command** | `make test` (or nextest notification + dialect_notifications + web notification filters) |
| **Estimated runtime** | ~60–120 seconds for notification-focused filters |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` verify
- **After every plan wave:** Run notification nextest + relevant Vitest filters
- **Before `/gsd-verify-work`:** Full notification + dialect + chrome/notifications web tests green
- **Max feedback latency:** 120 seconds for focused filters

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 17-00-T1 | 00 | 0 | NOTF-01/02 | T-17-01 | stubs discoverable | nextest list | see 17-00-PLAN | ✅ | ✅ present |
| 17-00-T2 | 00 | 0 | NOTF-02 | — | web stubs discoverable | file + vitest list | see 17-00-PLAN | ✅ | ✅ present |
| 17-01-T1 | 01 | 1 | NOTF-01/02 | T-17-01 | own-rows RPC | nextest | `cargo nextest run -p oxidean-api -E 'test(notification)'` | ✅ | ✅ green |
| 17-01-T2 | 01 | 1 | NOTF-01 | T-17-02 | comment→notify author | nextest | same filter | ✅ | ✅ green |
| 17-02-T1 | 02 | 2 | NOTF-01 | T-17-02 | issue event fan-out | nextest | `cargo nextest run -p oxidean-api -E 'test(notification)'` | ✅ | ✅ green |
| 17-02-T2 | 02 | 2 | NOTF-01 | — | @mention recipients | nextest | same | ✅ | ✅ green |
| 17-03-T1 | 03 | 3 | NOTF-01 | T-17-02 | PR event fan-out | nextest | PR+notification filter | ✅ | ✅ green |
| 17-04-T1 | 04 | 4 | NOTF-02 | T-17-03 | bell + badge | vitest | chrome notification filter | ✅ | ✅ green |
| 17-04-T2 | 04 | 4 | NOTF-02 | T-17-01 | list + mark read UI | vitest | notifications route filter | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/notification_rpc.rs` — stubs for NOTF-01/02 RPC behaviors
- [x] `crates/oxidean-db/tests/dialect_notifications.rs` — migration parity stub
- [x] `apps/web/src/components/chrome.notifications.integration.test.ts` — bell stub
- [x] `apps/web/src/routes/notifications.integration.test.ts` — list page stub

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Bell badge updates after another user's comment in a live stack | NOTF-01 | Multi-user timing / poll interval | Two browsers: A comments on B's issue; B sees badge increment within poll window |

*All other Phase 17 behaviors have automated verification planned.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 120s for focused filters
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `17-VERIFICATION.md` passed 2026-09-19)


## Gate status (integrate honesty pass)

**Gate status: GREEN** — Wave 0 stubs greened; phase shipped on integrate `cursor/gsd-remaining-integrate-c82f` (2026-09-16).

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: confirm every mapped command still names real files/tests on disk and that recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 7 (six task-map rows ⬜→✅ on verified file/test presence + recorded green; `status: complete` → `validated`) |
| Escalated | 0 |
| Manual-only | 1 (live two-browser badge poll — unchanged, documented above) |

| Check | Result |
|-------|--------|
| `crates/oxidean-api/tests/notification_rpc.rs` (`notification_*` own-rows, comment/assign/mention/PR fan-out, mark read, fail-closed unauthenticated) | ✅ present; 13 named tests exist |
| `crates/oxidean-db/tests/dialect_notifications.rs` | ✅ present (`dialect_notifications_migration_module_present`) |
| `apps/web/src/components/chrome.notifications.integration.test.ts` + `routes/notifications.integration.test.ts` | ✅ present |
| Recorded green runs | `17-VERIFICATION.md` truths PASS (verified 2026-09-19), `status: passed`; integrate gate note GREEN 2026-09-16 above |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Prior `complete` lifecycle value + stale ⬜ rows were why audit-milestone §5.5 bucketed this phase NOT-VALIDATED; map content now matches disk.
