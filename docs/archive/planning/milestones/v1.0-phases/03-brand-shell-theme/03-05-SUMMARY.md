---
phase: 03-brand-shell-theme
plan: "05"
subsystem: infra
tags: [pwa, service-worker, favicon, manifest, imagemagick, vite]

requires:
  - phase: 03-brand-shell-theme (03-01)
    provides: "apps/web/public/brand/squircle.svg mask + brand/oxidean-mark.png source"
  - phase: 03-brand-shell-theme (03-02)
    provides: "THEME_BOOT_SCRIPT and __root.tsx <Head> shell with charSet/viewport"
provides:
  - "scripts/gen-icons.sh reproducing the full D-20 favicon/app-icon set from brand/oxidean-mark.png"
  - "committed favicon.ico, favicon-16/32, apple-touch-icon, icon-192/512, icon-512-maskable"
  - "installable Oxidean web manifest at apps/web/public/manifest.webmanifest"
  - "assets-only service worker at apps/web/public/sw.js that never caches /api/* or /health"
  - "__root.tsx head icon/manifest links, light+dark theme-color metas, SW registration"
affects: ["04-*", "any future PWA/offline work", "any future service-worker changes"]

tech-stack:
  added: []
  patterns:
    - "Hand-authored static PWA assets (public/manifest.webmanifest, public/sw.js) instead of a build-plugin-generated service worker, documented because vite-plugin-pwa is incompatible with this repo's Vite 8 multi-environment build"
    - "Service worker fetch handler explicitly bypasses (never caches, in either direction) any request whose pathname starts with /api/ or equals /health"

key-files:
  created:
    - scripts/gen-icons.sh
    - apps/web/public/favicon.ico
    - apps/web/public/favicon-16.png
    - apps/web/public/favicon-32.png
    - apps/web/public/apple-touch-icon.png
    - apps/web/public/icons/icon-192.png
    - apps/web/public/icons/icon-512.png
    - apps/web/public/icons/icon-512-maskable.png
    - apps/web/public/manifest.webmanifest
    - apps/web/public/sw.js
  modified:
    - apps/web/vite.config.ts
    - apps/web/src/routes/__root.tsx
    - .gitignore

key-decisions:
  - "vite-plugin-pwa 1.3.0 was added, wired, and build-tested but never emits sw.js under this Vite 8 / @octanejs/tanstack-start multi-environment build (manifest virtual module resolves; workbox-build's closeBundle hook never fires, no error, no output). Per the plan's documented fallback, the plugin was removed and the manifest + service worker are hand-authored static files in apps/web/public/ instead — D-28/D-29/D-30 guarantees preserved without weakening."
  - "Maskable icon is generated full-bleed on the dark brand surface (#0b0c0e) with ~80% safe-zone padding so OS launchers apply their own mask shape instead of double-rounding the squircle."

requirements-completed: [BRAND-01, BRAND-02, BRAND-03]

duration: 35min
completed: 2026-09-09
---

# Phase 3 Plan 05: Favicon/App-Icon Set, PWA Manifest, Assets-Only Service Worker Summary

**Hand-authored Oxidean web manifest + assets-only service worker (vite-plugin-pwa doesn't emit a SW under this repo's Vite 8 multi-environment build) alongside a full ImageMagick-generated favicon/app-icon set sharing the DOM mark's squircle mask.**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-09-09T19:55:00Z
- **Completed:** 2026-09-09T20:30:00Z
- **Tasks:** 3 completed
- **Files modified:** 13 (7 created icons, 1 script, 2 hand-authored PWA files, 3 modified: vite.config.ts, __root.tsx, .gitignore)

## Accomplishments

- `scripts/gen-icons.sh` reproducibly derives the entire D-20 icon set — favicon.ico, 16/32 favicons, 180px apple-touch-icon, 192/512 manifest icons, and a full-bleed 512 maskable icon — from `brand/oxidean-mark.png`, clipped with the same `apps/web/public/brand/squircle.svg` mask the DOM `OxideanMark` component uses.
- Diagnosed and documented a real incompatibility: `vite-plugin-pwa` (latest, 1.3.0) resolves its virtual manifest module but never runs workbox-build's SW-generation `closeBundle` hook under this repo's Vite 8 + `@octanejs/tanstack-start` dual "client"/"ssr" environment build — confirmed via `DEBUG=vite-plugin-pwa:*` producing zero plugin log output during build.
- Implemented the plan's documented fallback without weakening any threat mitigation: hand-authored `apps/web/public/manifest.webmanifest` (exact Oxidean manifest: `standalone`, `start_url`/`scope` `"/"`, dark brand `theme_color`/`background_color`, 192/512/maskable icons) and `apps/web/public/sw.js` (precaches only the icon/manifest shell; explicitly bypasses — never `caches.put`/`caches.match` in either direction — any request under `/api/` or `/health`; clears stale caches on `activate`).
- Extended `__root.tsx` `<Head>` with favicon/apple-touch-icon/manifest links and both light (`#f4f6f8`) and dark (`#0b0c0e`) `theme-color` metas per D-29, and registered `/sw.js` via a static, non-interpolated inline script placed after `<Scripts />`, while preserving `THEME_BOOT_SCRIPT` as the first `<Head>` child and the `"Oxidean"` home title.

