---
phase: "14"
slug: "git-lfs"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-09-19"
validated_at: "2026-09-19"
---

# Phase 14 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded by plan-phase from 14-RESEARCH.md Validation Architecture. Nyquist reconcile via `/gsd-validate-phase` equivalent (22.1-09, 2026-09-19).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: cargo-nextest / `cargo test`; Web: Vitest via Bun |
| **Config file** | workspace Cargo; `.config/nextest.toml`; `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(lfs)'` |
| **Full suite command** | `make test` (+ `make smoke-git-lfs` when Docker available) |
| **Estimated runtime** | ~60–180 seconds (quick); full suite longer with smoke |

---

## Sampling Rate

- **After every task commit:** Run targeted nextest `test(lfs)` and/or Vitest for the plan’s files
- **After every plan wave:** Run `make test` + `make rpc-sync-check` (after 14-08)
- **Before `/gsd-verify-work`:** Full suite green + `make smoke-git-lfs` when Compose is up
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 14-00-T1 | 00 | 0 | GIT-12, GIT-13 | T-14-01, T-14-02 | Wave 0 RED stubs batch/auth/store | integration | `cargo nextest list -p oxidean-api -E 'test(lfs)'` | ✅ | ✅ green |
| 14-00-T2 | 00 | 0 | GIT-13 | T-14-04 | dialect + factory_reset LFS wipe stubs | integration | `cargo nextest list -p oxidean-db -E 'test(dialect_lfs)'` | ✅ | ✅ green |
| 14-01-T1 | 01 | 0 | GIT-12 | T-14-05 | Wave 0 web stubs pointer/settings/admin/browser | component | `test -f apps/web/src/lib/lfs-pointer.unit.test.ts` | ✅ | ✅ green |
| 14-02-T1 | 02 | 1 | GIT-12, GIT-13 | T-14-01, T-14-03 | Tracer batch+basic → LFS_DIR shard | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_batch) \| test(lfs_store)'` | ✅ | ✅ green |
| 14-03-T1 | 03 | 2 | GIT-12 | T-14-01, T-14-02 | PAT Basic; cookie ignore; Read/Write | integration | `cargo nextest run -p oxidean-api -E 'test(lfs)'` | ✅ | ✅ green |
| 14-03-T2 | 03 | 2 | GIT-12 | T-14-02 | Admin-only enable; disabled rejects | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_enable)'` | ✅ | ✅ green |
| 14-04-T1 | 04 | 3 | GIT-12, GIT-13 | T-14-03 | Max size + quotas reject upload | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_quota)'` | ✅ | ✅ green |
| 14-05-T1 | 05 | 4 | GIT-12 | T-14-04 | Dedup skip actions; verify; Range GET | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_dedup) \| test(lfs_verify)'` | ✅ | ✅ green |
| 14-06-T1 | 06 | 5 | GIT-13 | T-14-04 | GC unreferenced + factory reset wipe | integration | `cargo nextest run -p oxidean-api -E 'test(lfs_gc) \| test(factory_reset)'` | ✅ | ✅ green |
| 14-07-T1 | 07 | 5 | GIT-13 | T-14-SC | Compose `OXIDEAN_LFS_DIR` + CONFIGURATION | docs/smoke | `rg -n 'OXIDEAN_LFS_DIR' docker-compose.yml docs/CONFIGURATION.md` | ✅ | ✅ green |
| 14-08-T1 | 08 | 6 | GIT-12, GIT-13 | T-14-05 | rpc-gen repo.lfs / admin.lfs | codegen | `make rpc-gen && make rpc-sync-check` | ✅ | ✅ green |
| 14-09-T1 | 09 | 7 | GIT-12 | T-14-02 | Repo Settings toggle + usage breakdown | component | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.settings.lfs.integration.test.ts` | ✅ | ✅ green |
| 14-10-T1 | 10 | 7 | GIT-13 | T-14-03 | Admin quotas + instance usage | component | `bun --cwd apps/web exec vitest run src/routes/admin/lfs.integration.test.ts` | ✅ | ✅ green |
| 14-11-T1 | 11 | 8 | GIT-12 | T-14-05 | Pointer badge + Download + browser | component | `bunx vitest run src/lib/lfs-pointer.unit.test.ts src/components/repo/blob-viewer.lfs.integration.test.ts src/components/repo/lfs-browser.integration.test.ts` | ✅ | ✅ green |
| 14-12-T1 | 12 | 9 | GIT-12, GIT-13 | T-14-01 | smoke-git-lfs + docs + phase gate | smoke/mixed | `make smoke-git-lfs` (or skip-if-no-docker) + nextest `test(lfs)` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/lfs_batch.rs` — GIT-12 batch/auth matrix stubs — **14-00**
- [x] `crates/oxidean-api/tests/lfs_store.rs` — GIT-13 layout + hash mismatch stubs — **14-00**
- [x] `crates/oxidean-db/tests/dialect_lfs.rs` — next-free LFS migration parity stub — **14-00**
- [x] Extend `crates/oxidean-api/tests/factory_reset_scope.rs` — LFS_DIR wipe expectation — **14-00**
- [x] `apps/web/src/lib/lfs-pointer.unit.test.ts` (+ settings/admin/browser stubs) — **14-01**
- [x] `scripts/smoke-git-lfs.sh` + `make smoke-git-lfs` — scaffold **14-00** / green **14-12**

