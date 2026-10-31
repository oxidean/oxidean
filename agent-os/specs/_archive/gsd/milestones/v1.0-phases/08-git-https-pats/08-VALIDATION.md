---
phase: "08"
slug: "git-https-pats"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-13"
planned: "2026-09-13"
verified_at: "2026-09-13"
updated: "2026-09-19"
validated_at: "2026-09-19"
---

# Phase 08 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Task map refreshed by **08-13-T2** (2026-09-13). Nyquist reconcile via `/gsd-validate-phase` equivalent (22.1-09, 2026-09-19).

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust: cargo-nextest / `cargo test`; Web: Vitest via Bun |
| **Config file** | workspace Cargo; `apps/web` Vitest (existing) |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(pat_)|test(git_smart)'` |
| **Full suite command** | `make test` (+ `make test-e2e-stack` / `scripts/smoke-git-https.sh` for Traefik/git client) |
| **Estimated runtime** | ~60–180 seconds (quick); full suite longer with e2e |

---

## Sampling Rate

- **After every task commit:** Run `cargo nextest run -p oxidean-api -E 'test(pat_)|test(git_smart)'` (or Vitest for UI plans)
- **After every plan wave:** Run `make test` + `make rpc-sync-check` (after 08-08)
- **Before `/gsd-verify-work`:** Full suite green + Smart HTTP e2e through Traefik
- **Max feedback latency:** 180 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 08-00-T1 | 00 | 0 | GIT-02, GIT-11 | T-08-01, T-08-02 | Wave 0 RED stubs for PAT + Smart HTTP | integration | `cargo nextest list -p oxidean-api -E 'test(pat_) \| test(git_smart)'` | ✅ | ✅ green |
| 08-01-T1 | 01 | 0 | GIT-02, GIT-11 | T-08-01, T-08-03 | Wave 0 web stubs tokens + how-to | component | `bun --cwd apps/web exec vitest run src/routes/settings/tokens.integration.test.ts src/components/repo/clone-box.pat.integration.test.ts` | ✅ | ✅ green |
| 08-02-T* | 02 | 1 | GIT-02, GIT-11 | T-08-04, T-08-05 | Human locks D-08/D-18/D-21 | checkpoint | DISCUSSION-LOG options recorded | n/a | ✅ green |
| 08-03-T1 | 03 | 2 | GIT-11 | T-08-01 | Hash-at-rest schema; no plaintext column | unit/integration | `cargo test -p oxidean-db --lib migration_parity && cargo test -p oxidean-db --test dialect_pats` | ✅ | ✅ green |
| 08-04-T1 | 04 | 3 | GIT-11 | T-08-01 | create/list/revoke; secret once | integration | `cargo nextest run -p oxidean-api -E 'test(pat_)'` | ✅ | ✅ green |
| 08-04-T2 | 04 | 3 | GIT-02 | T-08-02, T-08-07 | Password reject; cookie ignore; public fetch | integration | `cargo nextest run -p oxidean-api -E 'test(git_smart)'` | ✅ | ✅ green |
| 08-05-T1 | 05 | 4 | GIT-11 | T-08-06 | FG create + owned-repo bind | integration | `cargo nextest run -p oxidean-api -E 'test(pat_)'` | ✅ | ✅ green |
| 08-06-T1 | 06 | 4 | GIT-02 | T-08-02, T-08-05 | 401/403/push/last_used | integration | `cargo nextest run -p oxidean-api -E 'test(git_smart)'` | ✅ | ✅ green |
| 08-06-T2 | 06 | 4 | GIT-02 | T-08-08 | 429 rate limit; unverified push deny | integration | `cargo nextest run -p oxidean-api -E 'test(git_smart)'` | ✅ | ✅ green |
| 08-07-T1 | 07 | 5 | GIT-02 | T-08-09 | Traefik .git → API | smoke | `rg PathRegexp docker-compose.yml` + `scripts/smoke-git-https.sh` | ✅ | ✅ green |
| 08-08-T1 | 08 | 5 | GIT-11 | T-08-10 | rpc-gen; no Bearer claim | codegen | `make rpc-gen && make rpc-sync-check` | ✅ | ✅ green |
| 08-09-T1 | 09 | 6 | GIT-11 | T-08-01, T-08-03 | List/nav; Generate gate | component | `bun --cwd apps/web exec vitest run src/routes/settings/tokens.integration.test.ts` | ✅ | ✅ green |
| 08-09-T2 | 09 | 6 | GIT-11 | T-08-01 | Revoke confirm Keep token | component | same | ✅ | ✅ green |
| 08-10-T1 | 10 | 6 | GIT-11 | T-08-01, T-08-03 | Classic create + reveal | component | same + `bun --cwd apps/web run build` | ✅ | ✅ green |
| 08-11-T1 | 11 | 6 | GIT-11 | T-08-06 | FG create UI | component | same | ✅ | ✅ green |
| 08-12-T1 | 12 | 7 | GIT-02 | T-08-02 | CloneBox/QuickSetup how-to | component | `bun --cwd apps/web exec vitest run src/components/repo/clone-box.pat.integration.test.ts` | ✅ | ✅ green |
| 08-13-T1 | 13 | 7 | GIT-02, GIT-11 | T-08-10 | Operator docs Smart HTTP/PAT | docs | `rg -n 'Smart HTTP\|PathRegexp\|oxidean_pat_\|PUBLIC_ORIGIN' docs/CONFIGURATION.md docs/ARCHITECTURE.md` | ✅ | ✅ green |
| 08-13-T2 | 13 | 7 | GIT-02, GIT-11 | T-08-10 | Phase gate sweep | mixed | `cargo nextest … pat_\|git_smart` + migration_parity + `make rpc-sync-check` + web build | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### 08-13-T2 gate results (2026-09-13)

