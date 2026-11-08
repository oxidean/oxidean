---
phase: 15-releases-transfer
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Passed with documented caveats (see report body). — Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 3/3 must-haves verified
covered_files:
  - .env.example
  - .planning/REQUIREMENTS.md
  - .planning/phases/15-releases-transfer/15-00-PLAN.md
  - .planning/phases/15-releases-transfer/15-00-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-01-PLAN.md
  - .planning/phases/15-releases-transfer/15-01-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-02-PLAN.md
  - .planning/phases/15-releases-transfer/15-02-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-03-PLAN.md
  - .planning/phases/15-releases-transfer/15-03-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-04-PLAN.md
  - .planning/phases/15-releases-transfer/15-04-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-05-PLAN.md
  - .planning/phases/15-releases-transfer/15-05-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-06-PLAN.md
  - .planning/phases/15-releases-transfer/15-06-SUMMARY.md
  - .planning/phases/15-releases-transfer/15-CONTEXT.md
  - .planning/phases/15-releases-transfer/15-VALIDATION.md
  - apps/web/src/components/repo/repo-chrome.tsrx
  - apps/web/src/routes/$owner.$repo.releases.$tag.tsrx
  - apps/web/src/routes/$owner.$repo.releases.integration.test.ts
  - apps/web/src/routes/$owner.$repo.releases.new.tsrx
  - apps/web/src/routes/$owner.$repo.releases.tsrx
  - apps/web/src/routes/$owner.$repo.settings.rename-transfer.integration.test.ts
  - apps/web/src/routes/$owner.$repo.settings.tsrx
  - crates/oxidean-api/src/app.rs
  - crates/oxidean-api/src/auth/admin.rs
  - crates/oxidean-api/src/jobs/reconcile.rs
  - crates/oxidean-api/src/release/mod.rs
  - crates/oxidean-api/src/repo/acl.rs
  - crates/oxidean-api/src/repo/rename_transfer.rs
  - crates/oxidean-api/src/routes/git_smart_http.rs
  - crates/oxidean-api/src/routes/release_assets.rs
  - crates/oxidean-api/src/ssh/pack.rs
  - crates/oxidean-api/tests/release_rpc.rs
  - crates/oxidean-api/tests/repo_rename_transfer.rs
  - crates/oxidean-db/migrations/mysql/0014_releases_redirects.sql
  - crates/oxidean-db/migrations/postgres/0014_releases_redirects.sql
  - crates/oxidean-db/migrations/sqlite/0014_releases_redirects.sql
  - crates/oxidean-db/src/redirects.rs
  - crates/oxidean-db/src/releases.rs
  - crates/oxidean-db/tests/dialect_releases.rs
  - docker-compose.yml
  - docs/API.md
  - docs/CONFIGURATION.md
  - packages/api-client/src/index.ts
covered_digest: "v1:sha256:0e29b43cbe6a606ea341c497cd1ea617dbc60c64e2eb63fcfd808aaefae0bc23"
behavior_unverified: 0
overrides_applied: 0
decision_coverage: "{'honored': 13, 'total': 13, 'not_honored': []}"
human_verification: "[{'test': 'Open a repo with Write+: create a release for an existing tag, upload an asset, download it. As Admin: rename and transfer with type-confirm; confirm old URL redirects.', 'expected': 'Release notes + assets work end-to-end in the browser; rename/transfer update owner/name and old /{owner}/{repo} still resolves within retention.', 'why_human': 'Harvested from 15-05-PLAN.md <human-check>; browser UX, redirects, and download feel cannot be certified by unit/API tests alone.'}]"
---

# Phase 15: Releases & Transfer Verification Report

**Phase Goal:** Users can ship tagged releases with assets and rename or transfer repositories they administer

**Verified:** 2026-09-14T17:54:05Z

**Status:** passed with caveats

**Re-verification:** No — initial verification

**Worktree:** `/home/jesse/wsl-projects/personal/typescript/oxidean-wt-15-rel` @ `feat/execute-15-releases-cont`

