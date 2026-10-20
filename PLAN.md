# Plan: fix the remaining render storm on the non-authed homepage hero (issue #143)

> Planning profile for [oxidean/oxidean#143](https://github.com/oxidean/oxidean/issues/143).
> Read-only exploration only; no implementation until approved.

## Goal

Eliminate the last known fresh-render storm called out in #143 — the anonymous
homepage hero (`AnonLanding`) unmounting/remounting and re-rendering on every
cold load — so a non-authed `/` settles with a minimal number of commits and
no hero DOM churn.

## Current state

- Issue #143 identified 6 root causes. **PR #144** (`8fabcb78`, "stop re-render
  storms on fresh page load") fixed most of them: `memo()` across the chrome
  (`SiteHeader`, `GlobalSearch`, `AccountActions`, `TemplateCard`,
  `OxideanMark`, skeletons), lazy dropdown internals, lazy `/new` template
  picker, and dropping the redundant `astro:page-load` notify in
  `pathname.tsrx` (its `subscribe` now only listens to `popstate` /
  `astro:after-swap`).
- **PR #146** (`8b5d2e1d`) stamped the setup verdict server-side
  (`data-oxidean-setup`) and seeded `authBootstrapQueryOptions` from it,
  removing another settle wave.
- The issue remains **open** with the author's follow-up comment:
  *"Only remaining render storm I've noticed is the hero section on the
  non-auth homepage."*
- Local branch `fix/web-render-storms-143` already carries #144/#146 plus an
  unmerged tip commit `f095d1da` ("gate remaining auth-branch surfaces against
  hydration rollback") that added the `useMounted()` pin in
  `apps/web/src/routes/index.tsrx` so a warm `auth.me` cache can't roll the
  home island back. This plan builds on that branch (working tree is clean).

**Why the hero still storms today** (`apps/web/src/routes/index.tsrx`,
`HomeContent`):

1. **Tree-shape swap on settle.** SSR/prerender always emits the *pending
   pair*: `<div data-anon-landing><AnonLanding/></div>` +
   `<div data-home-skeleton><SignedInHomeSkeleton/></div>` (CSS picks the
   visible one via the `data-oxidean-session` stamp). When `auth.me` resolves
   anonymous, `HomeContent` re-renders as a **bare** `<AnonLanding/>` — the
   landing subtree moves position in the tree, so Octane **unmounts and
   remounts the entire `AnonLanding`** (hero + explore/feature sections),
   recreating ~every DOM node, restarting the `oct-rise` hero animation, and
   re-running the `useScrollReveal` effect (its `epoch` arg changes
   `null → "marketing"`), which re-queries and re-observes the `.oct-reveal`
   nodes. This happens on **every** anonymous load.
2. **CTA insertion on settle.** `AnonLanding` takes `allowSignup` (from
   `authProviderConfigQueryOptions`), which is `false` while pending (no SSR
   data) and flips to `true` when `providerConfig` settles → a second hero-area
   DOM mutation (the "Get started" `AppLink` is inserted). The
   `data-anon-landing`/`data-home-skeleton` wrapper removal is asserted by the
   existing test contract (`index.integration.test.ts`: "wrappers dropped").
3. The hidden `SignedInHomeSkeleton` also mounts then unmounts — cheap
   (memoized `Skeleton`s), accepted cost of the rollback pin.

**Assumption (stated, not silently guessed):** I could not run the Octane
profiler myself (read-only constraint). I'm assuming the "hero section storm"
in the issue comment is exactly this settle-time unmount/remount + CTA insert
above. If measurement shows a different dominant cause, the plan's Phase 0
catches it.

## Coverage check against the issue's six root causes

| # | Root cause | Status |
|---|---|---|
| 1 | Zero memoization / cascade width | **Covered** — #144 memoized chrome; this plan adds the last piece: `memo(AnonLanding)` + removing its `allowSignup` prop so the settle commits bail. |
| 2 | `usePathname` fires on initial load | **Fixed by prior work, with an accepted residual** — the `astro:page-load` subscription is gone; the deliberate residual is that the SSR snapshot stays `"/"` so non-`/` pages get exactly **one** hydration converge render for render-based consumers (`AppLink` aria-current). Documented as intentional in `pathname.tsrx` (a live snapshot would risk rollback with warm caches). Phase 3 asserts ≤1 converge render, not zero. |
| 3 | Eager Base UI menu internals | **Fixed** in `00454d5e` (lazy dropdown internals), merged. Nothing to do. |
| 4 | `/new` eager 94 `TemplateCard`s | **Fixed** in `00454d5e` (`@if (open)` lazy lists + memoized cards). Nothing to do. |
| 5 | Whole-island mount rollback on `/new` | **Fixed** by `bb3af540` + `f095d1da` (`useMounted` pin). This plan *preserves* that pin on `/` — the restructured branch swap is post-mount only, and SSR output stays byte-identical, so the rollback surface doesn't widen. |
| 6 | Each async resolution commits separately | **Partially fixed; accepted residual** — #146 stamped the bootstrap verdict. Remaining commits on an anon `/`: mount, `auth.me`, `providerConfig` (chrome island only after this change), pathname converge on non-`/` pages. Single-digit `commit.root`, near the issue's expected 2–4. The success metric is *render cost / DOM mutations / hero churn*, not "commit.root → 2". |

And the open issue's actual ask — "Only remaining render storm I've noticed is
the hero section on the non-auth homepage" — is the pending→marketing
**unmount/remount** of `AnonLanding` (wrapper drops, tree position changes)
plus the CTA insertion. Confirmed against current `index.tsrx` (lines 123,
148, 151).

## Approach

Make the anonymous landing's **DOM stable across pending → marketing**, using
patterns already established in this codebase:

- **Keep the `data-anon-landing` wrapper in both branches.** The pending
  branch renders `[wrapper>AnonLanding, skeleton]`; the resolved marketing
  branch renders `[wrapper>AnonLanding]` only. Octane then keeps the first
  child in place (no remount, `oct-rise` doesn't restart, reveal observer
  doesn't need rebinding) and only unmounts the hidden skeleton.
- **Gate the "Get started" CTA in CSS, not in JS**, following the existing
  `data-oxidean-signup` / `[data-header-skeleton-signup]` pattern (the web
  tier already stamps `data-oxidean-signup="1"` from the
  `oxidean_allow_signup` companion cookie —
  `crates/oxidean-web/src/inject.rs:173`). `AnonLanding` drops its
  `allowSignup` prop entirely → zero props → `memo(AnonLanding)` bails on
  every `HomeContent` re-render (both the `auth.me` settle and the
  `providerConfig` settle).
- **Sync the stamp client-side** (see Phase 2): the stamp is injected
  per-request by the web tier only, and the cookie it derives from is written
  client-side after `providerConfig` resolves — so a first visit is unstamped
  and the CTA would stay hidden for the whole SPA session. The chrome's
  existing `cfg.isSuccess` effect gains a one-line stamp sync (set
  `data-oxidean-signup="1"` when allowed, remove otherwise) — one attribute
  mutation on `<html>`, zero island re-renders, no DOM-race surface.
- **Memoize `AnonLanding`** with the existing `import { memo } from "octane"`
  pattern used in `chrome.tsrx` (function impl + `export const X =
  memo(XImpl)`).
- **Preserve the hydration-rollback pin** (`useMounted` / `renderedTree`)
  untouched — it's load-bearing from `f095d1da`, and the new shape still
  matches prerendered HTML exactly (SSR output is byte-identical: same
  pending pair).
- Keep `useScrollReveal` as-is: with stable nodes the epoch re-run is a
  harmless no-op (re-observes live nodes, revealed ones already have
  `oct-reveal-in` and are unobserved), and it's still needed for the
  marketing → signed-in swap.

## Alternatives considered

- **Seed `authProviderConfigQueryOptions` with `initialData` from the
  `data-oxidean-signup` stamp (like #146 did for bootstrap).** Rejected:
  seeding makes the query resolved at first client render, which would insert
  the CTA button against SSR HTML that was prerendered with
  `allowSignup=false` → hydration presentation miss / rollback risk on exactly
  the island we're stabilizing; also the stamp only carries `allow_signup`,
  not `mode`, so the seed object would be a partial fabrication. CSS stamp
  gating avoids both.
- **Defer/memoize at the island level only, or add commit batching.**
  Rejected: Octane's scheduler already coalesces same-window updates per drain;
  the remaining damage is the *structural* swap (unmount/remount), which
  batching can't fix, and there's no per-island escape hatch that beats a
  stable tree shape.

## Changes

| File | Change |
|---|---|
| `apps/web/src/routes/index.tsrx` | 1) `HomeContent`: resolved marketing branch renders `<div data-anon-landing><AnonLanding/></div>` (wrapper retained) instead of bare `AnonLanding`; pending branch unchanged. 2) `AnonLanding`: drop `allowSignup` prop; "Get started" `AppLink` (hero **and** bottom CTA section) always rendered with a new attribute (e.g. `data-anon-signup-cta`) instead of `@if (allowSignup)`; export as `memo`-wrapped component. 3) Remove the now-unused `cfgQ` / `authProviderConfigQueryOptions` subscriber from `HomeContent` (the page island stops subscribing to `providerConfig` — one fewer page-island settle commit; the query still runs in the chrome island). 4) Optional: drop the dead `HomeLoaderData` type (declared, never referenced). |
| `apps/web/src/styles.css` | One rule in the components layer beside the existing `[data-header-skeleton-signup]` rule: `html:not([data-oxidean-signup]) [data-anon-signup-cta] { display: none; }`. |
| `apps/web/src/lib/session-hint.ts` (or new `signup-stamp.ts`) | Small helper: read/set/clear `data-oxidean-signup` on `<html>` (mirrors `stampedSetupVerdict` / `clearSetupStamp` in `setup-stamp.ts`). |
| `apps/web/src/lib/use-chrome-account.tsrx` | Extend the existing `cfg.isSuccess` effect (where `syncAllowSignupHint` already runs) to also sync the `<html>` stamp: set `"1"` when `allowSignup`, remove otherwise (clearing is correct even over a server stamp — a stale `"1"` contradicts the fresh RPC). |
| `apps/web/src/routes/index.integration.test.ts` | Update "anonymous resolution swaps to the bare landing tree (wrappers dropped)" → skeleton wrapper dropped, `data-anon-landing` **retained**; assert hero node identity preserved (e.g. hero `<h1>` element reference unchanged after settle). |
| `apps/web/src/routes/index.browser.test.tsx` | 1) Pending→marketing swap: assert `.oct-reveal`/hero nodes are **not** replaced (same element identity before/after `me.resolve(anonymous)`); keep the reveal-still-works assertion. 2) New: first-visit CTA flow — no stamp at mount, `providerConfig` resolves with signup open → CTA becomes visible on the **same DOM node** (attribute sync, no replace). 3) New: signup-closed flow — no stamp + RPC `allow_signup:false` → CTA stays hidden; client-stamped `"1"` is cleared on closed resolve. |

