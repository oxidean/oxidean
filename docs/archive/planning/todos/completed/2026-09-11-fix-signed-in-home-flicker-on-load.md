---
created: 2026-09-11T01:07:22.477Z
title: Fix signed-in home flicker on load
area: ui
severity: minor
updated: 2026-09-11T01:10:00.000Z
files:
  - apps/web/src/routes/index.tsrx
  - apps/web/src/routes/dashboard.tsrx
  - apps/web/src/lib/session-hint.ts
  - apps/web/src/components/page-skeletons.tsrx
  - apps/web/src/components/signed-in-home.tsrx
status: completed
completed: 2026-09-11
resolved_by: 06-05 SSR home + 06-08 chrome omit + 06-09 dashboard/signup gates
---

## Problem

Logged-out marketing home no longer throws `insertBefore` (landing paints during auth check). Signed-in `/` (“dashboard” home) still **flickers** on load: brief wrong chrome / landing-or-empty → skeleton → `SignedInHome` while `oxidean_signed_in` hint + `auth.me` resolve. SSR cannot read the HttpOnly session; presence cookie heals only after mount, so first paint for returning sessions is unstable.

## Solution

**Preferred direction (user):** server-side redirect based on session so the client never paints the wrong home.

Sketch:

1. Treat marketing home and signed-in home as separate route modules conceptually (`/` vs dashboard content), but **rewrite both to `/` depending on logged-in status** — one public URL surface for “home.”
2. **Server-side:** if the request has a valid session, serve/redirect to the signed-in home tree; if anonymous, serve the marketing landing. First HTML matches the final UI → no flicker.
3. **`/dashboard` must not be a real accessible page.** Hitting `/dashboard` directly should **404**. Keep it only as an internal rewrite target or remove the public file route once SSR gating owns `/`. (Today `dashboard.tsrx` soft-redirects to `/` — that soft redirect goes away in favor of 404 for direct hits.)
4. Soft redirects / client `auth.me` gates are a fallback only; they are what causes the flicker today.

Also re-check view-transition on `.oxidean-main` if any residual swap remains after SSR routing is correct.

## Resolution

Completed in Phase 06: SSR session gate on `/` (06-05), chrome omit while `needs_setup` (06-08), `/dashboard` 404 and closed `/signup` (06-09). Folded from CONTEXT; closed by 06-07 docs/closeout.

