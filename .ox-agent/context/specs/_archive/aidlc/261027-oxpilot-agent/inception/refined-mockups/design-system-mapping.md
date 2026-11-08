# Design-System Mapping — `/oxpilot`

Provenance: `refined-mockups/mockups.md` + `interaction-spec.md`;
`inception/practices-discovery/team-practices.md` (Octane `.tsrx`,
Tailwind tokens, TanStack Query, no React JSX). Every spec component maps
to an existing `apps/web` primitive — no new base component is needed;
feature components live under `apps/web/src/components/oxpilot/`.

## Route & shell

| Element | Existing primitive | Notes |
|---|---|---|
| Page shell | `AppPage` (`components/app-page`) | auth gate via `usePageRedirect` + `useQuery(authSessionQueryOptions())` — the `settings/ai.tsrx` precedent |
| Header | `PageHeader` | title "OxPilot"; actions slot holds `[+ New session]` |
| Route file | `pages/oxpilot.astro` + `routes/oxpilot.tsrx` (`.$session` child) | plus `shells.rs`, `RESERVED_TOP_SEGMENTS`, `routeTree`, coverage manifest rows (AC1.1.3) |
| Links | `AppLink` | `/settings/ai` ↔ `/oxpilot#targets` |

## Index (M1)

| Element | Primitive | Notes |
|---|---|---|
| Sessions list | `ul` + `OxpilotSessionRow` | `List` primitive for rows |
| Status badge | `Badge` | text labels; tone map: active=default, streaming=accent, errored=destructive, idle=secondary |
| Empty states | `EmptyState` | sessions + targets variants |
| New session | `Button` (primary) → `OxpilotConnectDialog`-style `Dialog` (M3) | |
| `[Manage]`/`[Revoke]` | `Button` (ghost/outline) | Revoke → `AlertDialog` (M5), modelled on `pat-revoke-dialog.tsrx` |
| Delete session | `AlertDialog` | modelled on `issue-delete-dialog.tsrx` |
| Connect a system | `Button` → `OxpilotConnectDialog` | two-step `Dialog` |

## Workspace (M2)

| Element | Primitive | Notes |
|---|---|---|
| Rail | `aside` + Tailwind `w-60`, collapse `Button` (`aria-expanded`) | sessions + target status footer |
| Transcript | `section role="main"`, `aria-live="polite"` | message list = `div`s, not `ul` (mixed card types) |
| Message bubble | Tailwind card (`rounded-lg border`) | user vs agent alignment |
| ToolCallCard | card + `Badge` status (`⟳✓✗` as an `animate-spin` glyph/icon + label text — no `Spinner` import exists; build it from Tailwind utilities) | summarized inputs as inline `code` |
| ProposedDiffCard | card + `Button`s | `View diff` opens drawer; `Show patch` toggles `<pre>` |
| ConsentCard | card + `role="group"` + three `Button`s | outcome line replaces buttons |
| QueuedMsg | ghosted row + `◐` + "queued" text | US1.8 |
| Composer | `form` + `Textarea` + `Button` submit | `Ctrl/⌘+Enter` |
| Context drawer | `aside role="complementary"` | overlay <1024px; `PageHeader`-less scroll region |
| Inline code | `code` chip | tool names, branches, paths |

## Dialogs (M3–M5)

| Element | Primitive | Notes |
|---|---|---|
| New-session dialog | `Dialog` + `Select` (repo, branch) + `Textarea` (goal) + `RadioGroup` (target) | high-risk controls → `browser-coverage` row (mandated practice) |
| Connect dialog | `Dialog` + `Input` + `RadioGroup` + `Checkbox`es + `CopyCommand` (pairing code) | copy field = `CopyCommand` precedent from `settings/` |
| Confirms | `AlertDialog` | delete/revoke/PR; focus starts on Cancel |

## Feedback

| Element | Primitive | Notes |
|---|---|---|
| Toasts | `Toaster` | PR created, target connected/revoked, errors |
| Loading | `Skeleton` | session rows, transcript first-open, drawer |
| Inline errors | notice block (`rounded-md border-destructive` + text) + retry `Button` | terminal SSE error, list fetch failure |
| Disabled state | `EmptyState` + `AppLink` | M7 flag-off |

## State management

| Data | Owner |
|---|---|
| sessions/targets/status lists | TanStack Query — `oxpilot-queries.ts` `queryOptions` factories |
| session transcript history | TanStack Query (initial fetch) |
| live stream | dedicated SSE channel (`EventSource`/fetch-reader) into local component state — snapshot-replace + append contract, no `refetchInterval` polling (affirmed: job-log SSE pattern is the precedent) |
| consent cards | stream frames + `oxpilot.action_*` mutations |
| local UI (drawer open, rail collapse, patch toggle) | `useState`/zustand only |

## Coverage obligations (per affirmed practice)

- `route-coverage` row for `/oxpilot` (+ `$session` variant) — real
  happy-dom mount.
- `browser-coverage` row(s) — `Select`, `RadioGroup`, `Checkbox`,
  `Dialog` are listed high-risk controls.
- `check-route-sync` triangle: `shells.rs` + Astro page +
  `RESERVED_TOP_SEGMENTS` + generated `routeTree`.

## Assumptions & Open Questions

- [assumption] `components/oxpilot/` is the feature dir (kebab-case
  files, PascalCase components per team.md).
- [assumption] SSE client uses the same auth/session cookie; no extra
  token plumbing needed — confirmed at contract-design.