No Rust, RPC, or generated-client changes.

## Phases

1. **Phase 0 — Measure baseline (executor).** Per the issue's "Steps to
   reproduce": `profile: "auto"` in `apps/web/astro.config.mjs`,
   `make up-with-dev-auth`, serve `apps/web/dist` through `oxidean-web`
   (`OXIDEAN_WEB_DIST=apps/web/dist`), headless-Chromium profiler run on `/`
   (anonymous) **and** `/{owner}/{repo}/branches` (worst row from the issue,
   as a no-regression guard on shared chrome). Record `component.render`,
   `commit.root`, post-mount update events, DOM-mutations-after-load, and hero
   island mount/unmount counts. Confirms the assumption and gives before/after
   numbers.
2. **Phase 1 — Wrapper retention + memo** (`index.tsrx`): restructure the
   marketing branch, wrap `AnonLanding` in `memo`, drop the `cfgQ` subscriber.
   Verify: happy-dom integration tests (updated) + `make web-lint` +
   `make web-format-check` pass; browser test shows hero node identity stable.
3. **Phase 2 — CSS-stamped CTA + client stamp sync** (`styles.css`,
   `index.tsrx`, `session-hint.ts`/`signup-stamp.ts`, `use-chrome-account.tsrx`):
   remove `allowSignup` prop, add the
   `html:not([data-oxidean-signup]) [data-anon-signup-cta]` rule, add the
   `<html>` stamp sync effect. Verify: browser tests with and without the
   stamp (signup open/closed, first-visit no-cookie flow) show correct CTA
   visibility with **zero** hero DOM mutation on `providerConfig` settle (only
   the single `<html>` attribute mutates).
