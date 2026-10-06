# Phase 3: Brand Shell & Theme - Context

**Gathered:** 2026-09-09
**Status:** Ready for planning

<domain>
## Phase Boundary

The product UI reads as Oxidean — mark, colors, naming — with light/dark themes that default to the OS preference and persist a user override (light or dark).

**Requirements:** BRAND-01, BRAND-02, BRAND-03, BRAND-04, BRAND-05

**Success criteria (from ROADMAP):**
1. Primary chrome (favicon, header/nav mark) uses `brand/oxidean-mark.png` and blue/orange brand direction
2. Product copy and titles present the name Oxidean (not “GitHub clone” or bare Octane)
3. UI renders correctly in light and dark modes
4. Theme defaults to the OS preference (system) until the user chooses light or dark
5. User can set theme to light or dark and the preference persists across refresh

**UI hint:** yes — expect `/gsd-ui-phase 3` (UI-SPEC) before or with planning.

</domain>

<decisions>
## Implementation Decisions

### A — Mark & chrome identity
- **D-01:** Shared mark component used everywhere the mark appears in the UI (header, landing, empty/marketing spots) — not a one-off header `<img>`
- **D-02:** Clip `brand/oxidean-mark.png` to a **macOS app-icon rounded square (squircle)**; no extra padded plate behind the asset; same treatment in light and dark
- **D-03:** Header: **mark + “Oxidean” wordmark on wide viewports; mark-only on narrow**
- **D-04:** Landing hero: **large clipped mark + headline only** — no duplicate “Oxidean” text in the hero
- **D-05:** Mark images always use `alt="Oxidean"`
- **D-06:** Footer stays **text-only** (“© Oxidean” + Status link) — no footer mark

### B — Theme control
- **D-07:** Theme preference remains `system` | `light` | `dark` with persistence (existing `oxidean-theme` localStorage key / `apps/web/src/lib/theme.ts` behavior is the starting point)
- **D-08:** Header control is a **ShadCN + Base UI Select** (not a native `<select>`)
- **D-09:** Project UI chrome rule: **prefer reusable ShadCN + Base UI components over plain HTML controls**
- **D-10:** Placement: **after search, before Sign in**
- **D-11:** Menu options: **icon + text** (System / Light / Dark); closed trigger shows **current choice with icon + short label**
- **D-12:** **Inline boot script before paint** to apply theme and avoid FOUC

### C — Design tokens & components
- **D-13:** Implement a **full ShadCN semantic token layer** (primary, secondary, muted, card, ring, etc.) mapped from Oxidean brand colors
- **D-14:** **Primary = cool/left (blues–cyans); Secondary = warm/right (oranges)** — values must match the brand logo direction (not generic blue/orange)
- **D-15:** **Remove** legacy `--color-accent-cool` / `--color-accent-warm`; migrate all call sites to semantic token names in this phase
- **D-16:** Replace hand-rolled `Button` with **ShadCN/Base UI Button**
- **D-17:** Use **Class Variance Authority (CVA)** on reusable components when variants apply
- **D-18:** Disabled header search → **ShadCN/Base UI Input** (still disabled / “soon”)
- **D-19:** Sign in / Sign up → **ShadCN Button group / toolbar** pattern; remain disabled placeholders until Auth phase

### D — Shell polish, landing, status, PWA
- **D-20:** **Full favicon / app icon set** from the mark: favicon.ico, PNG sizes, apple-touch-icon, web manifest icons; squircle where it matters
- **D-21:** Document titles: **`Oxidean`** on home; **`Status · Oxidean`** (and same `Page · Oxidean` pattern) on other routes
- **D-22:** **Full multi-section marketing landing refresh** (not chrome-only): story beats **Hero → Dual-mode (Cloud + self-host) → Collaboration pillars → Closing CTA**
- **D-23:** Landing layout: **editorial bands / split rows** — no card grid; avoid card clutter
- **D-24:** CTAs while auth is placeholder: **Get started stays disabled/placeholder**; **Explore** scrolls to dual-mode/pillars
- **D-25:** **2–3 intentional motions** on the landing (e.g. hero entrance, section reveal, CTA hover) with `prefers-reduced-motion` respect
- **D-26:** `/status` gets **full brand treatment** as a marketed system-status page; still **live `system.health` only** (no status history — that remains later)
- **D-27:** Status presentation: **clear status hero** — branded “All systems operational” / degraded / unreachable, with version + database detail below
- **D-28:** **Vite PWA setup** with a standard installable web manifest: `name` / `short_name` Oxidean, icons from D-20, `display: standalone`, `start_url: /`
- **D-29:** `theme_color` / `background_color` (and theme-color meta with light/dark `media` where supported) **follow brand surfaces** — theme ≈ dark surface / primary cool; background ≈ app bg
- **D-30:** Service worker: **assets-only / precache app shell**; **network-first or bypass** for `/api/*` and RPC — do not cache API/auth responses

