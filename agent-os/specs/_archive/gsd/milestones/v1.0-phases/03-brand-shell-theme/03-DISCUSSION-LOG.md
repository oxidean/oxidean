# Phase 3: Brand Shell & Theme - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-09-09
**Phase:** 3-Brand Shell & Theme
**Areas discussed:** Mark on light surfaces, Theme control chrome, Token depth & ShadCN mapping, Shell polish scope, Landing section content, Remaining plain controls, Status page brand polish, Web manifest / theme-color

---

## Mark on light surfaces

| Option | Description | Selected |
|--------|-------------|----------|
| Black plate / intended ground | Sit PNG on dark ground | ✓ (evolved to macOS squircle clip) |
| As-is on light | Show black square raw | |
| Light variant later | Defer art | |
| Other: macOS app icon | Rounded-square squircle | ✓ |

**User's choice:** Clip PNG to macOS app-icon squircle; shared mark everywhere in UI; mark+wordmark wide / mark-only narrow; hero = large mark + headline only; `alt="Oxidean"` always; footer text-only; same black in light/dark; clip asset (no extra pad).

**Notes:** User rejected generic tight/flush/circle plates in favor of macOS app icon shape.

---

## Theme control chrome

| Option | Description | Selected |
|--------|-------------|----------|
| Native `<select>` | Keep as today | |
| Icon cycle | One button cycles modes | |
| Segmented / menu | Clear three choices | |
| Other: Base UI Select | ShadCN + Base UI Select | ✓ |

**User's choice:** ShadCN/Base UI Select after search before Sign in; icon+text options; trigger shows icon+short label; prefer ShadCN/Base UI over plain HTML project-wide for chrome.

---

## Token depth & ShadCN mapping

| Option | Description | Selected |
|--------|-------------|----------|
| Brand-map existing vars | Small token set only | |
| Full ShadCN semantic layer | primary/secondary/muted/… | ✓ |
| You decide | Minimal on-brand | |

**User's choice:** Full semantic layer; primary=cool/left secondary=warm/right matching logo; remove accent-cool/warm; replace Button with ShadCN; use CVA when variants apply.

**Notes:** User insisted primary/secondary match the brand logo (not generic accents).

---

## Shell polish scope

| Option | Description | Selected |
|--------|-------------|----------|
| Single favicon | PNG/ICO only | |
| Full icon set | favicon + apple-touch + manifest | ✓ |
| Defer multi-size | link to PNG only | |

Also locked: inline FOUC boot script; `Page · Oxidean` titles; full multi-section landing + 2–3 motions.

---

## Landing section content

| Option | Description | Selected |
|--------|-------------|----------|
| Hero → Dual-mode → Pillars → CTA | Full thesis | ✓ |
| Hero → Trust → Dual-mode → CTA | Lighter pillars | |
| Hero → Pillars → CTA | Dual-mode in copy only | |

**User's choice:** Editorial bands (no cards); Get started placeholder + Explore scrolls.

---

## Remaining plain controls

| Option | Description | Selected |
|--------|-------------|----------|
| ShadCN Input (disabled search) | Consistent chrome | ✓ |
| Leave plain input | Until search ships | |
| Auth: already covered | Just ShadCN Button | |
| Auth: Button group/toolbar | Denser chrome | ✓ |

---

## Status page brand polish

| Option | Description | Selected |
|--------|-------------|----------|
| Tokens + title only | Minimal | |
| Light visual polish | | |
| Full brand treatment | Marketed status page | ✓ |

**User's choice:** Clear status hero (operational / degraded / unreachable) + version/database; still live health only.

---

## Web manifest / theme-color

| Option | Description | Selected |
|--------|-------------|----------|
| Standard installable + Vite PWA | | ✓ |
| Icons + theme-color only | | |
| Minimal stub | | |

**User's choice:** Vite PWA setup; theme colors follow brand surfaces; assets-only SW with `/api/*` + RPC bypass/network-first.

---

## Claude's Discretion

Exact squircle technique, hex sampling, lucide icon picks, vite-plugin-pwa/Workbox details within assets-only policy, motion implementation, status hero visual details.

## Deferred Ideas

Light mark variants/SVG set; real search; auth; status history; aggressive offline API caching.
