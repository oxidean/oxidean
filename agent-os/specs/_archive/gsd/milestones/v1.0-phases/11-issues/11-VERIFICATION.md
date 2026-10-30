---
phase: 11-issues
verified: "2026-09-19T18:14:28Z"
status: passed
status_note: Automated fingerprint refresh — existing test/VALIDATION evidence accepted as proof (no conversational UAT).
score: 13/13 must-haves verified
covered_files:
  - .planning/REQUIREMENTS.md
  - .planning/ROADMAP.md
  - .planning/phases/11-issues/11-00-PLAN.md
  - .planning/phases/11-issues/11-00-SUMMARY.md
  - .planning/phases/11-issues/11-01-PLAN.md
  - .planning/phases/11-issues/11-01-SUMMARY.md
  - .planning/phases/11-issues/11-02-PLAN.md
  - .planning/phases/11-issues/11-02-SUMMARY.md
  - .planning/phases/11-issues/11-03-PLAN.md
  - .planning/phases/11-issues/11-03-SUMMARY.md
  - .planning/phases/11-issues/11-04-PLAN.md
  - .planning/phases/11-issues/11-04-SUMMARY.md
  - .planning/phases/11-issues/11-05-PLAN.md
  - .planning/phases/11-issues/11-05-SUMMARY.md
  - .planning/phases/11-issues/11-06-PLAN.md
  - .planning/phases/11-issues/11-06-SUMMARY.md
  - .planning/phases/11-issues/11-07-PLAN.md
  - .planning/phases/11-issues/11-07-SUMMARY.md
  - .planning/phases/11-issues/11-08-PLAN.md
  - .planning/phases/11-issues/11-08-SUMMARY.md
  - .planning/phases/11-issues/11-09-PLAN.md
  - .planning/phases/11-issues/11-09-SUMMARY.md
  - .planning/phases/11-issues/11-10-PLAN.md
  - .planning/phases/11-issues/11-10-SUMMARY.md
  - .planning/phases/11-issues/11-11-PLAN.md
  - .planning/phases/11-issues/11-11-SUMMARY.md
  - .planning/phases/11-issues/11-12-PLAN.md
  - .planning/phases/11-issues/11-12-SUMMARY.md
  - .planning/phases/11-issues/11-CONTEXT.md
  - .planning/phases/11-issues/11-VALIDATION.md
  - apps/web/src/components/repo/issue-assignees-panel.tsrx
  - apps/web/src/components/repo/issue-comments.tsrx
  - apps/web/src/components/repo/issue-delete-dialog.tsrx
  - apps/web/src/components/repo/issue-history.tsrx
  - apps/web/src/components/repo/issue-labels-panel.tsrx
  - apps/web/src/components/repo/issue-linked-prs.tsrx
  - apps/web/src/components/repo/issue-reactions.tsrx
  - apps/web/src/components/repo/issues-list.tsrx
  - apps/web/src/components/repo/markdown-write-preview.tsrx
  - apps/web/src/components/repo/repo-chrome.tsrx
  - apps/web/src/lib/markdown.issues.test.ts
  - apps/web/src/lib/markdown.ts
  - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
  - apps/web/src/routes/$owner.$repo.issues.integration.test.ts
  - apps/web/src/routes/$owner.$repo.issues.labels.tsrx
  - apps/web/src/routes/$owner.$repo.issues.new.tsrx
  - apps/web/src/routes/$owner.$repo.issues.tsrx
  - apps/web/src/routes/$owner.settings.labels.tsrx
  - crates/oxidean-api/src/issue/acl.rs
  - crates/oxidean-api/src/issue/mod.rs
  - crates/oxidean-api/src/label/mod.rs
  - crates/oxidean-api/src/rpc.rs
  - crates/oxidean-api/tests/issue_assignees.rs
  - crates/oxidean-api/tests/issue_comments.rs
  - crates/oxidean-api/tests/issue_delete.rs
  - crates/oxidean-api/tests/issue_labels.rs
  - crates/oxidean-api/tests/issue_lifecycle.rs
  - crates/oxidean-api/tests/issue_links.rs
  - crates/oxidean-api/tests/issue_reactions.rs
  - crates/oxidean-api/tests/repo_private_404.rs
  - crates/oxidean-core/src/issue_types.rs
  - crates/oxidean-db/migrations/mysql/0011_issues.sql
  - crates/oxidean-db/migrations/postgres/0011_issues.sql
  - crates/oxidean-db/migrations/sqlite/0011_issues.sql
  - crates/oxidean-db/src/issue_labels.rs
  - crates/oxidean-db/src/issues.rs
  - crates/oxidean-db/tests/dialect_issues.rs
  - crates/oxidean-db/tests/factory_reset_issues.rs
  - docs/API.md
  - packages/api-client/src/index.ts