4. **Phase 3 — Re-measure** as Phase 0. Expected: anonymous `/` hero
   unmount/remount events → 0; hero post-settle DOM mutations → 0 (except the
   `<html>` attribute); `/` (anon) `component.render` back toward the ~91
   baseline (chrome-only work); ≤1 pathname converge render on non-`/` pages
   (accepted by design); no `rollback.root` over ~20 runs of `/` and `/new`
   (authed) to confirm the pin still holds.
5. **Phase 4 — Full regression pass** (below) + commit with SemVer subject
   (`perf(web): ...`).

## Risks & unknowns

- **Unverified baseline:** I couldn't run the profiler (read-only constraint).
  If the hero storm has a non-structural component (e.g. a CSS animation loop
  or a chrome subscriber missed), Phase 0 numbers will show it and the plan
  needs a re-triage — that's why Phase 0 comes first.
- **First-visit / old-tier CTA:** on the very first response the
  `oxidean_allow_signup` cookie is absent, so the web tier stamps nothing.
  The client-side stamp sync (Phase 2) closes this after `providerConfig`
  resolves — bounded by one RPC. On an *older* web tier that never stamps,
  the client sync still applies (it stamps `<html>` itself), so the CTA
  behaves correctly; note the interaction in the commit message/PR.
- **Stale-stamp edge:** signup closes after the stamp/cookie says open → the
  hero CTA stays visible until `providerConfig` re-resolves and the client
  clears the stamp. Same class of staleness the header skeleton already
  accepts ("unstamped means closed or unknown"); bounded by
  `staleTime: 30s`.