### Claude's Discretion
- Exact squircle CSS/SVG mask implementation and mark size scale (header vs hero)
- Exact hex sampling from the mark vs tuned tokens that clearly read as the logo’s cool/warm split
- Which lucide icons represent System / Light / Dark
- Exact vite-plugin-pwa (or equivalent) version and Workbox strategy details within D-28–D-30
- Landing motion library vs CSS keyframes, within D-25
- Status hero visual details within D-26–D-27 (as long as states are unmistakable)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product & requirements
- `.planning/PROJECT.md` — brand meaning, logo path, dual-mode model, UI/theme constraints
- `.planning/REQUIREMENTS.md` — BRAND-01…BRAND-05
- `.planning/ROADMAP.md` — Phase 3 goal, success criteria, UI hint, dependency on Phase 1

### Prior phase decisions
- `.planning/phases/01-monorepo-scaffold/01-CONTEXT.md` — D-22–D-26 marketing/`/status` shape; brand mark deferred polish to Phase 3
- `.planning/phases/02-multi-db-storage/02-CONTEXT.md` — out of scope for brand UI; do not reopen multi-DB here

### Brand assets
- `brand/oxidean-mark.png` — primary mark (blue/orange X on black)
- `brand/README.md` — usage notes (prefer dark ground; until SVG/variants exist)

### Existing web implementation (extend, don’t fork)
- `apps/web/src/components/chrome.tsx` — header/footer shell
- `apps/web/src/lib/theme.ts` — preference read/resolve/apply + storage key
- `apps/web/src/styles.css` — Tailwind v4 `@theme` + light/dark vars (migrate off accent-cool/warm)
- `apps/web/src/routes/index.tsx` — landing (to be refreshed)
- `apps/web/src/routes/status.tsx` — health UI (to be brand-treated)
- `apps/web/src/routes/__root.tsx` — root shell / head title
- `apps/web/components.json` — ShadCN base-nova, CSS variables, lucide
- `apps/web/vite.config.ts` — Vite + TanStack Start plugin (PWA plugs in here)

No phase-local SPEC.md yet — decisions above are the implementation lock. UI-SPEC may be produced via `/gsd-ui-phase 3`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SiteHeader` / `SiteFooter` in `chrome.tsx` — theme select and mark already present; replace with shared mark + ShadCN controls
- `theme.ts` — preference model and `dark` class toggle already match BRAND-03–05; add FOUC boot path
- `styles.css` — fonts (Sora + Source Sans 3) and dual-tone accents exist; remap to ShadCN semantics
- `public/oxidean-mark.png` — copied mark asset; derive favicon/PWA icons from brand source
- Hand-rolled `components/ui/button.tsx` — replace with ShadCN/Base UI + CVA

### Established Patterns
- Octane + TanStack Start file routes; root provides header/main/footer
- Theme via `html.dark` class + CSS variables
- ShadCN configured (`components.json`) but only a custom Button exists today
- Landing uses radial cool/warm gradients and a single fade-in motion

### Integration Points
- Header chrome for mark, theme Select, search Input, auth Button group
- `__root` / route `head` for titles, favicon links, theme boot script, manifest
- `vite.config.ts` for Vite PWA plugin
- Landing and `/status` routes for brand-facing surfaces
- Proxy already forwards `/api/rpc` and `/health` — SW must not break these

</code_context>

<specifics>
## Specific Ideas

- Mark treatment is explicitly **macOS app icon squircle**, not a generic rounded rect or circle
- Primary/secondary must **match the logo** (cool left / warm right)
- Prefer **ShadCN + Base UI** and **CVA** as standing UI conventions from this phase forward
- Landing is a real multi-section marketing page, but **editorial** (no card grid)
- PWA is in scope for Phase 3 (Vite PWA), with a forge-safe **assets-only** service worker
- Do not name the product bare “Octane” or market it as a “GitHub clone”

</specifics>

<deferred>
## Deferred Ideas

- Transparent / light-specific mark variants or full vector logo set — after PNG-based squircle shell
- Real search behavior — later phases; Phase 3 only styles the disabled control
- Auth signup/login — Phase 4; buttons stay disabled placeholders
- Status history / authenticated status report — deferred beyond Phase 1/3 live health
- Aggressive offline SPA caching of app data — out of scope; assets-only SW only

None of these expand Phase 3 requirements; they stay out of scope unless promoted later.

</deferred>

---

*Phase: 3-Brand Shell & Theme*
*Context gathered: 2026-09-09*
