---
phase: 11-issues
plan: "10"
subsystem: ui
tags: [remark-github, markdown, autolink, rehype-sanitize, ISS-04, vitest]

requires:
  - phase: 11-issues
    provides: "Wave 0 markdown.issues stubs + Write|Preview (11-05)"
provides:
  - "renderGfm #N / owner/repo#N autolink with sanitize-last"
  - "remark-github@12.0.0 pinned; mentions/commits disabled"
affects: [issues-detail, comments, markdown-preview]

actuals:
  tokens: 3155
  tasks: 2
  commits: 6

plan_head_before: 4527719b54514f5a23dcb1da17b4d8e9f33d742e

tech-stack:
  added: [remark-github@12.0.0]
  patterns:
    - "remarkParse → remarkGfm → remarkGithub(buildUrl) → remarkRehype → rehypeSanitize → rehypeStringify"
    - "buildUrl returns issue paths only; mention/commit/compare → false"

key-files:
  created:
    - .planning/phases/11-issues/.tdd/11-10-red-evidence.json
  modified:
    - apps/web/package.json
    - apps/web/src/lib/markdown.ts
    - apps/web/src/lib/markdown.issues.test.ts
    - apps/web/src/components/repo/markdown-write-preview.tsrx
    - apps/web/src/components/repo/issue-comments.tsrx
    - apps/web/src/routes/$owner.$repo.issues.$n.tsrx
    - apps/web/src/routes/$owner.$repo.issues.new.tsrx

key-decisions:
  - "Pin remark-github@12.0.0; Package Legitimacy OK (RESEARCH) — no human legitimacy checkpoint"
  - "Disable @mention/commit/compare autolinks via buildUrl → false (Q1 / D-ISS-13)"
  - "Confirmed buildUrl fields type/user/project/no against installed 12.x types (A1)"
  - "Thread owner/repo into Preview + comment/issue body render for surface autolink"

patterns-established:
  - "ISS-04 markdown: remark-github before rehype-sanitize; never post-process HTML for #N"

requirements-completed: [ISS-04]

coverage:
  - id: D1
    description: "#N and owner/repo#N autolink to /{owner}/{repo}/issues/{n} with sanitize-last"
    requirement: ISS-04
    verification:
      - kind: unit
        ref: "apps/web/src/lib/markdown.issues.test.ts#turns #N into /{owner}/{repo}/issues/{n} href"
        status: pass
      - kind: unit
        ref: "apps/web/src/lib/markdown.issues.test.ts#turns owner/repo#N into /owner/repo/issues/n href"
        status: pass
      - kind: unit
        ref: "apps/web/src/lib/markdown.test.ts#strips script tags and unsafe HTML"
        status: pass
    human_judgment: false
  - id: D2
    description: "Mentions and commit SHAs do not become links (buildUrl false)"
    requirement: ISS-04
    verification:
      - kind: unit
        ref: "apps/web/src/lib/markdown.issues.test.ts#does not autolink @mentions"
        status: pass
      - kind: unit
        ref: "apps/web/src/lib/markdown.issues.test.ts#does not autolink commit SHAs"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-14
status: complete
---

# Phase 11 Plan 10: remark-github Autolink Summary

**Installed remark-github@12.0.0 and extended `renderGfm` so `#N` / `owner/repo#N` become instance-local issue links with rehype-sanitize last; mentions and commits stay unlinked.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-09-14T15:48:32Z
- **Completed:** 2026-09-14T15:53:00Z
- **Tasks:** 2 completed
- **Files modified:** 8 (+ RED evidence)

## Accomplishments

- Pinned `remark-github@12.0.0` (Package Legitimacy OK per RESEARCH — no postinstall).
- TDD: Wave 0 `it.fails` → intentional RED → GREEN pipeline with sanitize-last.
- Threaded `owner`/`repo` through `MarkdownWritePreview`, comment bodies, issue detail body, and new-issue Preview.

## Task Commits

1. **Task 1: Install remark-github@12.0.0** — `3ca702d` (chore)
2. **Task 2 RED: Failing autolink tests** — `e5a9e63` (test)
3. **Task 2 GREEN: renderGfm + Preview wiring** — `fdfcca9` (feat)

**Plan metadata:** `4c0bf5d` (docs: complete plan)

## TDD Gate Compliance

| Gate | Commit | Evidence |
|------|--------|----------|
| RED | `e5a9e63` | `.tdd/11-10-red-evidence.json` → `RED_EVIDENCE_OK` (`target_test_failed`) |
| GREEN | `fdfcca9` | `bunx vitest run src/lib/markdown.issues.test.ts src/lib/markdown.test.ts` — 7/7 pass |
| REFACTOR | — | Not needed |

## Files Created/Modified

- `apps/web/package.json` / `bun.lock` — remark-github@12.0.0
- `apps/web/src/lib/markdown.ts` — optional owner/repo + remarkGithub buildUrl
- `apps/web/src/lib/markdown.issues.test.ts` — green autolink + sanitize/mention/commit cases
- `apps/web/src/components/repo/markdown-write-preview.tsrx` — owner/repo props → Preview
- `apps/web/src/components/repo/issue-comments.tsrx` — CommentBody + Preview context
- `apps/web/src/routes/$owner.$repo.issues.$n.tsrx` — bodyHtml + edit Preview context
- `apps/web/src/routes/$owner.$repo.issues.new.tsrx` — Preview context
- `.planning/phases/11-issues/.tdd/11-10-red-evidence.json` — RED gate record

## Decisions Made

- Legitimacy checkpoint skipped: RESEARCH verdict OK/Approved for remark-github@12.0.0.
- Q1: `buildUrl` returns `false` for mention/commit/compare; only `type === "issue"` yields `/{user}/{project}/issues/{no}`.
- A1: Confirmed 12.x `BuildUrlIssueValues` fields (`type`/`user`/`project`/`no`) against installed `index.d.ts`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] Wired detail route + comment render paths**
- **Found during:** Task 2 (GREEN)
- **Issue:** Plan said avoid editing detail route files, but success criteria require issue/comment markdown to autolink on surfaces — Preview props alone would leave displayed bodies without `#N` links.
- **Fix:** Passed `owner`/`repo` into detail `renderGfm(body)`, detail/edit Preview, new-issue Preview, `CommentBody`, and comment compose/edit Preview.
- **Files modified:** `apps/web/src/routes/$owner.$repo.issues.$n.tsrx`, `apps/web/src/routes/$owner.$repo.issues.new.tsrx`, `apps/web/src/components/repo/issue-comments.tsrx`
- **Verification:** Unit suite green; sample HTML `See <a href="/ada/hello/issues/42">#42</a>`
- **Committed in:** `fdfcca9`

---

**Total deviations:** 1 auto-fixed (Rule 2)
**Impact on plan:** Necessary for ISS-04 surface correctness; no architectural change.

## Issues Encountered

Vitest nested TAP lacks `# tests` summary — used flat TAP projection for `tdd-red-evidence` (same pattern as plan 07-14).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

ISS-04 markdown autolink complete. Remaining ISS-04 work (if any) is link-table / Linked PRs stubs in later plans.

---
*Phase: 11-issues*
*Completed: 2026-09-14*

## Self-Check: PASSED

- Files present: markdown.ts, markdown.issues.test.ts, markdown-write-preview.tsrx, 11-10-SUMMARY.md
- Commits present: 3ca702d, e5a9e63, fdfcca9
- remark-github@12.0.0 pinned; pipeline sanitize-last verified