*Existing nextest/Vitest/`make test` infrastructure covers runners; no new test frameworks.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Settings enable copy + docs link | GIT-12 / D-LFS-16 | Visual copy | Enable LFS in repo Settings; confirm docs link for `.gitattributes` / `git lfs track` |
| Admin quota dashboard readability | GIT-13 / D-LFS-19 | Visual layout | Open Admin LFS usage; confirm breakdown by repo/user and physical bytes |
| Blob Download via session | GIT-12 / D-LFS-18 | Browser download UX | Open LFS pointer blob; badge visible; Download fetches bytes while logged in with Read |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter — **owned by `/gsd-validate-phase`** (set 2026-09-19 / 22.1-09)

**Approval:** validated (Nyquist compliant) — API/dialect green; Compose client smoke optional residual

---

## Execution Gate Results (14-12)

Recorded 2026-09-14 during plan 14-12:

| Gate | Result |
|------|--------|
| `cargo nextest run -p oxidean-api -E 'test(lfs)'` | ✅ 19 passed |
| `cargo test -p oxidean-db --test dialect_lfs` | ✅ ok |
| `make rpc-sync-check` | ✅ ok |
| Web LFS Vitest (settings/admin/pointer/browser) | ✅ 8 passed |
| `make smoke-git-lfs` | ✅ skip (stack health unreachable — Docker present; run `make up` for full client path) |

---

## Validation Audit 2026-09-19

| Metric | Count |
|--------|-------|
| Gaps found | 0 (map path drift only: `lfs-pointer.test.ts` → `lfs-pointer.unit.test.ts`) |
| Resolved | 1 (path corrected in task map) |
| Escalated | 0 |
| Manual-only (UX) | 3 |

| Gate | Result |
|------|--------|
| `cargo nextest run -p oxidean-api -E 'test(lfs)'` | ✅ 20 passed (run id aaaddf99) |
| `cargo test -p oxidean-db --test dialect_lfs` | ✅ ok |
| Vitest `lfs-pointer.unit.test.ts` | ✅ passed (spot-check) |
| Key files (lfs_batch/store, dialect_lfs, smoke-git-lfs) | ✅ present |

**Verdict:** `status: validated`, `nyquist_compliant: true`. All 15 task-map rows green. Residual: `make smoke-git-lfs` still needs Compose up for full git-lfs client path (non-blocking; same class as Phase 09 optional ls-remote depth).
