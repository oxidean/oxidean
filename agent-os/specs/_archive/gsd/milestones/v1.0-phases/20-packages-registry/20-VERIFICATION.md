---
phase: 20-packages-registry
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: passed with caveats — OCI referrers deferred; packages chrome/IA + stack-browser + CI smoke closed in Phase 11.1 — Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 7/7 must-haves verified
covered_files:
  - .planning/phases/20-packages-registry/20-00-PLAN.md
  - .planning/phases/20-packages-registry/20-00-SUMMARY.md
  - .planning/phases/20-packages-registry/20-01-PLAN.md
  - .planning/phases/20-packages-registry/20-01-SUMMARY.md
  - .planning/phases/20-packages-registry/20-02-PLAN.md
  - .planning/phases/20-packages-registry/20-02-SUMMARY.md
  - .planning/phases/20-packages-registry/20-03-PLAN.md
  - .planning/phases/20-packages-registry/20-03-SUMMARY.md
  - .planning/phases/20-packages-registry/20-04-PLAN.md
  - .planning/phases/20-packages-registry/20-04-SUMMARY.md
  - .planning/phases/20-packages-registry/20-05-PLAN.md
  - .planning/phases/20-packages-registry/20-05-SUMMARY.md
  - .planning/phases/20-packages-registry/20-06-PLAN.md
  - .planning/phases/20-packages-registry/20-06-SUMMARY.md
  - .planning/phases/20-packages-registry/20-07-PLAN.md
  - .planning/phases/20-packages-registry/20-07-SUMMARY.md
  - .planning/phases/20-packages-registry/20-08-PLAN.md
  - .planning/phases/20-packages-registry/20-08-SUMMARY.md
  - .planning/phases/20-packages-registry/20-09-PLAN.md
  - .planning/phases/20-packages-registry/20-09-SUMMARY.md
  - .planning/phases/20-packages-registry/20-10-PLAN.md
  - .planning/phases/20-packages-registry/20-10-SUMMARY.md
  - .planning/phases/20-packages-registry/20-11-PLAN.md
  - .planning/phases/20-packages-registry/20-11-SUMMARY.md
  - .planning/phases/20-packages-registry/20-12-PLAN.md
  - .planning/phases/20-packages-registry/20-12-SUMMARY.md
  - .planning/phases/20-packages-registry/20-CONTEXT.md
  - .planning/phases/20-packages-registry/20-VALIDATION.md
  - apps/web/src/components/packages/delete-version-dialog.tsrx
  - apps/web/src/components/settings/pat-classic-form.tsrx
  - apps/web/src/components/settings/pat-fg-form.tsrx
  - apps/web/src/lib/package-quota-copy.ts
  - apps/web/src/routes/$owner.$repo.packages.integration.test.ts
  - apps/web/src/routes/$owner.$repo.packages.tsrx
  - apps/web/src/routes/$owner.packages.integration.test.ts
  - apps/web/src/routes/$owner.packages.tsrx
  - apps/web/src/routes/admin/packages.integration.test.ts
  - apps/web/src/routes/admin/packages.tsrx
  - apps/web/src/routes/settings/tokens.packages.integration.test.ts
  - apps/web/vite.config.ts
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/jobs/schedule.rs
  - crates/oxidean-api/src/packages/acl.rs
  - crates/oxidean-api/src/packages/auth.rs
  - crates/oxidean-api/src/packages/generic.rs
  - crates/oxidean-api/src/packages/mod.rs
  - crates/oxidean-api/src/packages/npm.rs
  - crates/oxidean-api/src/packages/oci.rs
  - crates/oxidean-api/src/packages/quota.rs
  - crates/oxidean-api/src/packages/rpc.rs
  - crates/oxidean-api/src/packages/store.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/tests/generic_registry.rs
  - crates/oxidean-api/tests/npm_registry.rs
  - crates/oxidean-api/tests/oci_registry.rs
  - crates/oxidean-api/tests/package_acl.rs
  - crates/oxidean-api/tests/package_gc.rs
  - crates/oxidean-api/tests/package_quota.rs
  - crates/oxidean-api/tests/package_rpc.rs
  - crates/oxidean-core/src/package_types.rs
  - crates/oxidean-core/src/pat_types.rs
  - crates/oxidean-db/migrations/mysql/0015_packages.sql
  - crates/oxidean-db/migrations/postgres/0015_packages.sql
  - crates/oxidean-db/migrations/sqlite/0015_packages.sql
  - crates/oxidean-db/src/packages.rs
  - crates/oxidean-db/tests/dialect_packages.rs
  - docker-compose.yml
  - docs/API.md
  - docs/CONFIGURATION.md
  - packages/api-client/src/index.ts
  - scripts/smoke-packages.sh
