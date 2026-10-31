---
phase: "19"
slug: "actions-runners"
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-16"
updated: "2026-09-16"
---

# Phase 19 — Validation Strategy

> Per-phase validation contract. Seeded from `19-RESEARCH.md` Validation Architecture.
> Updated by plan **19-11** after docs + smoke + phase gate execution.
> Nyquist audit by `/gsd-validate-phase` (2026-09-16).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: cargo-nextest; Web: Vitest via Bun |
| **Config file** | `.config/nextest.toml`; `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(actions_)|test(runner_)|test(commit_status)|test(actions_secrets)'` |
| **Full suite command** | `make test` (+ `make rpc-sync-check`; `make smoke-actions` skip-ok without Docker) |
| **Estimated runtime** | ~90–300 seconds (quick); longer with smoke |

---

## Sampling Rate

- **After every task commit:** targeted nextest / Vitest filter for touched area
- **After every plan wave:** actions nextest + web Actions Vitest + `make rpc-sync-check` after RPC changes
- **Before `/gsd-verify-work`:** `make test` + `make smoke-actions` (skip-ok) green
- **Max feedback latency:** 300 seconds

---

## Requirement Coverage (ACT-01…07)

| Req | Behavior | Automated proof | Status |
|-----|----------|-----------------|--------|
| ACT-01 | GHA-compatible `.github/workflows` YAML | `actions_workflow_parse_*` | COVERED |
| ACT-02 | push + pull_request dispatch | `actions_triggers_*` | COVERED |
| ACT-03 | Run status + logs UI | `actions_rpc_*` + Vitest Actions list/detail | COVERED |
| ACT-04 | Official runner image register/run | `smoke-actions` Dockerfile/Compose + register probe; live job manual | COVERED* |
| ACT-05 | Compose / standalone docs | `smoke-actions` README/Compose rg + DEPLOYMENT.md | COVERED* |
| ACT-06 | Registration + job-dispatch + labels | `actions_runner_protocol_*` (register/declare/fetch/`update_task`/`update_log`) + secrets | COVERED |
| ACT-07 | Registered runners only; no managed minutes | `actions_dispatch_policy_*` | COVERED |

\*Live end-to-end runner job and multi-host standalone remain **manual-only** (environment). Static/smoke + protocol integration cover the forge-side contract.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|----------------|-----------------|-----------|-------------------|-------------|--------|
| 19-00-T* | 00 | 0 | ACT-01..07 | T-19-SC | Wave 0 RED Rust stubs | integration | nextest list actions_* | ✅ | ✅ |
| 19-01-T* | 01 | 0 | ACT-03..05 | T-19-SC | Wave 0 web + smoke stubs | component/smoke | Vitest actions + smoke-actions.sh | ✅ | ✅ |
| 19-02-T* | 02 | 1 | ACT-03,07 | T-19-01 | Schema + LOG_DIR + factory reset | integration | dialect_actions | ✅ | ✅ |
| 19-03-T* | 03 | 2 | ACT-01 | T-19-02 | Workflow parse + run/job rows | integration | actions_workflow_parse | ✅ | ✅ |
| 19-04-T* | 04 | 3 | ACT-01,02,06,07 | T-19-03 | Tracer push→queue→FetchTask | integration | actions_triggers + runner protocol | ✅ | ✅ |
| 19-05-T* | 05 | 4 | ACT-04,06 | T-19-04 | Full runner protocol + labels | integration | actions_runner_protocol | ✅ | ✅ |
| 19-06-T* | 06 | 4 | ACT-02 | T-19-05 | pull_request dispatch contract | integration | actions_triggers | ✅ | ✅ |
| 19-07-T* | 07 | 5 | ACT-03 + Ph13 | T-19-06 | Commit statuses publish/query | integration | commit_statuses | ✅ | ✅ |
| 19-08-T* | 08 | 5 | ACT-04,05 | T-19-07 | Official image + Compose | smoke/docs | smoke-actions + Dockerfile rg | ✅ | ✅ |
| 19-09-T* | 09 | 6 | ACT-03 | T-19-08 | Actions UI list/detail/logs | component | Vitest actions routes | ✅ | ✅ |
| 19-10-T* | 10 | 6 | ACT-06,07 | T-19-09 | Tokens, secrets, repo enable | integration + UI | actions_secrets + admin | ✅ | ✅ |
| 19-11-T* | 11 | 7 | ACT-01..07 | T-19-01 | Docs + smoke + phase gate | mixed | smoke-actions + docs rg | ✅ | ✅ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/actions_workflow_parse.rs` — ACT-01
- [x] `crates/oxidean-api/tests/actions_triggers.rs` — ACT-02
- [x] `crates/oxidean-api/tests/actions_rpc.rs` — ACT-03
- [x] `crates/oxidean-api/tests/actions_runner_protocol.rs` — ACT-06 (includes `update_task` / `update_log`)
- [x] `crates/oxidean-api/tests/actions_dispatch_policy.rs` — ACT-07
- [x] `crates/oxidean-api/tests/actions_secrets.rs` — ACT-06 secrets
- [x] `crates/oxidean-api/tests/commit_statuses.rs` — Phase 13 surface / D-ACT-15
- [x] `crates/oxidean-db/tests/dialect_actions.rs` — migration parity
- [x] `apps/web/src/routes/$owner.$repo.actions.integration.test.ts` — ACT-03 UI
- [x] `apps/web/src/routes/$owner.$repo.actions.$run.integration.test.ts` — ACT-03 detail
- [x] `scripts/smoke-actions.sh` + `make smoke-actions` — ACT-04/05

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Official runner registers and runs a real job | ACT-04 | Needs Docker + runner image | Compose profile actions; push workflow; watch job green |
| Standalone runner against public origin | ACT-05 | Multi-host networking | Register with non-loopback ORIGIN; checkout works in job container |
| Phase 13 required check blocks merge | ORG-05/PR-08 | Phase 13 not this phase | Verify status contexts exist and are queryable for Phase 13 |

### Soft gaps (non-blocking)

| Gap | Notes |
|-----|-------|
| No Vitest for `admin/runners.tsrx` / settings Actions panel | Admin token mint + `listRunners` covered by `actions_secrets_admin_create_registration_token`; repo enable by `actions_secrets_enable_toggle_persists`. ACT-03 UI requirement is Actions list/detail/logs. |
| Nextest quick filter also matches `issue_reactions_*` / one notification test | Expression `test(actions_)` / `test(commit_status)` is broad; all still pass. Tighten later if noise matters. |

---

## Phase gate (19-11)

Documented in [docs/TESTING.md](../../../docs/TESTING.md#actions-phase-gate-phase-19):

```bash
make smoke-actions
cargo nextest run -p oxidean-api -E 'test(actions_)|test(runner_)|test(commit_status)|test(actions_secrets)'
make rpc-sync-check
```

**Validate-phase re-run (2026-09-16):** actions nextest 34 passed (includes new `actions_runner_protocol_update_task_and_log`); db dialect/factory_reset actions 3 passed; Vitest Actions routes 6 passed; `smoke-actions` skip-ok (no Docker engine).

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 300s
- [x] `nyquist_compliant: true` — `/gsd-validate-phase` 2026-09-16

**Approval:** validated (Nyquist) — ACT-01…07 automated or intentional manual-only

---

## Validation Audit 2026-09-16

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |
| Manual-only (intentional) | 3 |

**Gap filled:** ACT-06 protocol `POST /api/actions/update_task` + `update_log` had no behavioral test (register/fetch covered; job state/log via HTTP untested). Added `actions_runner_protocol_update_task_and_log` — green.