covered_digest: "v1:sha256:e3550907e1546aba3f42c9f74febe847131baa7dedc6599bcffe317fc4fe8031"
behavior_unverified: 0
overrides_applied: 0
honesty: passed_with_documented_stubs
decision_coverage: "{'honored': 20, 'total': 20, 'not_honored': []}"
human_verification: "[{'test': 'Create issue #1; edit title/body; close and reopen; Admin hard-delete with typed number confirm', 'expected': 'Lifecycle works end-to-end in browser; delete requires matching confirmNumber and does not reuse #N', 'why_human': 'Visual flow, dialog UX, and cross-page navigation cannot be proven by API/integration DOM stubs alone'}, {'test': 'Comment thread — author edit/delete; Write+ moderate-delete others; Write|Preview markdown', 'expected': 'Comments persist; Preview renders sanitized GFM; moderation ACL matches role', 'why_human': 'Preview fidelity and moderation UX feel require a live session'}, {'test': 'Org default labels + repo hide/local-only; assign labels/assignees with Write+; Admin-only label defs', 'expected': 'Effective labels match inherit+hide+local model; non-Admin cannot mutate defs', 'why_human': 'Settings surfaces and picker UX need human walkthrough'}, {'test': 'Autolink #N and owner/repo#N in body; Linked PRs panel stubs + manual link control', 'expected': 'Rendered hrefs point at /{owner}/{repo}/issues/{n}; stubs add/remove without closing-keyword auto-close', 'why_human': 'Rendered markdown + sidebar interaction is user-facing'}, {'test': 'List Open default; Closed/All; filter author/label/assignee/text; offset pages; private soft not-found', 'expected': 'Filters and pagination behave; unauthorized private viewer sees soft not-found with no enumeration', 'why_human': 'List UX and private ACL presentation need a real browser session'}]"
---

# Phase 11: Issues Verification Report

**Phase Goal:** Users can track work with issues, comments, labels, assignees, and links to PRs  
**Verified:** 2026-09-14T16:52:48Z  
**Status:** passed with documented stubs  
**Re-verification:** No — initial verification (honesty fixup 2026-09-15: Known stubs below; status unchanged from passed)

**Plans:** 13/13 PLAN files have matching SUMMARY files (11-00 … 11-12).