covered_digest: "v1:sha256:9e3b795de526663875aae55c984df1c09f5b10fb1781ca230e5d3346a6f2960e"
behavior_unverified: 0
overrides_applied: 0
honesty_annotated: 2026-09-15
decision_coverage: "{'honored': 16, 'total': 16, 'not_honored': []}"
re_verification: "{'previous_status': 'gaps_found', 'previous_score': '6/7', 'gaps_closed': ['Repo packages page lists packages linked to that repository (D-PKG-11)'], 'gaps_remaining': [], 'regressions': []}"
advisory: "['Repo packages chrome / Packages tab IA — CLOSED in Phase 11.1-01/03/06 (D-QH-01)', 'OCI referrers_deferred (404) — documented Phase 20 deferral; out of 11.1 scope', 'Packages list stack-browser — CLOSED in Phase 11.1-04 (forge-repo / forge-packages-ssh-orgs)', 'CI smoke-packages — CLOSED in Phase 11.1-05 (smoke-protocol job)']"
---

# Phase 20: Packages Registry Verification Report

**Phase Goal:** Users can publish and pull OCI, npm, and generic/raw packages scoped to repo/org with the same auth/visibility rules  
**Verified:** 2026-09-14T18:10:12Z  
**Status:** passed (with caveats — OCI referrers deferred; chrome/IA + packages e2e/smoke closed in 11.1)  
**Honesty annotate:** 2026-09-15 — residual UI/IA; 11.1-05 closeout marks chrome + stack-browser + CI smoke closed  
**Re-verification:** Yes — after gap closure (`fccad92`)  
**Worktree:** `/home/jesse/wsl-projects/personal/typescript/oxidean-wt-phase20` (`feat/execute-20-packages`)  
**Migration:** `0015_packages` (postgres/sqlite/mysql) — confirmed; not 0012/0013

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can publish and pull OCI container images from an instance registry scoped to a repo or org | ✓ VERIFIED | Prior pass + regression: `oci.rs` mounted; artifacts present (sanity) |
| 2 | User can publish and pull npm packages and generic/raw packages from an instance registry scoped to a repo or org | ✓ VERIFIED | Prior pass + regression: `npm.rs` / `generic.rs` present |
| 3 | Registry packages respect the same auth/visibility rules as their owning repo/org | ✓ VERIFIED | Prior pass: `acl.rs` hybrid ACL∩PAT |
| 4 | User can list and delete package versions they are permitted to manage | ✓ VERIFIED | Prior pass: `packages.list` / `deleteVersion` RPC + owner UI |
| 5 | Repo packages page lists packages linked to that repository (D-PKG-11) | ✓ VERIFIED | `$owner.$repo.packages.tsrx`: `repo.get` → `packagesListQueryOptions({ repository_id })`; old owner-wide soft-filter removed; Vitest `?raw` asserts `repo.get` + `repository_id` and rejects owner-only list; `package_rpc_list_by_repo_link` ok |
| 6 | Uploads exceeding max blob / owner quota rejected; Admin quota RPC; GC keeps live refs | ✓ VERIFIED | Prior pass: quota + GC tests / schedule |
| 7 | Docs describe `/v2` `/npm` `/generic`, PACKAGES_DIR, PAT scopes; edge smoke skip-ok | ✓ VERIFIED | Prior pass: docs + Compose/Vite + `smoke-packages.sh` |

