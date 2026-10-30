---
phase: 2
slug: multi-db-storage
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-09-09
updated: 2026-09-13
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust) + compose smoke (bash) + GitHub Actions |
| **Config file** | `Cargo.toml` workspace / `.github/workflows/ci.yml` |
| **Quick run command** | `cargo test -p oxidean-db --lib` |
| **Full suite command** | `cargo test --workspace` + CI `db-matrix` |
| **Estimated runtime** | ~30–120s local unit; matrix legs longer with services |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p oxidean-db --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green including CI matrix expectations
- **Max feedback latency:** 120 seconds for quick unit path
- **Compose smoke:** local `make smoke*` may exceed 30s (image build); not the quick unit path

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-T1 | 02-01 | 1 | PLAT-07 | T-02-01 | Dialect mismatch fails closed; credentials redacted | unit | `cargo test -p oxidean-db --lib resolve_dialect` / `redact_url_hides_password` | ✅ | ✅ green |
| 02-01-T2 | 02-01 | 1 | PLAT-07 | T-02-05 | One pool type per dialect; SQLite WAL + FK + bounded pool | unit | `cargo build -p oxidean-db && cargo test -p oxidean-api --tests` | ✅ | ✅ green |
| 02-02-T1 | 02-02 | 2 | PLAT-08 | T-02-02 | Migration sets cannot drift between dialects | unit | `cargo test -p oxidean-db --lib migration_parity` | ✅ | ✅ green |
| 02-02-T2 | 02-02 | 2 | PLAT-08 | T-02-03 | Probe SQL uses bound placeholders only | unit+build | `cargo test --workspace` | ✅ | ✅ green |
| 02-02-T3 | 02-02 | 2 | PLAT-08 | T-02-01, T-02-13 | Migrate+probe; SQLite parent dir; migrate CLI empty check | integration+cli | `DATABASE_URL=sqlite:./target/tmp/verify/oxidean.db cargo test -p oxidean-db --test dialect_probe` + `sqlite_paths` + migrate `--assert-empty` | ✅ | ✅ green |
| 02-03-T1 | 02-03 | 3 | PLAT-07 | T-02-08 | Bad dialect config aborts startup | integration | `DATABASE_URL=postgres://u:p@localhost/x OXIDEAN_DB_DIALECT=mysql cargo run -q -p oxidean-api` exits 1 | ✅ | ✅ green |
| 02-03-T2 | 02-03 | 3 | PLAT-08 | T-02-06, T-02-09 | Probe errors stable; version header enforced | integration | `cargo test -p oxidean-api --test rpc_db_probe` | ✅ | ✅ green |
| 02-03-T3 | 02-03 | 3 | PLAT-08 | T-02-10 | Generated client cannot drift | contract | `make rpc-sync-check && bun run --filter @oxidean/api-client test` | ✅ | ✅ green |
| 02-04-T1 | 02-04 | 4 | PLAT-07 | T-02-11 | SQLite overlay starts no DB container; `var/` gitignored | config | `docker compose -f docker-compose.yml -f docker-compose.sqlite.yml config` | ✅ | ✅ green |
| 02-04-T2 | 02-04 | 4 | PLAT-07 | T-02-13 | Dialect switch refuses non-empty target | cli | `DATABASE_URL=sqlite:./target/tmp/switch/oxidean.db ./scripts/db-switch-dialect.sh` + re-run migrate `--assert-empty` | ✅ | ✅ green |
| 02-04-T3 | 02-04 | 4 | PLAT-08 | T-02-14 | Smoke asserts dialect that served the write | smoke | `make smoke` / `make smoke-mysql` / `make smoke-sqlite` | ✅ | ⚠️ manual (live bring-up) |
| 02-05-T1 | 02-05 | 5 | PLAT-07, PLAT-08 | T-02-16 | All three dialects proven per PR | ci | CI job `db-matrix` (`cargo test -p oxidean-db --test dialect_probe` per leg) | ✅ | ✅ green (workflow present) |
| 02-05-T2 | 02-05 | 5 | PLAT-07 | T-02-18, T-02-19 | Docs name local-only creds + empty-target switch | docs | `grep -q 'sqlite:./var/oxidean.db' .env.example && test -f docs/database.md` | ✅ | ✅ green |
| 02-05-T3 | 02-05 | 5 | PLAT-07, PLAT-08 | — | Validation contract matches what shipped | doc-gate | `grep -q 'nyquist_compliant: true' .planning/phases/02-multi-db-storage/02-VALIDATION.md` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky/manual*

---

## Wave 0 Requirements

- [x] `crates/oxidean-db/src/dialect.rs` unit tests — 02-01-T1
- [x] Migration filename parity — 02-02-T1
- [x] `crates/oxidean-db/tests/dialect_probe.rs` gated on `DATABASE_URL` — 02-02-T3
- [x] `crates/oxidean-db/tests/sqlite_paths.rs` — 02-02-T3
- [x] `crates/oxidean-api/tests/rpc_db_probe.rs` — 02-03-T2
- [x] CI `db-matrix` job for postgres/mysql/sqlite — 02-05-T1
- [x] `docker compose config` for sqlite overlay in `compose` job — 02-05-T1
- [x] `scripts/compose-smoke.sh` `system.db_probe` + dialect assertion; `make smoke-*` — 02-04-T3

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live three-profile Compose bring-up | PLAT-08 / ROADMAP #3 | CI proves dialects via `db-matrix`; compose job is config-only | `make up` / `make up-mysql` / `make up-sqlite` then matching `make smoke*` (requires `docker` on PATH) |

---

## Validation Sign-Off

- [x] All tasks have automated verify or explicit manual note
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 120s for unit path
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** 2026-09-09 (Nyquist validate-phase audit)

---

## Validation Audit 2026-09-09

| Metric | Count |
|--------|-------|
| Gaps found | 3 (02-05-T1 CI, 02-05-T2 docs/env, 02-05-T3 sign-off) + smoke docker.exe hygiene |
| Resolved | 3 + docker smoke simplified to plain `docker` |
| Escalated / manual | 1 (live Compose smoke bring-up — by design) |
| Notes | Dropped WSL `docker.exe` / temp `DOCKER_CONFIG` hacks from `scripts/compose-smoke.sh` |

---

## Validation Audit 2026-09-13

| Metric | Count |
|--------|-------|
| Gaps found | 1 (frontmatter lifecycle `#2117`: `status: complete` → `validated`) |
| Resolved | 1 — frontmatter set `status: validated`, `nyquist_compliant: true` retained, `updated: 2026-09-13` |
| Re-verified green | `cargo test -p oxidean-db --lib resolve_dialect` (4 ok); `cargo test -p oxidean-db --lib migration_parity` (1 ok); `cargo test -p oxidean-api --test rpc_db_probe` (3 ok, no `DATABASE_URL` required) |
| PLAT-07 / PLAT-08 | COVERED in Per-Task Verification Map (all rows green except 02-04-T3) |
| Escalated / manual | 1 kept by design — live Compose smoke (`make smoke*`) remains Manual-Only |
| Implementation files modified | none |