| Command | Result |
|---------|--------|
| `cargo nextest run -p oxidean-api -E 'test(pat_) \| test(git_smart)'` | ✅ 18 passed |
| `cargo test -p oxidean-db --lib migration_parity` | ✅ ok |
| `make rpc-sync-check` | ✅ ok |
| `bun run build` (apps/web) | ✅ built |

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/pat_rpc.rs` — stubs for GIT-11 (create/list/revoke, email verified) — **08-00** → greened **08-04/08-05**
- [x] `crates/oxidean-api/tests/git_smart_http.rs` — stubs for GIT-02 (anon public, private 401, PAT push, password reject, cookie ignore, scope 403, rate limit 429, unverified push) — **08-00** → greened **08-04/08-06**
- [x] `crates/oxidean-db/migrations/*/0008_pats.sql` + parity — stub **08-00** / green **08-03**
- [x] `apps/web` Vitest stubs for `/settings/tokens` and CloneBox how-to — **08-01** → greened **08-09…08-12**
- [x] Compose Traefik `PathRegexp` for `.git` + smoke for `git ls-remote` / `git push` — **08-07**

*Existing nextest/Vitest/`make test` infrastructure covers runners; Wave 0 stubs shipped then filled by later plans.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| One-time plaintext PAT reveal + copy | GIT-11 | Browser clipboard / reveal UX | Create token in UI; confirm secret shown once; refresh list — secret gone |
| Confirm dialog before revoke | GIT-11 | Dialog interaction | Revoke flow shows confirm; Keep token leaves token; confirm removes it |
| Clone-box how-to panel readability | GIT-02 | Visual copy | Empty repo / clone box shows username + PAT-as-password + CTA |
| Long-note / long-URL backstops | GIT-11/02 | Visual overflow | List ellipsis+title; FG name truncate; revoke wrap; how-to URL overflow |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter — **owned by `/gsd-validate-phase`** (set 2026-09-19 / 22.1-09)

**Approval:** validated (Nyquist compliant) — execution gates green 2026-09-13; re-run 2026-09-19; UAT/manual rows remain backstops

---

## Validation Audit 2026-09-19

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
| Manual-only (UX) | 4 |

| Gate | Result |
|------|--------|
| `cargo nextest run -p oxidean-api -E 'test(pat_)\|test(git_smart)'` | ✅ 26 passed (run id 82e9814d) |
| `cargo test -p oxidean-db --lib migration_parity` | ✅ ok |
| Vitest `tokens.integration.test.ts` (+ LFS pointer spot) | ✅ 17 passed (shared spot-check) |
| Key files (pat_rpc, git_smart_http, smoke-git-https) | ✅ present |

**Verdict:** `status: validated`, `nyquist_compliant: true`. All 18 task-map rows green; Wave 0 closed; no MISSING automated reqs. Residual risk is browser UAT only (PAT reveal/revoke/CloneBox copy).
