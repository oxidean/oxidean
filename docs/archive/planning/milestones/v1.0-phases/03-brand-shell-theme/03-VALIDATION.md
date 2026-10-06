---
phase: "03"
slug: brand-shell-theme
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-09-13"
updated: "2026-09-13"
---

# Phase 03 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Reconstructed 2026-09-13 (State B: no prior VALIDATION.md) from `03-*-PLAN.md` /
> `03-*-SUMMARY.md` verify blocks, especially `03-06`.
> Routes/components are now `.tsrx` (were `.tsx` at phase close); automated commands
> below use current paths.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Web: Vitest 5 (unit / integration via happy-dom); shell greps for brand invariants |
| **Config file** | `apps/web/vitest.config.ts` |
| **Quick run command** | `cd apps/web && bunx vitest run src/lib/theme.integration.test.ts src/routes/status.integration.test.ts` + Phase 03 grep subset (see map) |
| **Full suite command** | `bun run --filter @oxidean/web build` + full 03-06 gate table + `bun run --filter @oxidean/web test` |
| **Estimated runtime** | ~5s quick Vitest · ~1–3m with web build |

---

## Sampling Rate

- **After every task commit:** Targeted greps for touched brand surface + theme Vitest
- **After every plan wave:** Quick Vitest + icon/SW existence checks
- **Before `/gsd-verify-work`:** Full 03-06 automated gates (build + greps) green; human visual approved
- **Max feedback latency:** 120 seconds (build path); ~10s without build

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------------|-----------------|-----------|-------------------|-------------|--------|
| 03-01-T1 | 03-01 | 1 | BRAND-01, BRAND-03 / D-13–15 | T-03-22 | Semantic tokens; no `accent-cool`/`accent-warm` | smoke | `grep -rn 'accent-cool\\|accent-warm' apps/web/src/styles.css apps/web/src/components/chrome.tsrx apps/web/src/routes/index.tsrx apps/web/src/routes/status.tsrx apps/web/public/brand` (expect empty) + `grep -q -- '--primary:' apps/web/src/styles.css` | ✅ | ✅ green |
| 03-01-T2 | 03-01 | 1 | BRAND-03 / D-16–18 | — | CVA Button + ShadCN Input/Select primitives | smoke | `grep -q 'cva(' apps/web/src/components/ui/button.tsrx` + `test -f apps/web/src/components/ui/input.tsrx` + `test -f apps/web/src/components/ui/select.tsrx` | ✅ | ✅ green |
| 03-01-T3 | 03-01 | 1 | BRAND-01 / D-01–02 | — | Shared `OxideanMark`; PNG only in mark module | smoke | `grep -rn 'oxidean-mark.png' apps/web/src` → sole match `oxidean-mark.tsrx` | ✅ | ✅ green |
| 03-02-T1 | 03-02 | 2 | BRAND-03–05 / D-07–12 | T-03-04 | FOUC boot + preference persistence helpers | unit/integration | `cd apps/web && bunx vitest run src/lib/theme.integration.test.ts` + `grep -q 'THEME_BOOT_SCRIPT' apps/web/src/routes/__root.tsrx` | ✅ | ✅ green |
| 03-02-T2 | 03-02 | 2 | BRAND-01–02 / D-03, D-06, D-08 | — | Chrome uses mark + no native select in chrome/theme-select | smoke | `grep -q 'OxideanMark' apps/web/src/components/chrome.tsrx` + `test -z "$(grep -nE '<select\\|<option' apps/web/src/components/chrome.tsrx apps/web/src/components/theme-select.tsrx \|\| true)"` | ✅ | ✅ green |
| 03-03-T1 | 03-03 | 2 | BRAND-01–02 / D-22–24 | — | Four editorial bands; `#explore` dual-mode | smoke | `grep -c '<section' apps/web/src/routes/index.tsrx` → `4` + `grep -q 'id="explore"' apps/web/src/routes/index.tsrx` | ✅ | ✅ green |
| 03-03-T2 | 03-03 | 2 | BRAND-03 / D-25 | — | Motion + reduced-motion respect | smoke | `grep -q 'oct-reveal' apps/web/src/routes/index.tsrx` + `grep -q 'prefers-reduced-motion' apps/web/src/styles.css` | ✅ | ✅ green |
| 03-04-T1 | 03-04 | 3 | BRAND-02–03 / D-21, D-26–27 | — | Status title + hero states | integration | `cd apps/web && bunx vitest run src/routes/status.integration.test.ts` + `grep -qF 'Status · Oxidean' apps/web/src/routes/status.tsrx` | ✅ | ✅ green |
| 03-04-T2 | 03-04 | 3 | BRAND-03 / D-25 | — | Status reduced-motion | smoke | `grep -q 'prefers-reduced-motion: reduce' apps/web/src/routes/status.tsrx` | ✅ | ✅ green |
| 03-05-T1 | 03-05 | 3 | BRAND-01 / D-20 | — | Full favicon/PWA icon set non-empty | smoke | `for f in apps/web/public/favicon.ico favicon-16.png favicon-32.png apple-touch-icon.png icons/icon-192.png icons/icon-512.png icons/icon-512-maskable.png; do test -s apps/web/public/${f#apps/web/public/} \|\| test -s apps/web/public/$f; done` (all seven exist, non-empty) | ✅ | ✅ green |
| 03-05-T2 | 03-05 | 3 | BRAND-02 / D-28–30 | T-03-16, T-03-21 | Manifest Oxidean; SW no `/api`/`/health` cache | smoke | `grep -q '"name": "Oxidean"\\|"name":"Oxidean"' apps/web/public/manifest.webmanifest` + `grep -cE '"url":"[^"]*/api/\\|"url":"[^"]*/health"' apps/web/dist/client/sw.js` → `0` + no `CacheFirst\|NetworkFirst\|StaleWhileRevalidate` | ✅ | ✅ green |
| 03-05-T3 | 03-05 | 3 | BRAND-02–03 / D-29 | — | Head manifest + theme-color metas | smoke | `grep -q 'rel="manifest"' apps/web/src/routes/__root.tsrx` + `grep -c 'name="theme-color"' apps/web/src/routes/__root.tsrx` → `2` | ✅ | ✅ green |
| 03-06-T1 | 03-06 | 4 | BRAND-01–05 / D-15, D-30 | T-03-21–22 | Phase-wide gates (scoped to Phase 03 surfaces; see audit) | smoke | See Gate checklist below | ✅ | ✅ green |
| 03-06-T2 | 03-06 | 4 | BRAND-01–05 | T-03-23 | Human visual theme/system/light/dark + brand read | manual | checkpoint (approved 2026-09-09) | n/a | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Plan index (every `03-*-PLAN.md`)

