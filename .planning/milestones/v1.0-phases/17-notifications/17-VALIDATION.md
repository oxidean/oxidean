---
phase: "17"
slug: notifications
status: complete
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
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
| 17-01-T1 | 01 | 1 | NOTF-01/02 | T-17-01 | own-rows RPC | nextest | `cargo nextest run -p oxidean-api -E 'test(notification)'` | ✅ | ⬜ pending |
| 17-01-T2 | 01 | 1 | NOTF-01 | T-17-02 | comment→notify author | nextest | same filter | ✅ | ⬜ pending |
| 17-02-T1 | 02 | 2 | NOTF-01 | T-17-02 | issue event fan-out | nextest | `cargo nextest run -p oxidean-api -E 'test(notification)'` | ✅ | ⬜ pending |
| 17-02-T2 | 02 | 2 | NOTF-01 | — | @mention recipients | nextest | same | ✅ | ⬜ pending |
| 17-03-T1 | 03 | 3 | NOTF-01 | T-17-02 | PR event fan-out | nextest | PR+notification filter | ✅ | ⬜ pending |
| 17-04-T1 | 04 | 4 | NOTF-02 | T-17-03 | bell + badge | vitest | chrome notification filter | ✅ | ⬜ pending |
| 17-04-T2 | 04 | 4 | NOTF-02 | T-17-01 | list + mark read UI | vitest | notifications route filter | ✅ | ⬜ pending |

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

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s for focused filters
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending


## Gate status (integrate honesty pass)

**Gate status: GREEN** — Wave 0 stubs greened; phase shipped on integrate `cursor/gsd-remaining-integrate-c82f` (2026-09-16).
