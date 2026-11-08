---
phase: 07-git-repos-browse
plan: "14"
subsystem: ui
tags: [shiki, remark-gfm, rehype-sanitize, markdown, syntax-highlighting, tsrx, ripple, TextMate]
requires:
  - phase: 07-git-repos-browse
    provides: "Web app package + vitest unit project (plan 03+)"
provides:
  - "renderGfm sanitized GFM HTML pipeline (D-18)"
  - "Shiki singleton with github-light/dark + in-repo tsrx/ripple grammars (D-19)"
affects:
  - 07-15
  - code-browse-ui
  - readme-rendering
actuals:
  tokens: 3637
  tasks: 1
  commits: 2
plan_head_before: ad16f7ce6493459bf5db9e1c51e139a30bcf2cdc
tech-stack:
  added:
    - shiki@4.4.3
    - unified@11.0.5
    - remark-parse@11.0.0
    - remark-gfm@4.0.1
    - remark-rehype@11.1.2
    - rehype-sanitize@6.0.0
    - rehype-stringify@10.0.1
  patterns:
    - "rehype-sanitize last in unified pipeline"
    - "Shiki createHighlighter singleton with custom TextMate langs (not TS/JS alias)"
key-files:
  created:
    - apps/web/src/lib/markdown.ts
    - apps/web/src/lib/markdown.test.ts
    - apps/web/src/lib/highlight.ts
    - apps/web/src/lib/highlight.test.ts
    - apps/web/src/lib/grammars/tsrx.tmLanguage.json
    - apps/web/src/lib/grammars/ripple.tmLanguage.json
  modified:
    - apps/web/package.json
    - apps/web/vitest.config.ts
    - bun.lock
key-decisions:
  - "D-18: remark-gfm → remark-rehype → rehype-sanitize → rehype-stringify (sanitize last)"
  - "D-19: in-repo TextMate grammars for tsrx/ripple registered by name — not TypeScript/JavaScript aliases"
  - "Cool-biased themes = github-light + github-dark via Shiki singleton"
patterns-established:
  - "Markdown/highlight libs live under apps/web/src/lib for Code UI consumption"
  - "Custom langs: JSON TextMate under src/lib/grammars/ loaded into createHighlighter"
requirements-completed: [GIT-05]
coverage:
  - id: D1
    description: "renderGfm sanitizes GFM to HTML with rehype-sanitize last (no script/XSS handlers)"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "apps/web/src/lib/markdown.test.ts#strips script tags and unsafe HTML"
        status: pass
      - kind: unit
        ref: "apps/web/src/lib/markdown.test.ts#renders GFM tables without raw HTML passthrough"
        status: pass
    human_judgment: false
  - id: D2
    description: "Shiki loads in-repo tsrx/ripple grammars and maps .tsrx/.ripple paths (not TS/JS alias)"
    requirement: GIT-05
    verification:
      - kind: unit
        ref: "apps/web/src/lib/highlight.test.ts#maps .tsrx and .ripple via in-repo grammars not TS/JS alias alone"
        status: pass
      - kind: unit
        ref: "apps/web/src/lib/highlight.test.ts#highlights tsrx source with registered language id"
        status: pass
    human_judgment: false
duration: 4min
completed: 2026-09-12
status: complete
---

# Phase 07 Plan 14: Markdown Sanitize + Shiki Grammars Summary

**Sanitized GFM (`renderGfm`) and Shiki highlighting with in-repo `.tsrx`/`.ripple` TextMate grammars ready for Code UI (D-18/D-19).**

## Performance

- **Duration:** 4 min
- **Started:** 2026-09-12T17:46:15Z
- **Completed:** 2026-09-12T17:50:15Z
- **Tasks:** 1
- **Files modified:** 8

## Accomplishments