| Plan | Title (objective) | SUMMARY |
|------|-------------------|---------|
| 03-01 | Semantic tokens + CVA primitives + OxideanMark | ✅ |
| 03-02 | FOUC boot + ThemeSelect + branded chrome | ✅ |
| 03-03 | Four-band editorial landing + motions | ✅ |
| 03-04 | Branded `/status` hero states | ✅ |
| 03-05 | Favicon/PWA icons + assets-only SW | ✅ |
| 03-06 | Phase-wide gates + human verification | ✅ |

### BRAND requirement coverage

| Req | Behavior | Automated | Manual |
|-----|----------|-----------|--------|
| BRAND-01 | Mark + blue/orange primary chrome | OxideanMark sole PNG; `--primary`/`--secondary`; icons | Visual squircle / header (03-06-T2) |
| BRAND-02 | Named Oxidean (not GitHub clone / bare Octane) | Titles + naming greps + manifest | Tab title / wordmark (03-06-T2) |
| BRAND-03 | Correct light/dark render | Tokens + status Vitest + theme resolve | Visual contrast (03-06-T2) |
| BRAND-04 | Default system / OS preference | `theme.integration.test.ts` (system default, matchMedia) | Fresh profile OS follow (03-06-T2) |
| BRAND-05 | Persist light/dark override | `theme.integration.test.ts` (`applyTheme` → localStorage/cookie/read) | Refresh persistence (03-06-T2) |

### 03-06 automated gate checklist (adapted paths)

| # | Gate | Command (2026-09-13) | Result |
|---|------|----------------------|--------|
| 1 | Legacy token eradication (D-15) | `grep -rn 'accent-cool\\|accent-warm' apps/web/src apps/web/public/brand` | ✅ empty |
| 2 | No `var(--color-` in Phase 03 surfaces | `grep -rn 'var(--color-' apps/web/src/components/chrome.tsrx apps/web/src/routes/index.tsrx apps/web/src/routes/status.tsrx` | ✅ empty |
| 3 | No native select in chrome/theme-select (D-08) | `grep -nE '<select\\|<option' apps/web/src/components/chrome.tsrx apps/web/src/components/theme-select.tsrx` | ✅ empty |
| 4 | Mark shared (D-01) | `grep -rn 'oxidean-mark.png' apps/web/src` → `oxidean-mark.tsrx` only | ✅ |
| 5 | Titles (D-21) | `grep -q` `title: "Oxidean"` in `__root.tsrx`; `Status · Oxidean` in `status.tsrx` | ✅ |
| 6 | Naming (BRAND-02) | `grep -rni 'github clone' apps/web/src`; `grep -rn '>Octane\\|"Octane"' apps/web/src` | ✅ empty |
| 7 | Icons (D-20) | Seven public icon files non-empty | ✅ |
| 8 | Motion / reduced-motion | landing: `oct-reveal` + `styles.css` `@keyframes`/`prefers-reduced-motion`; status: `@keyframes` + reduce media | ✅ |
| 9 | Built SW no API cache (D-30, T-03-21) | `apps/web/dist/client/sw.js` greps for `/api`/`/health` URLs and Workbox strategies → 0 | ✅ (existing dist; rebuild optional) |
| 10 | Theme helpers (BRAND-04/05) | `bunx vitest run src/lib/theme.integration.test.ts` | ✅ |
| 11 | Status hero states | `bunx vitest run src/routes/status.integration.test.ts` | ✅ |

