---
phase: 17
slug: notifications
status: draft
shadcn_initialized: true
preset: base-nova
created: 2026-09-16
---

# Phase 17 — UI Design Contract

> Visual and interaction contract for Notifications. Locked upstream: `17-CONTEXT.md` D-08…D-15.
> Reuse Phase 3 brand chrome (Sora / Source Sans 3, base-nova). Do not invent a new visual system.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | shadcn (initialized) |
| Preset / style | **base-nova** |
| Component library | Base UI path already in app |
| Icon library | lucide (`Bell`) |
| Fonts | Existing `--font-display` / `--font-sans` |

**Standing rule:** Extend `SiteHeader` / `AccountActions`; do not remount a second header.

---

## Screens in scope

| Route / surface | Purpose | Focal point |
|-----------------|---------|-------------|
| `SiteHeader` signed-in cluster | Bell + unread badge | Bell control (after Create menu, before Account menu) |
| `/notifications` | Inbox list + mark read | Filter tabs then notification rows |

**Out of scope:** Email prefs, watch settings, Done/archive inbox, realtime toast stack.

---

## Visual hierarchy & layout

### Header bell (D-08, D-11, D-14)

| Element | Contract |
|---------|----------|
| Placement | Signed-in `AccountActions` row: `CreateMenu` → **Bell** → `AccountMenu` |
| Control | Icon button ≥ 44×44 hit target; `aria-label` includes unread count when > 0 (e.g. "Notifications (3 unread)") |
| Badge | Numeric unread; hide when 0; use existing muted/primary tokens — no purple glow |
| Behavior | Navigates to `/notifications` (dropdown preview optional; page is mandatory) |
| Visibility | Signed-in only; omit when anonymous / setup |

### `/notifications` page (D-09, D-10, D-12, D-13)

| Element | Contract |
|---------|----------|
| Title | "Notifications" (page H1) |
| Filters | **Unread** \| **All** (segmented control or tablist); default Unread |
| Rows | Reason line + `owner/repo` + `#N` title; relative time secondary |
| Empty | Clear empty copy for Unread vs All |
| Actions | Per-row activate → mark read + navigate; page-level **Mark all as read** when unread > 0 |
| Pagination | Offset "Older" / page controls matching Issues list density — not infinite scroll |
| Auth | Signed-out users redirected to `/login` with returnTo `/notifications` |

---

## Copy

| Surface | Copy |
|---------|------|
| Empty Unread | "You're all caught up." |
| Empty All | "No notifications yet." |
| Mark all | "Mark all as read" |
| Reason examples | "commented on", "assigned you", "requested your review", "mentioned you" — map from `reason` enum in UI |

---

## Accessibility

- Bell and Mark-all are real `<button>` / link controls with visible focus rings
- Filter control is a tablist or radiogroup with keyboard support
- Badge not color-only — count included in accessible name
- Rows are links or buttons with clear accessible names including repo and issue/PR number

---

## UI Considerations

### Resolved

- **Header placement:** Create → Bell → Account (D-08)
- **Filters:** Unread \| All; newest first; offset pagination (D-09)
- **Click behavior:** mark read + navigate (D-10)
- **No email prefs UI** (D-06/D-07)

### Backstop

- **Dropdown preview:** omit unless implementer finds it cheap; page remains source of truth (D-11)

---

## Anti-patterns

- Card-grid dashboard for the inbox (prefer simple list)
- Toast spam for every event
- Purple/glow badge styling
- Mixing React JSX `return (` with Rivet `@{` in the same component
