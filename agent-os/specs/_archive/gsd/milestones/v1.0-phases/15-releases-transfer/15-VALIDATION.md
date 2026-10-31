---
phase: "15"
slug: "releases-transfer"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-09-19"
validated_at: "2026-09-19"
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `15-RESEARCH.md` Validation Architecture. Wave 0 closed by plan `15-00`.
> Nyquist reconcile via `/gsd-validate-phase` equivalent (22.1-09, 2026-09-19) — prior draft map still listed Wave 0 stubs as missing after execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(release) \| test(rename) \| test(transfer) \| test(redirect)'` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted |
| **Phase gate** | Quick run + `cargo test -p oxidean-db --test dialect_releases` + `make rpc-sync-check` + web Vitest releases/rename-transfer |

---

## Sampling Rate

- **Per task commit:** focused nextest filter + relevant Vitest file
- **Per wave merge:** `cargo nextest run -p oxidean-api -p oxidean-db` + web Vitest for touched routes
- **Phase gate:** Full automated gate green before `/gsd-verify-work`

---

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? | Status |
|--------|----------|-----------|-------------------|--------------|--------|
| GIT-14 | Create release for existing tag with notes + asset | API integration | `cargo nextest run -p oxidean-api -E 'test(release)'` | ✅ `release_rpc.rs` | ✅ green |
| GIT-14 | Reject missing tag (`release.tag_missing`) | API integration | same | ✅ | ✅ green |
| GIT-14 | Draft hidden from Read-only / anon | API integration | same | ✅ | ✅ green |
| GIT-15 | Download asset with Read / anon public published | API integration | same | ✅ | ✅ green |
| GIT-15 | Oversized upload rejected | API integration | same | ✅ | ✅ green |
| GIT-16 | Admin rename moves disk + DB; old path redirects | API integration | `cargo nextest run -p oxidean-api -E 'test(rename) \| test(redirect)'` | ✅ `repo_rename_transfer.rs` | ✅ green |
| GIT-16 | Non-admin rename → soft not_found | API integration | same | ✅ | ✅ green |
| GIT-17 | Admin transfer to user/org + type-confirm | API integration | `cargo nextest run -p oxidean-api -E 'test(transfer)'` | ✅ | ✅ green |
| GIT-17 | Issues/LFS associations remain on `repo_id` when tables exist | API integration | same | ✅ | ✅ green |
| GIT-16/17 | New repo at old path supersedes redirect | API integration | same | ✅ | ✅ green |
| UI | Releases tab + settings danger zone | web | `bunx vitest run src/routes/\$owner.\$repo.releases.integration.test.ts src/routes/\$owner.\$repo.settings.rename-transfer.integration.test.ts` | ✅ | ✅ green |
| Dialect | Migration parity for releases + redirects | DB | `cargo test -p oxidean-db --test dialect_releases` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/oxidean-api/tests/release_rpc.rs` — GIT-14/15 — **15-00 → greened**
- [x] `crates/oxidean-api/tests/repo_rename_transfer.rs` — GIT-16/17 + redirects — **15-00 → greened**
- [x] `crates/oxidean-db/tests/dialect_releases.rs` — tri-dialect migration parity — **15-00 → greened**
- [x] `apps/web/src/routes/$owner.$repo.releases.integration.test.ts` — tab/routes — greened
- [x] `apps/web/src/routes/$owner.$repo.settings.rename-transfer.integration.test.ts` — danger-zone rename/transfer confirm — greened

---

## Manual / UAT Backstops

- Create release for existing tag; upload asset; download from Releases UI
- Draft visible only to Write+; Admin deletes release
- Admin rename; old `/{owner}/{repo}` and `.git` URLs redirect within retention window
- Admin transfer with type-confirm to user and org; issues/LFS stay reachable on new owner path

---

## Validation Sign-Off

- [x] All requirements have automated verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 240s
- [x] `nyquist_compliant: true` set in frontmatter — **owned by `/gsd-validate-phase`** (set 2026-09-19 / 22.1-09)

**Approval:** validated (Nyquist compliant) — draft map reconciled to greened Wave 0 + live nextest/vitest

---

## Validation Audit 2026-09-19

| Metric | Count |
|--------|-------|
| Gaps found | 0 (stale draft map listed all Wave 0 rows as ❌; files existed) |
| Resolved | 12 (map rows flipped File Exists ✅ / Status green; Wave 0 checklist closed) |
| Escalated | 0 |
| Manual-only (UX) | 4 |

| Gate | Result |
|------|--------|
| `cargo nextest run -p oxidean-api -E 'test(release)\|test(rename)\|test(transfer)\|test(redirect)'` | ✅ 25 passed (run id e1a2ed5f) |
| `cargo test -p oxidean-db --test dialect_releases` | ✅ 2 passed |
| Vitest releases + rename-transfer integration | ✅ 6 passed (2 files) |
| Key files (release_rpc, repo_rename_transfer, dialect_releases, web routes) | ✅ present |

**Verdict:** `status: validated`, `nyquist_compliant: true`. `15-VERIFICATION.md` already `passed with caveats`; Nyquist sampling closed. Residual is browser UAT only.
