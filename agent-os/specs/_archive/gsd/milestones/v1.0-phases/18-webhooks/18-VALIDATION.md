---
phase: "18"
slug: "webhooks"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 18: Webhooks - Validation Strategy

**Nyquist validation** for HOOK-01, HOOK-02, HOOK-03.
Retroactively reconciled to the standard contract on 2026-10-02 (ROADMAP DEBT-08); original checklist content preserved below.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(webhook_)'` + `cargo test -p oxidean-db --test dialect_webhooks` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted |

---

## Sampling Rate

- **Per task commit:** focused `test(webhook_*)` nextest filter + relevant Vitest file
- **Per wave merge:** `make test` + `make rpc-sync-check` after RPC/client changes
- **Phase gate:** Full automated gate green before `/gsd-verify-work`

---

## Wave 0 Gaps

| Gap | Stub file | Turns green in |
|-----|-----------|----------------|
| webhook CRUD RPC | `crates/oxidean-api/tests/webhook_rpc.rs` | 18-01, 18-02 |
| delivery + HMAC | `crates/oxidean-api/tests/webhook_delivery.rs` | 18-01, 18-02, 18-03 |
| dialect migration | `crates/oxidean-db/tests/dialect_webhooks.rs` | 18-01 |
| settings Webhooks UI | `apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts` | 18-04 |

## Requirements ↔ Tests

| Req | Automated | Notes |
|-----|-----------|-------|
| HOOK-01 | `cargo nextest run -p oxidean-api -E 'test(webhook_)'` + Vitest settings webhooks | Admin CRUD |
| HOOK-02 | `webhook_delivery` + issue/push/PR emit filters | Mock HTTP sink |
| HOOK-03 | deliveries.list assertions + Vitest history rows | Status visible |

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------------|-----------|-------------------|-------------|--------|
| 18-00-T1 | 00 | 0 | HOOK-01..03 | RED stubs discoverable (RPC + delivery + dialect) | stubs | `cargo nextest list -p oxidean-api -E 'test(webhook)'` + `test -f` the three Rust files | ✅ | ✅ green |
| 18-00-T2 | 00 | 0 | HOOK-01 | Settings Webhooks Vitest stub | component | `test -f apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts` | ✅ | ✅ green |
| 18-01-T1 | 01 | 1 | HOOK-01, HOOK-02 | Admin create → issue.opened delivery attempt | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_create) \| test(webhook_list) \| test(webhook_admin) \| test(webhook_issues_deliver)'` + `dialect_webhooks` | ✅ | ✅ green |
| 18-01-T2 | 01 | 1 | HOOK-01 | update/delete/active + ping stub path | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_update) \| test(webhook_delete) \| test(webhook_inactive)'` | ✅ | ✅ green |
| 18-01-T3 | 01 | 1 | HOOK-02 | issue.update lifecycle actions → issues events | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_issues_)'` | ✅ | ✅ green |
| 18-02-T1 | 02 | 2 | HOOK-02 | SSRF policy + HMAC contract + timeouts | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_hmac) \| test(webhook_ssrf) \| test(webhook_timeout)'` | ✅ | ✅ green |
| 18-02-T2 | 02 | 2 | HOOK-03 | Retry worker + deliveries.list/ping/redeliver RPC | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_retry) \| test(webhook_deliveries) \| test(webhook_ping) \| test(webhook_redeliver)'` + `make rpc-sync-check` | ✅ | ✅ green |
| 18-03-T1 | 03 | 3 | HOOK-02 | push event after HTTPS + SSH receive success | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_push)'` | ✅ | ✅ green |
| 18-03-T2 | 03 | 3 | HOOK-02 | pull_request emitters on Phase 12 PR model | integration | `cargo nextest run -p oxidean-api -E 'test(webhook_pull_request)'` | ✅ | ✅ green |
| 18-04-T1 | 04 | 4 | HOOK-01 | Webhooks panel CRUD on repo settings | component | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.settings.webhooks.integration.test.ts` | ✅ | ✅ green |
| 18-04-T2 | 04 | 4 | HOOK-03 | Delivery history + ping/redeliver UI | component | same Vitest file + `make web-lint && make web-format-check` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Prior verify commands to reuse

Prefer `cargo nextest run -p oxidean-api -E '…'` and `bun`/`make` web test filters already used in Phase 15 Wave 0.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real outbound delivery to an external endpoint | HOOK-02 | Needs a reachable receiver; automated coverage uses mock sink | Create webhook on a repo, point at a local `nc -l` / request-bin endpoint, push, observe signed delivery + row in deliveries list |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-02 (retroactive audit below; `18-VERIFICATION.md` passed 2026-09-19)

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: add standard frontmatter + per-task map, confirm every mapped command still names real files/tests on disk, and confirm recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (draft-era doc upgraded: frontmatter added, Wave 0 gaps mapped to real task rows) |
| Escalated | 0 |
| Manual-only | 1 (live external receiver — documented above) |

| Check | Result |
|-------|--------|
| `crates/oxidean-api/tests/webhook_rpc.rs` (`webhook_create_admin`, `webhook_list_admin`, `webhook_update_admin`, `webhook_delete_admin`, `webhook_admin_denial`, `webhook_deliveries_list`, `webhook_ping`, `webhook_redeliver`) | ✅ present; named tests exist |
| `crates/oxidean-api/tests/webhook_delivery.rs` (`webhook_hmac_signature`, `webhook_ssrf_rejects_unsafe_url`, `webhook_timeout_records_error`, `webhook_retry_transient`, `webhook_inactive_skips_enqueue`, `webhook_issues_*`, `webhook_push_*`, `webhook_pull_request_lifecycle`) | ✅ present; named tests exist |
| `crates/oxidean-db/tests/dialect_webhooks.rs` | ✅ present (`dialect_webhooks_migrate_schema_presence`) |
| `apps/web/src/routes/$owner.$repo.settings.webhooks.integration.test.ts` | ✅ present |
| Recorded green runs | `18-VERIFICATION.md` truths PASS (verified 2026-09-19), `status: passed` |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Previously unbucketed because the file had no frontmatter; content already matched disk.
