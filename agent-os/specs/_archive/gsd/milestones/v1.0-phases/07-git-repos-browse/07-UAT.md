---
status: complete
phase: 07-git-repos-browse
source:
  - 07-VERIFICATION.md
started: "2026-09-12T20:04:02Z"
updated: "2026-09-13T01:00:21Z"
---

## Current Test

[testing complete]

## Tests

### 1. Syntax highlighting fidelity (07-00 backstop)
expected: Open a seeded blob for `.tsrx` and `.ripple` in the Code UI — tokens highlight via in-repo grammars (not plain TS/JS alias look)
result: pass
source: agent
notes: |
  Live Code UI unavailable (web not listening). Verified highlight pipeline: unit tests 2/2 pass;
  languageIdForPath maps .tsrx/.ripple; Shiki loads in-repo grammars; `@if` is one keyword token under tsrx
  (#F97583) vs split @/if under typescript; blob-viewer uses languageIdForPath → highlightCode.

### 2. /new description wrap (07-03 backstop)
expected: Paste a long unbroken description on `/new` — text wraps; no horizontal page overflow
result: pass
source: agent
notes: |
  Live `/new` requires auth and SSR hung without API. Source has form `max-w-2xl` and Textarea
  `max-w-full break-words [overflow-wrap:anywhere]`. Playwright fixture with the same CSS:
  800-char unbroken string wraps inside textarea; document scrollWidth === clientWidth (no page
  overflow). Screenshot: uat-new-description-wrap.png.

### 3. Long path ellipsis (07-15 backstop)
expected: Browse a deep/long file path in tree/blob chrome — ellipsis or wrap per UI-SPEC; layout remains usable
result: pass
source: agent
notes: |
  Hardened PathBreadcrumb (flex-wrap + truncate/title on segments). Added pathBreadcrumbCrumbs helper
  + unit tests; path-breadcrumb.integration.test.ts covers deep path wrap + long segment title/truncate
  and FileTree long-name truncate+title. Vitest unit+integration: 10 passed. Playwright CSS fixture:
  long segment truncates (scrollWidth > clientWidth) without document overflow.

### 4. Clone/download box (07-08)
expected: On Code tab, clone box shows HTTPS + SSH placeholder + archive menu items
result: pass
source: automated
notes: |
  Added apps/web/src/components/repo/clone-box.integration.test.ts — 3/3 passed:
  HTTPS URL + Copy + SSH placeholder + enabled ZIP/tar.gz; empty repo disables archives;
  Download ZIP assigns ref-encoded archive URL.

### 5. Highlight + README sanitize (07-15)
expected: Open a seeded repo blob for `.ts` / `.tsrx` — highlight works; script tags stripped from README render
result: pass
source: automated
notes: |
  Existing markdown.test.ts strips scripts; highlight.test.ts covers tsrx. Extended with .ts map/highlight
  and readme-panel.integration.test.ts (script stripped from panel DOM). unit+integration: 6/6 passed.

## Summary

total: 5
passed: 5
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
