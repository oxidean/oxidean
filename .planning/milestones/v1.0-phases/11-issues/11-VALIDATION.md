---
phase: "11"
slug: "issues"
status: complete
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-14"
updated: "2026-09-15"
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Seeded from `11-RESEARCH.md` Validation Architecture. Wave 0 gaps closed by plans 00–12.
>
> **Honesty (2026-09-15):** Phase gate remains complete, but residual stubs/gaps are documented in
> [`11-VERIFICATION.md` → Known stubs / residual gaps](./11-VERIFICATION.md#known-stubs--residual-gaps)
> (`pr_stub`, closing keywords → Phase 12, thin/no stack-browser issues e2e). Do not treat Wave 0 ✅ as real PR linking or forge Chromium e2e.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo nextest (Rust) + Vitest (web) |
| **Config file** | workspace Cargo / `apps/web/vitest.config.ts` |
| **Quick run command** | `cargo nextest run -p oxidean-api -E 'test(issue_)' ; cargo nextest run -p oxidean-db -E 'test(dialect_issues) | test(factory_reset_issues)' ; bun --cwd apps/web exec vitest run src/lib/markdown.test.ts src/lib/markdown.issues.test.ts` |
| **Full suite command** | `make test` |
| **Estimated runtime** | ~90–240 seconds targeted |
| **Phase gate (11-12)** | Quick run + `cargo test -p oxidean-db --lib migration_parity` + `make rpc-sync-check` + `bun --cwd apps/web run build` |

---

## Sampling Rate

- **Per task commit:** focused nextest filter + relevant Vitest file
- **Per wave merge:** `make test` (or nextest workspace + `bun run test` in apps/web)
- **Phase gate:** Full automated gate green + `make rpc-sync-check` before `/gsd-verify-work`

---

## Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ISS-01 | create/edit/close/reopen + ACL | API integration | `cargo nextest run -p oxidean-api -E 'test(issue_lifecycle)'` | ✅ |
| ISS-01 | per-repo `#N` monotonic / no reuse | dialect + API | `cargo nextest run -p oxidean-db -E 'test(dialect_issues)'` | ✅ |
| ISS-01 | Admin hard-delete + confirmNumber | API | `cargo nextest run -p oxidean-api -E 'test(issue_delete)'` | ✅ |
| ISS-01 | issue edit history trail | API | `cargo nextest run -p oxidean-api -E 'test(issue_lifecycle)'` | ✅ |
| ISS-02 | comment CRUD + moderation delete | API | `cargo nextest run -p oxidean-api -E 'test(issue_comments)'` | ✅ |
| ISS-02 | comment edit history | API | `cargo nextest run -p oxidean-api -E 'test(issue_comments)'` | ✅ |
| ISS-02 | Write\|Preview uses renderGfm sanitize | unit | `bun --cwd apps/web exec vitest run src/lib/markdown.test.ts` | ✅ |
| ISS-03 | labels assign Write+ / defs Admin | API | `cargo nextest run -p oxidean-api -E 'test(issue_labels)'` | ✅ |
| ISS-03 | assignees Read+ eligibility | API | `cargo nextest run -p oxidean-api -E 'test(issue_assignees)'` | ✅ |
| ISS-04 | markdown `#N` / `owner/repo#N` autolink | unit | `bun --cwd apps/web exec vitest run src/lib/markdown.issues.test.ts` | ✅ |
| ISS-04 | link stubs CRUD | API | `cargo nextest run -p oxidean-api -E 'test(issue_links)'` | ✅ |
| ISS-* | reactions toggle (issue + comment) | API | `cargo nextest run -p oxidean-api -E 'test(issue_reactions)'` | ✅ |
| ISS-01..04 | private ACL soft not-found | API | `cargo nextest run -p oxidean-api -E 'test(issue_private) | test(repo_private_404_issue)'` | ✅ |
| UI | Issues tab + list/detail/new routes | web integration | `bun --cwd apps/web exec vitest run src/routes/\$owner.\$repo.issues.integration.test.ts` | ✅ |
| OPS | factory reset cascades issue tables | DB | `cargo nextest run -p oxidean-db -E 'test(factory_reset_issues)'` | ✅ |

---

## Wave 0 Gaps

- [x] `crates/oxidean-api/tests/issue_lifecycle.rs` — ISS-01 create/edit/close/reopen + history
- [x] `crates/oxidean-api/tests/issue_delete.rs` — Admin hard-delete + confirmNumber
- [x] `crates/oxidean-api/tests/issue_comments.rs` — ISS-02 comments + moderation + history
- [x] `crates/oxidean-api/tests/issue_labels.rs` — ISS-03 label defs + assign
- [x] `crates/oxidean-api/tests/issue_assignees.rs` — ISS-03 multi-assignee + Read+ eligibility
- [x] `crates/oxidean-api/tests/issue_links.rs` — ISS-04 stubs + manual link
- [x] `crates/oxidean-api/tests/issue_reactions.rs` — D-ISS-11 eight reactions
- [x] Extend private soft not-found for unauthorized issue access (D-ISS-20) — `repo_private_404.rs` issue_* cases
- [x] `crates/oxidean-db/tests/dialect_issues.rs` — `0011_issues` tri-dialect
- [x] `crates/oxidean-db/tests/factory_reset_issues.rs` — cascade wipe
- [x] `apps/web/src/routes/$owner.$repo.issues.integration.test.ts` — Issues tab + list/detail/new
- [x] `apps/web/src/lib/markdown.issues.test.ts` — `#N` / `owner/repo#N` + sanitize regression

---

## Manual / UAT Backstops

- Create issue `#1`; edit title/body; close and reopen; Admin hard-delete with typed number confirm
- Comment thread: author edit/delete; Write+ moderate-delete others; Write\|Preview markdown
- Org default labels + repo hide/local-only; assign labels/assignees with Write+; Admin-only label defs
- Autolink `#N` and `owner/repo#N` in body; Linked PRs panel shows stubs; manual link control
- List: Open default; Closed/All; filter author/label/assignee/text; offset pages
- Private repo: unauthorized viewer gets soft not-found (no issue enumeration)

## Known stubs / residual gaps (honesty)

Cross-ref full table in [`11-VERIFICATION.md`](./11-VERIFICATION.md#known-stubs--residual-gaps):

| Item | Note for Phase 12 / quality |
|------|-----------------------------|
| `pr_stub` / `IssueLinkKind::PrStub` | UI “PR stub #N”; real PRs are Phase 12 |
| Closing keywords (D-ISS-15) | Not enforced; deferred to Phase 12 |
| Stack-browser issues e2e | Wave 0 UI map uses Vitest integration only; no issues flow under `apps/web/e2e/stack-browser/` yet |