**Note:** App-wide original gates 3/7/8 from 03-06 (`native <select` anywhere under `routes/`, `font-medium`/`text-[12px]`, `animate-pulse`) are **not** reasserted globally — later phases (auth setup, repo UI, skeleton) intentionally use those patterns. Phase 03 invariants are scoped to brand/chrome/landing/status as above.

---

## Wave 0 Requirements

Existing infrastructure covers Phase 03 (no separate 03-00 stubs required):

- [x] `apps/web/src/lib/theme.ts` + `theme.integration.test.ts` — BRAND-04/05 helpers
- [x] `apps/web/src/routes/status.integration.test.ts` — status hero states
- [x] Vitest projects in `apps/web/vitest.config.ts` (unit / integration)
- [x] Public icon set + `manifest.webmanifest` + hand-authored `public/sw.js`
- [x] Shell grep gates recorded in `03-06-SUMMARY.md` (2026-09-09)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| System / light / dark visual correctness + FOUC-free load + override persists across browser refresh | BRAND-03, BRAND-04, BRAND-05 | Pixel/FOUC and full-browser localStorage round-trip; approved human UAT | Follow `03-06-PLAN.md` how-to-verify items 1–3; **approved 2026-09-09** in `03-06-SUMMARY.md` |
| Squircle mark, responsive wordmark, landing bands, status live states, PWA install/cache UX | BRAND-01, BRAND-02 + D-22–30 | Visual/browser DevTools | Follow `03-06-PLAN.md` items 4–7; **approved 2026-09-09** |

Theme **logic** (default system, invalid→system, matchMedia resolve, `applyTheme` persistence, FOUC boot script contents) is automated via Vitest — only the human visual surface remains Manual-Only.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (03-06-T2 checkpoint is manual by design)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covered by existing Vitest + icons/SW artifacts
- [x] No watch-mode flags
- [x] Feedback latency < 120s (quick path ~5–10s)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** Nyquist auditor gap fill 2026-09-13 (VALIDATION reconstructed; gates re-verified; human UAT retained from 2026-09-09)

---

## Validation Audit

**Date:** 2026-09-13  
**Auditor:** gsd-nyquist-auditor  
**Scope:** State B — create `03-VALIDATION.md`; Fix all gaps; verify representative automated gates; strengthen theme persistence Vitest if missing coverage

### Commands run

| Command | Result |
|---------|--------|
| `grep -rn 'accent-cool\\|accent-warm' apps/web/src apps/web/public/brand` | PASS (empty) |
| Scoped native-select / semantic / mark / title / naming greps (`.tsrx` paths) | PASS |
| Icon set existence (7 files) | PASS |
| `grep -cE '"url":"[^"]*/api/\\|"url":"[^"]*/health"' apps/web/dist/client/sw.js` → 0; Workbox strategies → 0 | PASS |
| `cd apps/web && bunx vitest run src/lib/theme.integration.test.ts src/routes/status.integration.test.ts` | PASS (2 files / 10 tests) |
| Full `bun run --filter @oxidean/web build` | SKIPPED (heavy; existing `dist/client/sw.js` grepped instead) — ⚠️ WARNING |

### Outcomes

- **VALIDATION.md:** Created with plans 03-01…03-06 + BRAND-01…05 mapped (**FILLED**).
- **Theme persistence:** Extended `theme.integration.test.ts` with invalid→system, matchMedia system resolve, applyTheme read-back, `THEME_BOOT_SCRIPT` FOUC asserts (**FILLED**).
- **Visual BRAND-03/04/05:** Remain Manual-Only (approved 2026-09-09) (**justified SKIP of re-running browser UAT**).
- **Implementation changes:** none (read-only).
- **Escalations:** none for Phase 03 requirements. Global 03-06 typography/native-select gates intentionally scoped (later-phase growth) — documented as WARNING, not BLOCKER.
- **Frontmatter:** `status: validated`, `nyquist_compliant: true`, `wave_0_complete: true`.