**Migration lock:** `0014_releases_redirects` present on sqlite/postgres/mysql (not renumbered).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can create a release for a tag with notes and downloadable assets, and download those assets from the web UI | ✓ VERIFIED | `release.create` tag-bound + notes (`release_create_existing_tag_with_notes` ok); multipart upload + `GET /api/releases/assets/{id}` (`release_asset_upload_download_acl` ok); UI list/create/detail + file upload + download `href` in `$owner.$repo.releases*.tsrx`; chrome Releases tab |
| 2 | User with permission can rename a repository | ✓ VERIFIED | `repo.rename` Admin gate, bare `fs::rename`, DB name update, redirect insert (`repo_rename_admin_moves_disk_and_inserts_redirect` ok); non-admin soft `repo.not_found`; Settings Danger zone `apiClient.repo.rename` |
| 3 | User with permission can transfer a repository to another user or organization | ✓ VERIFIED | `repo.transfer` type-confirm + user/org dest (`repo_transfer_admin_to_user_or_org_with_confirm` ok); issues/LFS stay on `repo_id` (`repo_transfer_cascade_issues_lfs_by_repo_id` ok); Settings transfer AlertDialog |

**Score:** 3/3 truths verified (0 present, behavior-unverified)

### Supporting behaviors (plan must-haves — spot-checked)

