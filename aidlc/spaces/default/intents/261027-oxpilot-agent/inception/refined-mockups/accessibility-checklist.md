# Accessibility Checklist — `/oxpilot`

Provenance: `refined-mockups/mockups.md`, `refined-mockups/interaction-spec.md`;
target: WCAG 2.1 AA (Q4-A), matching NFR7 and the rough-wireframe notes.
Each item is checkable in the happy-dom/browser coverage mandated for the
route.

## Landmarks & structure

- [ ] Page has exactly one `h1` ("OxPilot" on index; session title in workspace)
- [ ] `header` (PageHeader), `nav` (sessions rail), `main` (transcript),
  `complementary` (context drawer), `form` (composer) landmarks present
- [ ] Heading order is sequential (h1 → h2 "Sessions"/"Computer use"; no
  skipped levels)
- [ ] Keyboard entry point on index = `[+ New session]`; on workspace =
  composer textarea

## Live regions & streaming

- [ ] Transcript uses `aria-live="polite"` + `aria-atomic="false"`
- [ ] Announcements fire on message boundaries (start/end), not per
  stream frame — screen readers hear complete thoughts, not fragments
- [ ] ToolCallCard status changes are conveyed by text labels
  ("running"/"done"/"failed"), not icon-only
- [ ] Queued message (US1.8) announces once on queue, once on send

## Dialogs & consent

- [ ] Every `Dialog`/`AlertDialog` has a visible label +
  `aria-label`/`aria-labelledby`, focus trap, Esc dismissal
- [ ] Connect-a-system: step change re-announces the dialog label
  ("Pair the connector")
- [ ] Consent checkbox is a real labelled `input` (not a click div)
- [ ] ConsentCard: `role="group"`, `aria-label="Action request"`,
  buttons in DOM order, Deny always reachable by keyboard
- [ ] Confirms focus Cancel on open (safe default)
- [ ] Pairing code sits in a read-only `Input` + labelled copy button;
  it is never announced into the transcript live region

## Controls & forms

- [ ] All actions are real `button`/`a`/`input` elements — no
  keyboard-inaccessible divs
- [ ] Selects (repo/branch), radios (target type), checkboxes
  (capabilities, consent) carry visible `Label`/`legend`
- [ ] Composer: visible label + `Ctrl/⌘+Enter` hint announced via
  `aria-describedby`
- [ ] Rail collapse toggle uses `aria-expanded`

## Status & color

- [ ] Target/session status never color-only — always a text label
  ("connected·idle", "offline", "pairing", "errored")
- [ ] Text contrast ≥ 4.5:1; status dots/badges ≥ 3:1 against background
- [ ] Error states (terminal stream error, fetch failure) are textual
  notices, not just red styling

## Focus & navigation

- [ ] Focus visible on every interactive element (Tailwind focus ring)
- [ ] Row-level actions ([Delete]/[Revoke]) are reachable without hover
  (focus-within reveals; keyboard shows them)
- [ ] Drawer/rail open state survives `Esc` (returns focus to the toggle)
- [ ] Route change to a session moves focus to the composer (or an
  announced landmark), not to the middle of the transcript

## Assumptions & Open Questions

- [assumption] Streaming announcements at message boundaries are the
  AA-compliant middle ground between per-frame chatter and silence; if
  usability testing shows the tail is lost, a "reply finished" summary
  announcement is the fallback.