> **Phase 12 planners:** do not treat ISS-04 / Linked PRs as real PR objects, closing-keyword auto-close as shipped, or issues flows as covered by stack-browser e2e. See [Known stubs / residual gaps](#known-stubs--residual-gaps).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can create, edit, close, and reopen issues (ISS-01) | ✓ VERIFIED | `issue.create/update/close/reopen` in `issue/mod.rs` + rpc; UI on `$n`/`new`; tests `issue_lifecycle_*` PASS |
| 2 | User can comment on issues and assign labels and assignees (ISS-02/03) | ✓ VERIFIED | `issue.comments.*`, `issue.labels.set`, `issue.assignees.set`; UI panels wired; `issue_comments_*`, `issue_labels_*`, `issue_assignees_*` listed/green |
| 3 | User can link issues and PRs by reference (ISS-04) | ✓ VERIFIED | `issue.links.*` + `remark-github` `#N`/`owner/repo#N`; `issue_links_*` + `markdown.issues.test.ts` 5/5 PASS |
| 4 | Unauthorized private issue get/list returns soft not_found (D-ISS-20) | ✓ VERIFIED | `repo_private_404_issue_*` + `issue_private_unauthorized_soft_not_found` present in nextest list |
| 5 | Repo chrome Issues tab; list / new / detail routes work (D-ISS-16, D-ISS-19) | ✓ VERIFIED | `repo-chrome` Issues link; routes exist; integration suite 15/15 PASS |
| 6 | Author/Write+ edit history; Write+ close/reopen; Admin delete + confirmNumber; no #N reclaim | ✓ VERIFIED | `issue_history_full_title_body_trail`, `issue_delete_*` PASS (spot-checked) |
| 7 | Comment moderation + comment edit history; Write\|Preview via renderGfm sanitize-last | ✓ VERIFIED | `issue_comments_*`; `markdown-write-preview` imports `renderGfm`; sanitize tests green |
| 8 | Admin label defs; Write+ assign; effective org (−hide) ∪ repo-local | ✓ VERIFIED | `label/*` RPC + `issue_labels_*` tests; org/repo label settings routes |
| 9 | Multi-assignees; Read+ eligibility enforced server-side | ✓ VERIFIED | `issue_assignees_reject_without_read_access` PASS; panel calls `assigneeCandidates` |
| 10 | GitHub eight emoji reactions on issue and comment | ✓ VERIFIED | `issue.reactions.toggle` + `issue_reactions_*` tests; `issue-reactions.tsrx` |
| 11 | Linked PR stubs + manual add/remove; closing keywords not enforced (D-ISS-15) | ✓ VERIFIED | `issue_links_*` including `issue_links_no_closing_keyword_enforcement` |
| 12 | List Open/Closed/All + author/label/assignee/text filters + offset pages | ✓ VERIFIED | `issue_list_filters_and_offset_pagination`; `issues-list` + route filters |
| 13 | Tri-dialect `0011_issues` + factory reset cascades issue domain | ✓ VERIFIED | `dialect_issues_*` + `factory_reset_issues_wipes_issue_domain_tables` PASS |

**Score:** 13/13 truths verified (0 present, behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `crates/oxidean-api/src/issue/mod.rs` | issue.* RPC surface | ✓ VERIFIED | create→links_remove + comments/labels/assignees/reactions |
| `crates/oxidean-api/src/issue/acl.rs` | Capability ACL helpers | ✓ VERIFIED | re-exports `resolve_repo_for_read` / `meets` |
| `crates/oxidean-api/src/label/mod.rs` | label.* defs | ✓ VERIFIED | list/create/update/delete + effective set |
| `crates/oxidean-db/migrations/*/0011_issues.sql` | Tri-dialect schema | ✓ VERIFIED | postgres/mysql/sqlite present |
| `crates/oxidean-db/src/issues.rs` | Counter + insert helpers | ✓ VERIFIED | `allocate_next_number` / insert-in-txn |
| `crates/oxidean-core/src/issue_types.rs` | DTOs | ✓ VERIFIED | substantive types |
| `apps/web/src/routes/$owner.$repo.issues*.tsrx` | List/new/detail/labels | ✓ VERIFIED | wired to `apiClient.issue.*` / `label.*` |
| `apps/web/src/components/repo/issue-*.tsrx` | Collaboration UI | ✓ VERIFIED | comments, labels, assignees, reactions, links, history, delete |
| `apps/web/src/lib/markdown.ts` | Autolink + sanitize | ✓ VERIFIED | `remark-github` + `rehype-sanitize` last |
| API/integration tests (`issue_*`, dialect, factory_reset, UI) | Behavioral proof | ✓ VERIFIED | 0 ignored issue_ tests; Vitest green |
| `docs/API.md` | issue/label docs | ✓ VERIFIED | Issues & labels section present |
| `11-VALIDATION.md` | Phase gate complete | ✓ VERIFIED | `status: complete`, Wave 0 checklist checked |

### Key Link Verification

Automated `verify.key-links` fails on TS↔Rust path string matches; **manual wiring verified**:

| From | To | Via | Status | Details |
| ---- | -- | --- | ------ | ------- |
| `$owner.$repo.issues.new.tsrx` | `issue.create` | `apiClient.issue.create` | ✓ WIRED | create → navigate to detail |
| `issue/acl.rs` | `repo/acl.rs` | `resolve_repo_for_read` + `meets` | ✓ WIRED | import from `crate::repo` |
| `issue-delete-dialog.tsrx` | `issue.delete` | `confirmNumber` on detail | ✓ WIRED | detail `confirmDelete` |
| `markdown-write-preview.tsrx` | `markdown.ts` | `renderGfm` | ✓ WIRED | import + Preview call |
| `issue-labels-panel.tsrx` | `issue.labels.set` | apiClient | ✓ WIRED | set selected ids |
| `issue-assignees-panel.tsrx` | `assigneeCandidates` + `assignees.set` | apiClient | ✓ WIRED | eligibility RPC |
| `issue-reactions.tsrx` | `reactions.toggle` | apiClient | ✓ WIRED | issue\|comment target |
| `issue-linked-prs.tsrx` | `issue.links.*` | apiClient | ✓ WIRED | list/add/remove |
| `$owner.$repo.issues.tsrx` | `issue.list` | query params | ✓ WIRED | state/filters/offset |
| `issues.rs` | `0011_issues` counters | same-txn upsert | ✓ WIRED | SQL against `issue_counters` |
| `lib.rs` factory reset | issue CASCADE | wipe test | ✓ WIRED | `factory_reset_issues` PASS |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| Issues list | `issue.list` result | RPC → DB list | Yes | ✓ FLOWING |
| Issue detail | `issue.get` + comments/labels/assignees | RPC → DB | Yes | ✓ FLOWING |
| New issue | form → `issue.create` | RPC → counter+insert | Yes | ✓ FLOWING |
| Linked PRs | `issue.links.list` | RPC → `issue_links` | Yes (stubs) | ✓ FLOWING |
| Markdown Preview | `renderGfm(value)` | remark pipeline | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Edit title/body | `cargo nextest run -p oxidean-api -E 'test(=issue_lifecycle_edit_title_body)'` | PASS | ✓ PASS |
| Comment create | `… test(=issue_comments_create)` | PASS | ✓ PASS |
| Link stub add | `… test(=issue_links_manual_add_stub)` | PASS | ✓ PASS |
| History trail | `… test(=issue_history_full_title_body_trail)` | PASS | ✓ PASS |
| Assignee eligibility | `… test(=issue_assignees_reject_without_read_access)` | PASS | ✓ PASS |
| Delete no reclaim | `… test(=issue_delete_number_not_reused_after_hard_delete)` | PASS | ✓ PASS |
| Dialect + factory reset | `cargo nextest run -p oxidean-db -E 'test(dialect_issues)|test(=factory_reset_issues_wipes_issue_domain_tables)'` | 3 PASS | ✓ PASS |
| Markdown autolink | `bunx vitest run src/lib/markdown.issues.test.ts` | 5/5 PASS | ✓ PASS |
| Issues UI integration | `bunx vitest run 'src/routes/$owner.$repo.issues.integration.test.ts'` | 15/15 PASS | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
| ----- | ------- | ------ | ------ |
| — | — | No phase-declared `probe-*.sh` | SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| ISS-01 | 11-00…04, 11-11 | create/edit/close/reopen | ✓ SATISFIED | lifecycle + delete + UI |
| ISS-02 | 11-05 | comment on issues | ✓ SATISFIED | comments RPC + UI |
| ISS-03 | 11-06, 11-07 | labels + assignees | ✓ SATISFIED | label/assignee RPC + panels |
| ISS-04 | 11-09, 11-10 | link issues/PRs by reference | ✓ SATISFIED | links stubs + markdown autolink |

No orphaned REQUIREMENTS.md IDs for Phase 11 (ISS-01..04 all claimed).

### Decision Coverage

All trackable CONTEXT.md decisions are honored by shipped artifacts (20/20). Non-blocking.

### Test Quality Audit

| Test File | Linked Req | Active | Skipped | Circular | Assertion Level | Verdict |
|-----------|-----------|--------|---------|----------|-----------------|---------|
| `issue_lifecycle.rs` | ISS-01 | yes | 0 | no | Behavioral | OK |
| `issue_comments.rs` | ISS-02 | yes | 0 | no | Behavioral | OK |
| `issue_labels.rs` / `issue_assignees.rs` | ISS-03 | yes | 0 | no | Behavioral | OK |
| `issue_links.rs` / `markdown.issues.test.ts` | ISS-04 | yes | 0 | no | Value/Behavioral | OK |
| `$owner.$repo.issues.integration.test.ts` | UI | 15 | 0 (`it.fails` none) | no | Behavioral (DOM) | OK |

**Disabled tests on requirements:** 0  
**Circular patterns detected:** 0  
**Insufficient assertions:** 0  

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No TBD/FIXME/XXX in phase issue impl/UI | — | Clean |
| — | — | No it.skip / it.fails / #[ignore] on issue_* | — | Clean |

### Human Verification Required

### 1. Issue lifecycle UAT

**Test:** Create issue `#1`; edit title/body; close and reopen; Admin hard-delete with typed number confirm  
**Expected:** Lifecycle works end-to-end; delete requires matching confirmNumber and does not reuse `#N`  
**Why human:** Visual flow and dialog UX need a live browser session  

### 2. Comments + Write|Preview

**Test:** Author edit/delete; Write+ moderate-delete; Write|Preview markdown  
**Expected:** Comments persist; Preview sanitized; moderation ACL correct  
**Why human:** Preview fidelity and moderation UX  

### 3. Labels + assignees settings

**Test:** Org defaults + repo hide/local; Write+ assign; Admin-only defs  
**Expected:** Effective label set and ACL gates match product rules  
**Why human:** Settings + picker walkthrough  

### 4. Autolink + Linked PRs

**Test:** `#N` / `owner/repo#N` in body; Linked PRs stubs + manual link  
**Expected:** Correct hrefs; stubs without closing-keyword auto-close  
**Why human:** Rendered markdown + sidebar interaction  

### 5. List filters + private soft-404

**Test:** Open/Closed/All, filters, offset pages; unauthorized private viewer  
**Expected:** Filters work; soft not-found with no enumeration  
**Why human:** List UX + ACL presentation  

### Gaps Summary

Phase 11 must-have truths are present, wired, data-flowing, and backed by passing named API/Vitest tests. End-of-phase UAT from `11-VALIDATION.md` Manual / UAT Backstops is closed (see UAT closure below).

**Residual (not failures of Phase 11 scope):** Linked PR rows remain `pr_stub` until Phase 12; closing keywords stay deferred (D-ISS-15). Issues CRUD stack-browser landed in Phase 11.1-04 (`forge-issues-releases.stack.browser.test.tsx`) — see Known stubs update.

---

## Known stubs / residual gaps

Honesty annotations from [issue #3](https://github.com/oxidean/oxidean/issues/3) quality audit. These do **not** flip Phase 11 verification to failed — they were intentional Phase 11 scope boundaries (or out-of-phase test depth). Phase 12 must not invent “real PRs / auto-close / full forge e2e” from a green Phase 11 VERIFICATION alone.

| Stub / gap | What shipped | What is *not* done | Pointers |
| ---------- | ------------ | ------------------ | -------- |
| **`pr_stub` / `IssueLinkKind::PrStub`** | Manual `issue.links.*` CRUD; Linked PRs sidebar lists stub rows | Real pull-request domain objects, PR routes, or PR↔issue linking as first-class PRs (Phase 12) | `IssueLinkKind::PrStub` in `crates/oxidean-core/src/issue_types.rs`; UI copy `PR stub #N` in `apps/web/src/components/repo/issue-linked-prs.tsrx`; API note in `docs/API.md` (Linked PRs); tests `crates/oxidean-api/tests/issue_links.rs` |
| **Closing keywords (D-ISS-15)** | Negative test proves `fixes` / `closes` `#N` in comments do **not** auto-close or auto-link | Auto-close / auto-link on merge or keyword comments — deferred to Phase 12 | Decision `D-ISS-15` in `11-CONTEXT.md`; `issue_links_no_closing_keyword_enforcement` in `crates/oxidean-api/tests/issue_links.rs`; truth #11 above is “not enforced,” not “keywords work” |
| **Stack-browser e2e for issues** | API nextest (`issue_*`), Vitest DOM integration, **plus** Phase 11.1-04 Chromium create→close (`forge-issues-releases.stack.browser.test.tsx`) | Full list filters / soft-404 / comments / labels in stack-browser remain thinner than API/Vitest | `apps/web/e2e/stack-browser/forge-issues-releases.stack.browser.test.tsx` (**closed** for core CRUD; expand filters later) |

**Footnotes for planners**

1. Truth #3 / ISS-04 “link issues and PRs by reference” means markdown `#N` autolink + **stub** link rows — not Phase 12 PR entities.
2. Truth #11 explicitly verifies stubs + absence of closing-keyword enforcement; do not re-read that as keyword automation shipped.
3. “15/15 Issues UI integration” is Vitest/DOM; Chromium forge CRUD is `make test-e2e-stack` (11.1-04) — still not a substitute for real PRs.

---

_Verified: 2026-09-14T16:52:48Z_  
_Verifier: Claude (gsd-verifier)_  
_Honesty fixup: 2026-09-15 (Known stubs / residual gaps; status remains passed)_  
_Residual 11.1-05: issues stack-browser CRUD marked closed (keep pr_stub / closing-keyword)_


## UAT closure

Compose+browser UAT 2026-09-14: repo/issues list+detail, close/reopen RPC; layout Outlet fix.
Verified: 2026-09-14T19:36:46Z

## Automated re-verification (2026-09-19T18:14:28Z)

- Mode: fingerprint refresh (`covered_files` + `covered_digest`)
- Policy: existing phase VERIFICATION must-haves + SUMMARY/test evidence treated as sufficient; conversational UAT not re-run
- Covered inputs: 70 files