## Task Commits

Each task was committed atomically:

1. **Task 1: Generate the favicon / app-icon set from the mark** - `6d2341d` (feat)
2. **Task 2: PWA manifest + assets-only service worker** - `31e8145` (feat) — *fallback path, see Deviations*
3. **Task 3: Head icon/manifest links, per-mode theme-color, SW registration** - `05c95f0` (feat)

**Plan metadata:** (this commit) `docs(03-05): complete plan summary`

## Files Created/Modified

- `scripts/gen-icons.sh` - ImageMagick pipeline: squircle-clips the 1024² mark, emits favicon.ico/16/32/apple-touch/192/512, and a full-bleed dark-surface maskable 512
- `apps/web/public/favicon.ico`, `favicon-16.png`, `favicon-32.png`, `apple-touch-icon.png` - squircle-clipped favicons
- `apps/web/public/icons/icon-192.png`, `icon-512.png` - squircle-clipped manifest icons (transparent corners)
- `apps/web/public/icons/icon-512-maskable.png` - full-bleed, opaque, safe-zone padded maskable icon
- `apps/web/public/manifest.webmanifest` - hand-authored Oxidean web manifest (fallback path)
- `apps/web/public/sw.js` - hand-authored assets-only service worker (fallback path)
- `apps/web/vite.config.ts` - documents the vite-plugin-pwa evaluation/rejection inline; plugin list unchanged from before this plan (`tanstackStart()`, `tailwindcss()`)
- `apps/web/src/routes/__root.tsx` - favicon/manifest `<link>`s, light/dark `theme-color` metas, `apple-mobile-web-app-title`, static SW-registration script
- `.gitignore` - added `dev-dist/` (vite-plugin-pwa dev artifact directory; kept even though the plugin was ultimately removed, since it's a harmless, still-correct ignore for any future PWA-dev-mode experiments)

## Decisions Made

- **vite-plugin-pwa → hand-authored fallback (plan-anticipated, Rule 3 blocking-issue path):** Added `vite-plugin-pwa@^1.0.0` (resolved to 1.3.0) as a devDependency, wired it into `apps/web/vite.config.ts` exactly per the plan's task 2 snippet (manifest, `injectRegister: null`, `devOptions.enabled: false`, assets-only `workbox.globPatterns`, `navigateFallbackDenylist`, two `NetworkOnly` runtime-caching entries for `/api/` and `/health`), ran `bun install`, and built. `dist/client/manifest.webmanifest` was correctly emitted with all Oxidean values, but no `sw.js` appeared anywhere in the build output regardless of dist location searched. Re-ran with `DEBUG=vite-plugin-pwa:*` — zero plugin debug output during either the "client" or "ssr" environment build, confirming the plugin's SW-generation hook simply never fires under `@octanejs/tanstack-start`'s Vite 8 environments API (this plugin version targets the pre-environments single-bundle Vite build model). No newer `vite-plugin-pwa` release exists (1.3.0 is latest as of this session) to retry.
  Per the plan's explicit instruction ("If vite-plugin-pwa cannot coexist… use the plan's hand-authored fallback WITHOUT weakening D-28–D-30"), removed the plugin and dependency (net-zero `bun.lock`/`package.json` diff — added then cleanly removed) and hand-authored `apps/web/public/manifest.webmanifest` (identical JSON to what the plugin generated) and `apps/web/public/sw.js` (precache-on-install for the exact same static asset list, cache-first serve for only those assets, and an explicit `fetch(event.request)` passthrough — no `caches.put`/`caches.match` on either side — for any request under `/api/` or equal to `/health`, plus stale-cache cleanup on `activate` to satisfy the T-03-18 "no pinned stale shell after deploy" mitigation).
  This is the **fallback path**, not the primary vite-plugin-pwa path. `apps/web/vite.config.ts` plugin list is unchanged from before this plan; the reasoning is documented as an inline code comment for future maintainers.
- **Maskable icon safe-zone:** generated by extending the plain (non-squircle-clipped) square mark to `410×410` centered on a `512×512` canvas filled with the dark brand surface `#0b0c0e`, guaranteeing full opacity (verified `magick identify %[opaque]` → `true`) so Android/OS launchers can safely apply their own rounding without clipping the mark or exposing transparency.

## Deviations from Plan

### Auto-fixed / Plan-anticipated

**1. [Plan-documented fallback, not a Rule 1-4 auto-fix] vite-plugin-pwa incompatibility → hand-authored manifest + service worker**
- **Found during:** Task 2
- **Issue:** `vite-plugin-pwa` resolves the manifest virtual module but never generates `sw.js` under this repo's Vite 8 + `@octanejs/tanstack-start` multi-environment build (confirmed via debug logging: zero plugin hook activity during build).
- **Fix:** Removed the plugin/dependency; hand-authored `apps/web/public/manifest.webmanifest` and `apps/web/public/sw.js` with equivalent content and equal-or-stronger threat mitigation (T-03-16 verified against the *actual served* `sw.js`, not a generated artifact whose provenance is now moot).
- **Files modified:** `apps/web/package.json`, `bun.lock` (net-zero after add+remove), `apps/web/vite.config.ts`, `apps/web/public/manifest.webmanifest` (new), `apps/web/public/sw.js` (new)
- **Verification:** `bun run --filter @oxidean/web build` emits `dist/client/sw.js` (the static file copied through unchanged) and `dist/client/manifest.webmanifest` with correct content; `grep -cE '"url":"[^"]*/api/|"url":"[^"]*/health'` → `0`; `grep -cE 'CacheFirst|StaleWhileRevalidate|NetworkFirst'` → `0`; `bun install --frozen-lockfile` exits `0`.
- **Committed in:** `31e8145` (Task 2 commit)

---

**Total deviations:** 1 plan-anticipated fallback (explicitly authorized by the plan's `<action>` step 4 and the objective's `<critical_security>` block). No unauthorized scope creep.
**Impact on plan:** None on outcome — D-20/D-28/D-29/D-30 and BRAND-01/02/03 are all satisfied; the delivery mechanism for the manifest/SW is static files instead of a build plugin. Some acceptance-criteria greps in the plan text that specifically target `apps/web/vite.config.ts` (e.g. `NetworkOnly` count, manifest keys in vite.config.ts) do not apply verbatim on this path since that content now lives in `apps/web/public/manifest.webmanifest` and `apps/web/public/sw.js` instead — verified against those files in this session (see Verification Log below) and confirmed equivalent in substance.

## Verification Log (fallback-path equivalents of plan acceptance criteria)

| Plan criterion (vite.config.ts-targeted) | Fallback-path check | Result |
|---|---|---|
| `grep -q '"vite-plugin-pwa"' apps/web/package.json` | N/A — plugin removed after proving incompatibility | documented above |
| Manifest keys (`Oxidean`, `standalone`, `start_url: "/"`, `scope: "/"`, icons) | same keys checked in `apps/web/public/manifest.webmanifest` | all present |
| `theme_color`/`background_color` `#0b0c0e` | checked in `apps/web/public/manifest.webmanifest` | both present |
| `NetworkOnly` × 2 / `navigateFallbackDenylist` | N/A (no Workbox config) — equivalent enforced directly in `sw.js` fetch handler | `sw.js` bypasses `/api/` and `/health` explicitly (grep confirms) |
| `bun run --filter @oxidean/web build` exits 0 | ran | exit 0 |
| Built `sw.js` exists (not under node_modules/dev-dist) | `find apps/web/dist -name sw.js` | `apps/web/dist/client/sw.js` |
| Built SW precaches nothing under `/api` or `/health` | `grep -cE '"url":"[^"]*/api/|"url":"[^"]*/health' sw.js` | `0` |
| Built SW has no CacheFirst/SWR/NetworkFirst handler | `grep -cE 'CacheFirst|StaleWhileRevalidate|NetworkFirst' sw.js` | `0` |
| `dev-dist` in `.gitignore` | `grep -q dev-dist .gitignore` | present |
| `__root.tsx` head links, theme-color × 2, SW registration, boot-script ordering, `dangerouslySetInnerHTML` × 2 | all plan greps run verbatim | all pass |

## Issues Encountered

- `bash scripts/gen-icons.sh` produces byte-different (same-size) PNGs on each run — ImageMagick embeds a fresh timestamp/metadata chunk per invocation even with identical pixel content. Re-ran the script during verification, confirmed the diffs were re-encoding artifacts only (identical file sizes, no dimension/opacity change), and restored the originally-committed bytes with `git checkout --` rather than re-committing a spurious diff. No functional impact; documenting in case a future `gen-icons.sh` re-run surprises someone with an unexpected git diff.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None - all assets are generated/wired end-to-end (icons render from the real brand mark, manifest and SW are fully functional, no placeholder data).

## Next Phase Readiness

- Phase 3's brand shell is now complete at the platform edge: favicon/app-icon set, installable manifest, per-mode theme-color, and an API-safe service worker are all in place and build-verified.
- Any future work that touches `apps/web/vite.config.ts` build plugins should be aware `vite-plugin-pwa` was tried and rejected for this stack; re-evaluate only if the plugin or `@octanejs/tanstack-start`/Vite ship explicit multi-environment support.
- No blockers for subsequent phases.

---
*Phase: 03-brand-shell-theme*
*Completed: 2026-09-09*

## Self-Check: PASSED

All 13 key files verified present on disk (`scripts/gen-icons.sh`, 7 generated icon assets, `manifest.webmanifest`, `sw.js`, `vite.config.ts`, `__root.tsx`, `.gitignore`). All 3 task commits (`6d2341d`, `31e8145`, `05c95f0`) verified present in `git log`.