- **Octane diff semantics:** the plan relies on Octane keeping the first
  fragment child in place when the second is dropped. High confidence
  (standard key/position diffing; same mechanics as the keepMounted fixes in
  #144), but if Octane's fragment matching remounts anyway, the fallback is to
  keep the skeleton wrapper in the DOM at all times and hide it via CSS on
  settle (`@if` only for signed-in) — slightly more DOM but guaranteed stable.
- **Rollback pin interaction:** any change that alters first-client-render vs
  prerender HTML could reintroduce the `f095d1da` rollback race. All changes
  here are post-mount-only (branch swap after `renderedTree` resolves), and
  SSR output is unchanged — but the `/new`-style intermittent rollback means
  the browser tests must run repeatedly (Chromium gate) to gain confidence.

## Verification

1. `make web-lint` and `make web-format-check` — clean.
2. Targeted unit/DOM: `bun run --filter @oxidean/web test` filtered to
   `index.integration.test.ts`, `signed-in-home.integration.test.ts`,
   `chrome.*` tests.
3. Browser gate: `make test-web-browser` (Chromium) —
   `index.browser.test.tsx` (node identity, reveal, CTA first-visit/closed
   flows, no DOM races via `trackDomErrors`), `chrome.browser.test.tsx`.
4. Profiler before/after per the issue's repro steps (Phase 0/3), anonymous
   `/` and `/{owner}/{repo}/branches` through `oxidean-web` with
   `OXIDEAN_WEB_DIST=apps/web/dist`: hero unmount/remount events → 0;
   `component.render` / post-mount update events on `/` (anon) materially
   reduced vs the 91/31 baseline row; no `rollback.root` over ~20 runs of `/`
   and `/new` (authed).
5. `make test` not needed (no Rust changes); skip `rpc-sync-check` (no RPC
   changes).

## Status

- [ ] Plan approved
- [ ] Phase 0 — baseline measurement
- [ ] Phase 1 — wrapper retention + memo
- [ ] Phase 2 — CSS-stamped CTA + client stamp sync
- [ ] Phase 3 — re-measure
- [ ] Phase 4 — regression pass + commit
