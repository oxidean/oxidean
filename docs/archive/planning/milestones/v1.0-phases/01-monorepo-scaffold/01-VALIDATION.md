---
phase: 1
slug: monorepo-scaffold
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-09-09
approved: 2026-09-09
validated: 2026-09-13
updated: 2026-09-13
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust) + vitest (JS) |
| **Config file** | `packages/api-client/vitest.config.ts`; Cargo workspace |
| **Quick run command** | `cargo test -p oxidean-api && bun run --filter @oxidean/api-client test` |
| **Full suite command** | `cargo test --workspace && bun run --filter @oxidean/api-client test && make rpc-sync-check && docker compose config && bunx turbo run build --filter=@oxidean/web` |
| **CI** | `.github/workflows/ci.yml` — rust / js / rpc-sync / compose |
| **Estimated runtime** | Quick ~60s · Full ~5–8 min (Compose smoke when Docker Engine up) |

---

## Sampling Rate

- **After every task commit:** Run quick command (or package-scoped subset)
- **After every plan wave:** Run full suite for touched layers + Compose smoke when Compose plan lands
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds for quick; 600 seconds for full

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-00-01 | 01 | 0 | PLAT-* | — | N/A | infra | scaffold configs exist | ✅ | ✅ green |
| 01-02-01 | 02 | 2 | PLAT-05, PLAT-06 | T-01-03 | Reject missing/wrong `Oxidean-RPC-Version` | integration | `cargo test -p oxidean-api --test rpc_http` | ✅ `crates/oxidean-api/tests/rpc_http.rs` | ✅ green (re-verified 2026-09-13) |
| 01-02-02 | 02 | 2 | PLAT-06 | T-01-04 | CORS prod requires allowlist | unit | `cargo test -p oxidean-api cors::tests` | ✅ `crates/oxidean-api/src/cors.rs` | ✅ green (re-verified 2026-09-13; map filter was `cors_` — matched 0 tests) |
| 01-02-03 | 02 | 2 | PLAT-06 | T-01-05 | echo HTTP+WS (+ 8KiB cap) | integration | `cargo test -p oxidean-api --test rpc_http --test rpc_ws` | ✅ `tests/rpc_http.rs`, `tests/rpc_ws.rs` | ✅ green |
| 01-02-04 | 02 | 2 | PLAT-06 | T-01-06 | codegen sync | script | `make rpc-sync-check` | ✅ `scripts/check-rpc-sync.sh` | ✅ green (2026-09-13: dropped hand JSDoc; `make rpc-gen` SoT) |
| 01-03-01 | 03 | 3 | PLAT-04, PLAT-10, PLAT-11 | T-01-07 | web build | build | `bunx turbo run build --filter=@oxidean/web` | ✅ `apps/web` | ✅ green |
| 01-04-01 | 04 | 4 | PLAT-01 | T-01-09/10 | Compose config + smoke | smoke | `docker compose config` / `./scripts/compose-smoke.sh` | ✅ | ⚠️ env-blocked this host (`docker` not on PATH); last green 2026-09-09 + CI compose job |
| 01-05-01 | 05 | 5 | PLAT-* | T-01-12/13 | CI gates | ci | `.github/workflows/ci.yml` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky / env-blocked*

**Requirement coverage:** PLAT-01 COVERED (CI/smoke historically green; local docker CLI env-blocked), PLAT-04/05/06/10/11 COVERED.

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/rpc_http.rs` — health (`/health`), echo, version reject
- [x] `crates/oxidean-api/tests/rpc_ws.rs` — health/echo over WS
- [x] `packages/api-client` vitest config + smoke test
- [x] `scripts/check-rpc-sync.sh` — regen + diff
- [x] Vitest + Cargo workspace test harness installed

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions | UAT |
|----------|-------------|------------|-------------------|-----|
| Landing first-viewport brand/CTA composition | UI-SPEC / PLAT-04 | Visual judgment | Open `/` — mark + Oxidean hero + Get started / Explore Oxidean | ✅ passed (`01-UAT.md`) |
| Theme system → light/dark force persists | UI-SPEC | Browser preference UX | Toggle theme; refresh; confirm persistence | ✅ passed |
| Footer Status navigation | CONTEXT D-26 | Simple UX | Click footer Status → `/status` live health | ✅ passed (agent + human) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 120s quick
- [x] `nyquist_compliant: true` set in frontmatter after plans pass Dimension 8

**Approval:** Nyquist validate-phase 2026-09-13 (`status: validated`)

---

## Validation Audit 2026-09-09

| Metric | Count |
|--------|-------|
| Gaps found | 0 (MISSING) |
| Partial refreshed | 1 (`01-04-01` smoke → green) |
| Escalated | 0 |
| Suite re-run | cargo + vitest + rpc-sync + compose config + web build — green |

---

## Validation Audit 2026-09-13

| Metric | Count |
|--------|-------|
| Gaps found | 0 (MISSING new product tests) |
| Re-verify green | rpc_http (4/4); cors::tests (2/2); `make rpc-sync-check` after dropping hand JSDoc |
| Env-blocked | `docker compose config` — docker CLI not on PATH this host |
| Escalated then resolved | 1 (`01-02-04` hand JSDoc on generated client → `make rpc-gen` SoT restore) |
| Lifecycle | `#2117` `status: validated` applied |

### Commands run

| Command | Result |
|---------|--------|
| `cargo test -p oxidean-api --test rpc_http` | ✅ 4 passed |
| `cargo test -p oxidean-api cors::tests` | ✅ 2 passed |
| `make rpc-gen` + commit aligned client (removed hand JSDoc on `default_gitignore`) | ✅ |
| `make rpc-sync-check` | ✅ after client aligned |
| `docker compose config -q` | ⚠️ docker not on PATH |

### Map hygiene

- Updated `01-02-02` command `cors_` → `cors::tests`.
- PLAT-06 sync closed by restoring generated client as source of truth (no hand edits in `@oxidean/api-client`).