- Installed RESEARCH-pinned `shiki` + unified/remark/rehype stack (all legitimacy OK)
- `renderGfm` strips script tags/event handlers with sanitize-last pipeline
- Shiki singleton registers GitHub-class langs plus `source.tsrx` / `source.ripple` grammars

## TDD Gate Compliance

| Gate | SHA | Result |
|------|-----|--------|
| RED `test(07-14)` | `114bc5b` | `RED_EVIDENCE_OK` — target `strips script tags and unsafe HTML` assertion failure on stub passthrough |
| GREEN `feat(07-14)` | `8355524` | vitest markdown/highlight + `bun run build` green |

## Task Commits

Each task was committed atomically (TDD = test → feat):

1. **Task 1 RED: failing sanitize + grammar tests** - `114bc5b` (test)
2. **Task 1 GREEN: markdown/highlight libs + grammars** - `8355524` (feat)

**Plan metadata:** `d7c1875` (docs: complete plan)

## Files Created/Modified

- `apps/web/src/lib/markdown.ts` — GFM → sanitized HTML
- `apps/web/src/lib/markdown.test.ts` — XSS/table sanitize tests
- `apps/web/src/lib/highlight.ts` — Shiki singleton + path→lang map
- `apps/web/src/lib/highlight.test.ts` — tsrx/ripple registration tests
- `apps/web/src/lib/grammars/tsrx.tmLanguage.json` — Octane/Rivet-oriented TextMate grammar
- `apps/web/src/lib/grammars/ripple.tmLanguage.json` — Ripple TextMate grammar
- `apps/web/package.json` / `bun.lock` — package pins
- `apps/web/vitest.config.ts` — unit include for plan `*.test.ts` paths

## Decisions Made

- Sanitize last after remark-rehype (T-07-14 / D-18)
- Full TextMate grammars for tsrx/ripple — prohibition on alias-only gap escape (D-19)
- Themes: `github-light` / `github-dark` (cool-biased GitHub pair from plan/RESEARCH)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Vitest projects skipped `*.test.ts`**
- **Found during:** Task 1 RED
- **Issue:** Unit project only included `*.unit.test.ts` / `*.gate.test.ts`, so plan-named `markdown.test.ts` / `highlight.test.ts` yielded zero tests (`1..0` TAP)
- **Fix:** Added those two paths to the unit project `include` list
- **Files modified:** `apps/web/vitest.config.ts`
- **Verification:** vitest discovered 4 tests
- **Committed in:** `114bc5b`

**2. [Rule 1 - Bug] Sanitize leaves inert text after stripping `<script>`**
- **Found during:** Task 1 GREEN
- **Issue:** Test asserted `/alert\(/` absence; rehype-sanitize correctly removes tags but keeps text content
- **Fix:** Assert no `<script>` / `on*=` handlers; allow inert text
- **Files modified:** `apps/web/src/lib/markdown.test.ts`
- **Verification:** both markdown tests pass
- **Committed in:** `8355524`

**Total deviations:** 2 auto-fixed (1× Rule 3, 1× Rule 1)
**Impact on plan:** Necessary for discoverable RED and correct XSS assertions; no scope creep (no Code routes / tree APIs)

## Issues Encountered

- Plan verify used `bun --cwd apps/web exec vitest` — Bun has no `exec` script; used `bunx vitest run` from `apps/web` (equivalent)
- Vitest TAP is nested (no `# tests` summary); RED evidence used a flat TAP projection of the same assertion failures for `gsd_run check tdd-red-evidence` → `RED_EVIDENCE_OK`

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Markdown/highlight libs ready for 07-15 Code UI to render README/blobs
- Does not implement Code routes or tree/blob APIs (deferred as planned)

## Self-Check: PASSED

- FOUND: `apps/web/src/lib/markdown.ts`, `highlight.ts`, grammars, tests, package pins
- FOUND: commits `114bc5b`, `8355524`

---
*Phase: 07-git-repos-browse*
*Completed: 2026-09-12*