| Behavior | Evidence | Status |
| --- | --- | --- |
| Tag missing → `release.tag_missing`; drafts Write+-only | `release/mod.rs` + `release_tag_missing_*` / `release_draft_hidden_*` tests present | ✓ |
| Assets on `OXIDEAN_RELEASE_ASSETS_DIR/{asset_id}` ≠ LFS; max size | `asset_fs_path`, Compose bind, `release_asset_size_reject` | ✓ |
| Redirect retention + purge; live path supersedes | `lookup_repo_row_or_redirect`, Smart HTTP + SSH; `redirect_purge_expired_rows` ok | ✓ |
| Factory reset wipes release-assets children | `auth/admin.rs` `wipe_dir_contents(&ctx.release_assets_dir, …)` + unit wipe test | ✓ |
| Migration `0014_releases_redirects` tri-dialect | sqlite/postgres/mysql files; `dialect_releases_*` ok | ✓ |

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts (13/13). D-REL-01…13 reflected in RPC, assets volume, rename/transfer, and Releases chrome.

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/oxidean-api/src/release/mod.rs` | release.* RPC | ✓ VERIFIED | create/list/get/update/delete + delete_asset; ACL via `meets(Write)` / `resolve_repo_for_admin` |
| `crates/oxidean-db/src/releases.rs` | persistence | ✓ VERIFIED | 325 lines; insert/list/update/delete + assets |
| `crates/oxidean-db/src/redirects.rs` | redirect helpers | ✓ VERIFIED | insert/find/delete/purge |
| `crates/oxidean-api/src/routes/release_assets.rs` | upload/download | ✓ VERIFIED | 481 lines; multipart + id download |
| `crates/oxidean-api/src/repo/rename_transfer.rs` | rename + transfer | ✓ VERIFIED | 387 lines; confirm, dest resolve, disk move |
| `crates/oxidean-db/migrations/*/0014_releases_redirects.sql` | schema | ✓ VERIFIED | releases, release_assets, repository_redirects |
| `packages/api-client/src/index.ts` | generated client | ✓ VERIFIED | `release.*`, `repo.rename`, `repo.transfer` |
| `apps/web/.../releases*.tsrx` | Releases UI | ✓ VERIFIED | list/new/$tag; in `routeTree.gen.ts` |
| `apps/web/.../$owner.$repo.settings.tsrx` | Danger zone | ✓ VERIFIED | rename + transfer type-confirm; `can_admin` |
| `apps/web/.../repo-chrome.tsrx` | Releases tab + can_admin Settings | ✓ VERIFIED | `active === "releases"`; Settings via `repo.can_admin` |
| `docker-compose.yml` | release-assets volume | ✓ VERIFIED | `./var/release-assets:/var/release-assets` |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `release/mod.rs` | ACL + `db.insert_release` / list / update / delete | `meets` / `resolve_repo_*` | ✓ WIRED | Manual (gsd path-literal check false-negative) |
| `$owner.$repo.releases.$tag.tsrx` | `GET /api/releases/assets/{id}` | download `href` | ✓ WIRED | Line ~203 |
| `release_assets.rs` | `release_assets_dir.join(asset_id)` | opaque FS path | ✓ WIRED | D-REL-04 |
| `git_smart_http.rs` / `ssh/pack.rs` | `lookup_repo_row_or_redirect` | old path resolve | ✓ WIRED | D-REL-08 |
| `jobs/reconcile.rs` | `purge_expired_repository_redirects` | expires_at purge | ✓ WIRED | |
| settings Danger zone | `repo.rename` / `repo.transfer` | forms + AlertDialog | ✓ WIRED | |
| `admin.factory_reset` | `release_assets_dir` | wipe on repos scope | ✓ WIRED | |
| releases.new | `release.create` | tag picker + RPC | ✓ WIRED | |
| repo-chrome | `/releases` routes | nav link + routeTree | ✓ WIRED | |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| Releases list | `release.list` → `releases` | DB `list_releases_for_repo` | Yes | ✓ FLOWING |
| Release detail | `release.get` + assets | DB releases + release_assets | Yes | ✓ FLOWING |
| Asset download | file bytes | `OXIDEAN_RELEASE_ASSETS_DIR/{id}` | Yes | ✓ FLOWING |
| Rename/transfer | repo row + bare path | DB + filesystem | Yes | ✓ FLOWING |
| Redirect resolve | `repository_redirects` | DB lookup | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Create release for existing tag | `cargo test -p oxidean-api --test release_rpc release_create_existing_tag_with_notes -- --exact` | ok | ✓ PASS |
| Asset upload/download ACL | `cargo test -p oxidean-api --test release_rpc release_asset_upload_download_acl -- --exact` | ok | ✓ PASS |
| Admin rename + redirect | `cargo test -p oxidean-api --test repo_rename_transfer repo_rename_admin_moves_disk_and_inserts_redirect -- --exact` | ok | ✓ PASS |
| Admin transfer + confirm | `cargo test -p oxidean-api --test repo_rename_transfer repo_transfer_admin_to_user_or_org_with_confirm -- --exact` | ok | ✓ PASS |
| Redirect purge | `… redirect_purge_expired_rows -- --exact` | ok | ✓ PASS |
| Transfer cascade issues/LFS | `… repo_transfer_cascade_issues_lfs_by_repo_id -- --exact` | ok | ✓ PASS |
| Dialect migration 0013 | `cargo test -p oxidean-db --test dialect_releases` | 2 passed | ✓ PASS |
| Web chrome + settings raw tests | vitest rename-transfer + chrome Releases | 3 passed | ✓ PASS |
| Web route ESM import discoverability | vitest `routes under /releases discoverable` | import without `?raw` → null | ⚠️ WARNING |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| — | — | No phase-declared `scripts/*/tests/probe-*.sh` | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| GIT-14 | 15-01/02/05/06 | Create release for tag with notes + assets | ✓ SATISFIED | RPC + UI + asset upload |
| GIT-15 | 15-02/05 | Download release assets from web UI | ✓ SATISFIED | Download links + ACL’d GET handler |
| GIT-16 | 15-03/05 | Rename repository with permission | ✓ SATISFIED | `repo.rename` + Danger zone |
| GIT-17 | 15-04/05 | Transfer to user or org | ✓ SATISFIED | `repo.transfer` + type-confirm |

No orphaned Phase 15 requirements.

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `release_rpc.rs` | GIT-14/15 | 10 | 0 | 0 | Behavioral | PASS |
| `repo_rename_transfer.rs` | GIT-16/17 | 8 | 0 | 0 | Behavioral | PASS |
| `dialect_releases.rs` | schema | 2 | 0 | 0 | Value | PASS |
| `releases.integration.test.ts` | GIT-14/15 UI | 1 pass / 1 fail | 0 | 0 | Existence (broken harness) | WARNING |
| `settings.rename-transfer.integration.test.ts` | GIT-16/17 UI | 2 | 0 | 0 | Existence (`?raw`) | PASS |

**Disabled tests on requirements:** 0

**Circular patterns detected:** 0

**Insufficient assertions:** 1 WARNING — discoverability test imports `.tsrx` without `?raw` (sibling settings tests use `?raw` and pass). Routes exist with `export function RepoRelease*` and are registered in `routeTree.gen.ts`. Not treated as a goal BLOCKER; fix the test import style in a follow-up if desired.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `apps/web/src/routes/$owner.$repo.releases.integration.test.ts` | ~27–33 | Dynamic import without `?raw` fails in Vitest | ⚠️ Warning | Flaky/false-fail discoverability check; implementation present |
| — | — | TBD/FIXME/XXX in phase impl files | — | None found |

### Human Verification Required

### 1. End-to-end Releases + Danger zone (from 15-05-PLAN)

**Test:** Open a repo with Write+: create a release for an existing tag, upload an asset, download it. As Admin: rename and transfer with type-confirm; confirm old URL redirects.

**Expected:** Full browser flow works; assets download; rename/transfer change paths; old URL redirects within retention.

**Why human:** Planner-deferred `<human-check>`; visual/UX and live redirect navigation.

### Gaps Summary

No goal-blocking gaps for the three must-have truths. Roadmap success criteria 1–3 are implemented and covered by named API/integration tests. Migration remains **`0014_releases_redirects`**. End-of-phase compose/browser UAT is closed (see UAT closure below). Non-blocking Vitest import-style WARNING above remains advisory only.

**Residual (not failures of Phase 15 must-haves):** Wave 0 UI tests are mostly export/chrome/`?raw` asserts; there is no stack-browser e2e for releases CRUD or settings Danger zone rename/transfer; soft-delete disk purge remains a documented Phase 07 deferral adjacent to this Danger zone. Details below.

---

## Known stubs / residual gaps

Honesty annotations from [issue #3](https://github.com/oxidean/oxidean/issues/3) quality audit (`tmp/issue-3-quality-audit.md`) and Phase 11.1 `D-QH-05`. These do **not** flip Phase 15 must-haves to failed — API/nextest coverage and compose UAT still support the three truths — but **do** justify **passed with caveats** so planners do not treat green VERIFICATION as full forge browser e2e.

| Stub / gap | What shipped | What is *not* done | Pointers |
| ---------- | ------------ | ------------------ | -------- |
| **Wave 0 UI tests thin (export/chrome)** | Vitest colocated suites assert Releases tab in chrome, route exports / discoverability, and Danger zone rename/transfer source contracts via `?raw` | Full interaction / hydration coverage of create→upload→download or rename/transfer dialogs in happy-dom | `$owner.$repo.releases.integration.test.ts` (chrome + export asserts; one discoverability WARNING without `?raw`); `$owner.$repo.settings.rename-transfer.integration.test.ts` (`?raw` string matches only) |
| **Stack-browser e2e for releases CRUD** | Rust `release_rpc`; Vitest Wave 0; **plus** Phase 11.1-04 Chromium create-from-tag (`forge-issues-releases.stack.browser.test.tsx`) | Asset upload/download + full detail hydration remain thinner than API | `apps/web/e2e/stack-browser/forge-issues-releases.stack.browser.test.tsx` (**closed** for create path; assets later) |
| **Settings Danger zone under-tested in browser** | `repo.rename` / `repo.transfer` nextest; settings source contract Vitest; human-check / UAT for rename RPC | No stack-browser (or equivalent) coverage of type-confirm rename/transfer UX in Chromium | `$owner.$repo.settings.tsrx` Danger zone; `$owner.$repo.settings.rename-transfer.integration.test.ts`; residual after 11.1 forge matrix |
| **Soft-delete disk purge deferred** | Danger zone shares soft-delete IA with rename/transfer; DB soft-delete + redirect retention jobs for rename/transfer paths | Synchronous or Phase-15-owned disk wipe of soft-deleted bare repos — still the Phase 07 **D-35** deferral (orphan reconcile / retention purge elsewhere) | Relevant because Settings Danger zone co-locates Delete with Rename/Transfer (`15-05`); disposition remains documented deferral, not a Phase 15 reopen |

**Footnotes for planners**

1. Truths #1–3 are RPC + wiring + compose UAT — Chromium create path is 11.1-04, not asset/Danger e2e.
2. “Web chrome + settings raw tests” / Wave 0 green ≠ Danger zone browser e2e.
3. Soft-delete purge gaps belong to Phase 07 retention jobs / quality follow-ups — do not invent purge behavior from a Phase 15 VERIFICATION green light.

---

_Verified: 2026-09-14T17:54:05Z_
_Verifier: Claude (gsd-verifier)_
_Honesty fixup: 2026-09-15 (Known stubs / residual gaps; status → passed with caveats)_
_Residual 11.1-05: releases stack-browser create marked closed (keep Danger zone + soft-delete)_


## UAT closure

Compose+browser UAT 2026-09-14: release list+detail SSR, rename RPC; PG draft decode + releases index layout fixes.
Verified: 2026-09-14T19:36:46Z

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 46 files

