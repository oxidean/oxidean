---
phase: "10"
slug: "orgs-permissions"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-10-02"
validated_at: "2026-10-02"
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `10-RESEARCH.md` Validation Architecture. Wave 0 gaps closed by plans 00–13 + 12 closeout.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(org_) | test(collab) | test(repo_private) | test(git_smart) | test(pat_) | test(coalesce)'` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted |
| **Phase gate (10-12)** | Quick run + `cargo test -p oxidean-db --lib migration_parity` + `make rpc-sync-check` + `bun --cwd apps/web run build` |

---

## Sampling Rate

- **Per task commit:** focused nextest filter + relevant Vitest file
- **Per wave merge:** `make test`
- **Phase gate:** Full automated gate green before `/gsd-verify-work`; dialect migration smoke via `migration_parity`

---

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ORG-01 | Create org; add member by username; email invite accept | API integration | `cargo nextest run -p oxidean-api -E 'test(org_)'` | ✅ |
| ORG-02 | Owner/Admin admin; Member respects member_base none/read/write | unit + API | `test(coalesce)` + org ACL integration | ✅ |
| ORG-03 | Collaborator CRUD on personal + org repos; visibility admin-gated | API + Vitest | `test(collab)` + settings collaborators Vitest | ✅ |
| ORG-04 | Private non-grantee → web `repo.not_found`; git → 401; push denied without write | API | `test(repo_private)` / `test(git_smart)` | ✅ |
| ORG-04 | PAT collaborator push with classic `repo` scope | API | `test(pat_)` / smart http | ✅ |
| ORG-01 | Username lookup rate/limit shape | API | `cargo nextest run -p oxidean-api -E 'test(user_lookup)'` | ✅ |
| ORG-01/03 | `/orgs/new`, owner picker, collaborators UI | Vitest | `orgs.new` / `new.owner-picker` / collaborators integration | ✅ |
| ORG-* | Factory reset wipes org ACL + repos | DB + API | `cargo test -p oxidean-db --test factory_reset_orgs` + `test(factory_reset)` | ✅ |

---

## Wave 0 Gaps

- [x] `crates/oxidean-api/tests/org_create_members.rs` — ORG-01/02
- [x] `crates/oxidean-api/tests/org_invites.rs` — email invite + closed signup
- [x] `crates/oxidean-api/tests/repo_collaborators_acl.rs` — ORG-03/04 matrix
- [x] Extend `repo_private_404.rs` + `git_smart_http.rs` for collaborator/org Member cases
- [x] Extend PAT authorize tests for non-owner collaborator
- [x] `apps/web` integration tests for `/orgs/new`, owner picker, collaborators panel
- [x] Factory reset coverage for org tables
- [x] Unit tests in `acl.rs` for coalesce matrix

---

## Manual / UAT Backstops

- Create org; add member via live username lookup; email invite accept
- member_base_permission none/read/write behavior on private org repo
- Collaborator read/write/admin on personal and org-owned repo
- Unauthorized cannot see private repo in UI; git gets 401

---

## Validation Audit 2026-10-02

Retroactive Nyquist audit (ROADMAP DEBT-08) — same class of reconcile as `22.1-09` for phases 8/14/15/20. Scope: confirm every mapped command still names real files/tests on disk and that recorded green runs exist; suites not re-executed in this docs pass.

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 1 (`status: complete` → `validated`; stale lifecycle value reconciled) |
| Escalated | 0 |
| Manual-only (UAT backstops) | 4 (unchanged; documented above) |

| Check | Result |
|-------|--------|
| `crates/oxidean-api/tests/org_create_members.rs` (`org_create_*`, `org_members_*`, `org_member_base_*`, `org_lookup_shape_prefix_limit_no_email`) | ✅ present; named tests exist |
| `crates/oxidean-api/tests/org_invites.rs` (`org_invites_*`) | ✅ present; named tests exist |
| `crates/oxidean-api/tests/repo_collaborators_acl.rs` (`collab_*`) | ✅ present; named tests exist |
| `crates/oxidean-api/tests/repo_private_404.rs` + `git_smart_http.rs` + `pat_*` | ✅ present; org/collab soft-404 + 401 + PAT push cases exist |
| `crates/oxidean-api/tests/user_lookup.rs` (`user_lookup_*`) | ✅ present (shipped with admin-users follow-up) |
| `crates/oxidean-api/src/repo/acl.rs` `coalesce_*` unit tests | ✅ present (8 cases incl. `coalesce_collaborator_raises_member_base_none`) |
| `crates/oxidean-db/tests/factory_reset_orgs.rs` + `factory_reset_scope.rs` org rows | ✅ present (`factory_reset_wipes_org_acl_and_repository_rows`) |
| Web: `orgs.new`, `new.owner-picker`, `settings.collaborators` integration tests | ✅ present under `apps/web/src/routes/` |
| Recorded green runs | `10-VERIFICATION.md` behavioral spot-checks PASS (verified 2026-09-14; fingerprint refresh 2026-09-19), `status: passed` |

**Verdict:** `status: validated`, `nyquist_compliant: true`. Prior `complete`/`executed` lifecycle value was why audit-milestone §5.5 bucketed this phase NOT-VALIDATED; map content already matched disk.