**Score:** 7/7 truths verified (0 present, behavior-unverified)

### Advisory (New Scope, Unevidenced)

None — re-verification Step 7 found no new-scope unevidenced blockers.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/oxidean-db/migrations/*/0015_packages.sql` | Schema packages/versions/blobs/refs/quotas | ✓ VERIFIED | All three dialects present |
| `crates/oxidean-api/src/packages/{oci,npm,generic,acl,auth,store,rpc,quota}.rs` | Protocol + shared subsystem | ✓ VERIFIED | Substantive; mounted in `app.rs` |
| `crates/oxidean-api/tests/{oci,npm,generic}_registry.rs` + `package_*.rs` | Behavioral coverage | ✓ VERIFIED | Named RPC list-by-repo test re-run green |
| `apps/web/src/routes/$owner.packages.tsrx` | Owner list + delete | ✓ VERIFIED | Query + mutation wired |
| `apps/web/src/routes/$owner.$repo.packages.tsrx` | Repo-linked list | ✓ VERIFIED | Resolves repo id then lists by `repository_id` |
| `apps/web/src/components/packages/delete-version-dialog.tsrx` | Type-to-confirm | ✓ VERIFIED | Prior pass |
| `apps/web/src/routes/admin/packages.tsrx` | Admin quota UI | ✓ VERIFIED | Prior pass |
| `apps/web/src/components/settings/pat-*-form.tsrx` | package:read/write + FG | ✓ VERIFIED | Prior pass |
| `scripts/smoke-packages.sh` | Edge routing smoke | ✓ VERIFIED | Skip-ok without Docker/Compose |
| `packages/api-client` packages.* | Generated RPC client | ✓ VERIFIED | `packagesListQueryOptions` accepts `PackagesListRequest` incl. `repository_id` |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `app.rs` | `packages::{oci,npm,generic}::router` | nest `/v2` `/npm` `/generic` | WIRED | Prior pass |
| Protocol handlers | `acl` + `store` + `quota` | authorize + CA commit + check_can_store | WIRED | Prior pass |
| `rpc.rs` dispatch | `packages::rpc::*` | `packages.list` / `deleteVersion` / admin | WIRED | `list` branches on `repository_id` → `list_packages_by_repository` |
| Owner packages UI | `@oxidean/api-client` | `packagesListQueryOptions` / delete mutation | WIRED | Prior pass |
| Repo packages UI | `packages.list` by `repository_id` | `repo.get` → id → list | WIRED | Fixed in `fccad92` |
| Traefik / Vite | API | PathPrefix / proxy | WIRED | Prior pass |
| GC job | `quota::gc_unref_blobs` | schedule interval | WIRED | Prior pass |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| `$owner.packages.tsrx` | `packages` | `packages.list` RPC → DB | Yes | ✓ FLOWING |
| `$owner.$repo.packages.tsrx` | `packages` | `repo.get` → `packages.list({ repository_id })` → DB | Yes | ✓ FLOWING |
| `admin/packages.tsrx` | `usageQ.data` | `packages.adminUsage` | Yes when lookup set | ✓ FLOWING |
| OCI/npm/generic GET | blob/tarball bytes | CA store + DB refs | Yes (integration tests) | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------- |
| Repo UI scopes by repository_id | `bunx vitest run src/routes/$owner.$repo.packages.integration.test.ts` | 2 passed | ✓ PASS |
| RPC list by repo | `cargo test -p oxidean-api --test package_rpc package_rpc_list_by_repo_link -- --exact` | ok | ✓ PASS |
| Migration id | `ls …/0015_packages.sql` (3 dialects) | present | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| N/A | — | No `scripts/*/tests/probe-*.sh` declared for this phase | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| PKG-01 | 00,05,12 | OCI publish/pull | ✓ SATISFIED | oci handlers + tests (prior) |
| PKG-02 | 00,06,07,12 | npm publish/pull (+ dist-tags/search) | ✓ SATISFIED | npm handlers + tests (prior) |
| PKG-03 | 00,04,12 | generic/raw | ✓ SATISFIED | generic handlers + tests (prior) |
| PKG-04 | 00,02,03,05,11 | Auth/visibility hybrid | ✓ SATISFIED | acl + PAT scopes + tokens UI (prior) |
| PKG-05 | 00,08,10,11 | List/delete manage | ✓ SATISFIED | Owner + repo-scoped list (`repository_id`); delete RPC/UI |

Orphaned requirements mapped to Phase 20 but unclaimed by plans: none.

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts (16/16). Gate non-blocking.

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `package_rpc.rs` | PKG-05 | 6 | 0 | 0 | Behavioral | PASS |
| `$owner.$repo.packages.integration.test.ts` | PKG-05 | 2 | 0 | 0 | Value (`?raw` source contract) | PASS (gap closed) |
| Other registry/acl/quota/web files | PKG-01…05 | — | 0 | 0 | Prior audit | Unchanged |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Insufficient assertions:** remaining web typeof-only files are WARNING only (not blocking; not the prior gap)

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `dialect_packages.rs` | ~27 | Assert message says `"0012 must define {}"` while checking `0015_packages.sql` | ℹ️ Info / 📋 Advisory-eligible | Misleading failure text only; predates gap closure; not in prior `gaps:`; no regression evidence |

No unresolved `TBD`/`FIXME`/`XXX` debt markers in gap-fix files. Prior D-PKG-11 wrong-filter blocker is closed.

### Human Verification Required

(None for status.) Optional edge UAT remains documented in `20-VALIDATION.md` (docker/npm client flows, delete confirm UX, admin quota page) — not required to close must-haves; protocol truths already covered by Rust integration tests.

### Gaps Summary

Prior gap (D-PKG-11 repo packages page listing by owner instead of `repository_id`) is closed in `fccad92`: page resolves `repo.get` then calls `packages.list({ repository_id })`; Vitest enforces the wiring via `?raw`. All 7 must-have **protocol / list-wiring** truths verified — phase registry goal achieved for publish/pull/ACL/quota.

**Residual product gaps remain** (non-blocking for those truths; do not re-mark phase failed). See **Known stubs / residual gaps** below. Repo chrome / Packages tab IA is owned by **Phase 11.1** (`D-QH-01`), not a Phase 20 reopen.

### Known stubs / residual gaps

Audit source: `tmp/issue-3-quality-audit.md` (Issue #3) + Phase 11.1 context `D-QH-01` / `D-QH-05`. These did **not** fail the 7 must-have truths above; they are honesty footnotes so “passed” is not read as full UX/discovery complete.

| Gap | Pointers | Owner / disposition |
| --- | -------- | ------------------- |
| Repo packages chrome / Packages tab IA | Layout `RepoChrome` + Packages tab + content-only leaves (`11.1-01` / `03` / `06`) | **CLOSED** — D-QH-01 |
| Owner vs repo packages chrome inconsistency | Shared layout chrome for `$owner.$repo.packages`; owner packages page remains owner-scoped | **CLOSED** for repo shell; owner page intentional scope |
| OCI referrers deferred | `referrers_deferred` in `crates/oxidean-api/src/packages/oci.rs` (route returns 404; clients use referrers tag schema) | Documented Phase 20 deferral (`20-RESEARCH` / `20-05-SUMMARY`); **still open / out of 11.1 scope** |
| Stack-browser e2e for packages | `forge-repo.stack.browser.test.tsx` Packages tab / list (11.1-04); CI `smoke-packages` via `smoke-protocol` (11.1-05) | **CLOSED** for list/discovery + protocol routing |

---

_Verified: 2026-09-14T18:10:12Z_  
_Verifier: Claude (gsd-verifier)_  
_Honesty annotate: 2026-09-15 (Phase 11.1 GSD truth pass)_  
_Residual 11.1-05: packages chrome + stack-browser + CI smoke closed; OCI referrers remain deferred_

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 71 files

