# Rough Wireframes — `/oxpilot` Agentic Chat / Builder / Debugger

Provenance: consumes `intent-capture/intent-statement.md`,
`scope-definition/scope-document.md`, `scope-definition/intent-backlog.md`;
answers Q1–Q6 in `rough-mockups-questions.md` (conductor-decided under
standing instruction 2026-10-27; ratify at PR review). Product-manager lens
integrated inline. ASCII per stage standards; Oxidean idiom = `AppPage` +
`PageHeader` + Tailwind tokens.

## Screen 1 — `/oxpilot` index (sessions + targets)

```
┌──────────────────────────────────────────────────────────────────────┐
│ AppHeader (global nav — unchanged)                                    │
├──────────────────────────────────────────────────────────────────────┤
│  OxPilot                                          [+ New session]     │
│  Agentic chat, builder, and debugger — inside your forge.            │
│                                                                      │
│  ┌─ Sessions ──────────────────────────────────────────────┐         │
│  │ ▸ Fix failing actions run on oxidean/oxidean    repo▸CI  │ 2h ago  │
│  │ ▸ Add pagination to issues API                  builder  │ 1d ago  │
│  │ ▸ Triage #142 flaky test                        debug    │ 3d ago  │
│  │                                                         │         │
│  │              … empty state when none:                    │         │
│  │   "No sessions yet — describe a task to get started."    │         │
│  └─────────────────────────────────────────────────────────┘         │
│                                                                      │
│  ┌─ Computer use ───────────────────────────── [Connect a system] ┐  │
│  │ ○ workstation-mbp   local   connected ●idle   [Manage] [Revoke] │  │
│  │ ○ build-eu-1        remote  offline           [Manage] [Revoke] │  │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                      │
│  (The `/settings/ai` page links HERE — "Manage connected systems      │
│   → /oxpilot" — `/settings/ai` stays the external-agent surface.)     │
└──────────────────────────────────────────────────────────────────────┘
```

A11y note: h1 "OxPilot"; landmarks `header`/`main`; h2 "Sessions",
"Computer use"; keyboard entry = the `+ New session` button (first
interactive stop after nav), rows are list items with buttons.

## Screen 2 — `/oxpilot/<session>` workspace

```
┌──────────────────────────────────────────────────────────────────────┐
│ AppHeader                                                            │
├──────────┬───────────────────────────────────────────┬───────────────┤
│ Sessions │  Fix failing actions run · oxidean/oxidean │  Context ▸    │
│ (rail)   │  ●connected: workstation-mbp               │  (drawer,     │
│          ├───────────────────────────────────────────┤   toggled)    │
│ ▸ Fix CI │  You — 14:02                               │               │
│  2h ago  │  The actions run on main is failing —      │  Repo         │
│          │  find out why and fix it.                  │  oxidean/     │
│ ▸ Add    │                                            │  oxidean      │
│  pagin.  │  OxPilot — 14:02 ▌streaming                │               │
│          │  The failure is in `actions-e2e`:          │  Proposed     │
│          │  ┌─ tool: read run logs ────────────────┐  │  diff         │
│          │  │ actions.getRun #8814 → log tail …    │  │  (collapsed   │
│          │  └──────────────────────────────────────┘  │   by default) │
│          │  ┌─ tool: repo.readFile ────────────────┐  │               │
│          │  │ ci/workflow.yml                      │  │  Computer     │
│          │  └──────────────────────────────────────┘  │  use          │
│          │  Found it: the job pins `actions/setup-    │  workstation- │
│          │  node@v5` but the runner image carries     │  mbp ●idle    │
│          │  an older Node. Proposed fix:              │               │
│          │  ┌─ Proposed change ────────────────────┐  │               │
│          │  │ bump setup-node → v6  [View diff]    │  │               │
│          │  │ [Create branch + PR]  [Show patch]   │  │               │
│          │  └──────────────────────────────────────┘  │               │
│          │                                            │               │
│          │  ┌─ composer ────────────────────────────┐ │               │
│          │  │ Ask a follow-up…              [Send]  │ │               │
│          │  └───────────────────────────────────────┘ │               │
│ ──────── │                                            │               │
│ ●1 target│                                            │               │
│  online  │                                            │               │
│  [collapse‹]                                           │               │
└──────────┴───────────────────────────────────────────┴───────────────┘
```

A11y note: h1 = session title; landmarks `nav` (rail), `main` (transcript),
`complementary` (context drawer), `footer` region replaced by a labelled
`form` composer; keyboard entry = composer textarea; `aria-live="polite"`
announces message boundaries; card buttons are real buttons in tab order;
the rail footer shows connection status and the collapse control.

## Screen 2b — "New session" surface (from `[+ New session]`)

```
                 ┌── New session ───────────────────────────────┐
                 │  Repository   [ oxidean / oxidean-app    ▾ ] │
                 │  Branch       [ main                     ▾ ] │
                 │                                              │
                 │  What should OxPilot work on?                │
                 │  ┌────────────────────────────────────────┐  │
                 │  │ e.g. "find why actions run #8814       │  │
                 │  │ fails and propose a fix"               │  │
                 │  └────────────────────────────────────────┘  │
                 │                                              │
                 │  Computer-use target (optional):             │
                 │    (●) none   ( ) workstation-mbp            │
                 │                                              │
                 │              [Cancel]  [Start session]       │
                 └──────────────────────────────────────────────┘
```

A11y note: dialog labelled "New session", focus-trapped, Escape cancels;
repo/branch selects are labelled comboboxes; the goal field is the natural
focus target on open.

## Screen 3 — Computer-use consent + connect dialog

```
                 ┌── Connect a system ─────────────────────────┐
                 │ You're connecting OxPilot to a system YOU    │
                 │ control. The agent can act on it only while  │
                 │ the connector is running and only for the    │
                 │ capabilities you grant.                      │
                 │                                              │
                 │  Name        [ workstation-mbp           ]   │
                 │  Type        (●) local   ( ) remote           │
                 │  Capabilities [x] shell  [x] files  [ ] GUI  │
                 │                                              │
                 │  1. Run the connector on that system:        │
                 │     ┌──────────────────────────────────┐     │
                 │     │ ox pilot connect --token oxp_…   │ [⧉] │
                 │     └──────────────────────────────────┘     │
                 │  2. It dials out to this forge — nothing    │
                 │     listens on your machine.                 │
                 │                                              │
                 │   ☐ I understand the agent will act on this │
                 │     system under my consent                  │
                 │                                              │
                 │              [Cancel]  [Connect]             │
                 └──────────────────────────────────────────────┘
```

A11y note: dialog labelled "Connect a system", focus-trapped, Escape
cancels; h2 inside `main`; checkbox is a real input; the token is in a
read-only field with a copy button — never logged to the transcript.

## Screen 4 — Per-action consent prompt (in-transcript)

```
│  OxPilot — 14:11                                                  │
│  ┌─ Action request · workstation-mbp ──────────────────────┐      │
│  │ Run `bun test` in ~/work/oxidean (shell)                │      │
│  │ Working dir snapshot attached · 30s timeout             │      │
│  │ [Allow once]  [Allow this session]  [Deny]              │      │
│  └─────────────────────────────────────────────────────────┘      │
```

A11y note: card inside transcript; action buttons in tab order; denial is
always available and narrated in the transcript.

## Screen 5 — Unconfigured / flag-off state (operator disabled)

```
│  OxPilot — not enabled                                          │
│  This deployment has no agent provider configured. An operator   │
│  can enable it by setting the OXIDEAN_* env vars documented in   │
│  docs/CONFIGURATION.md.                                          │
```

A11y note: h1 state page; static text; link in a normal anchor.

## Assumptions & Open Questions

- [assumption] `/oxpilot/<session>` uses a route param file naming consistent
  with existing `$owner.$repo.*` conventions; exact route naming (index vs
  `.$session`) is decided at code-generation against the route manifest.
- [assumption] The connector entry point is assumed to be `ox` CLI-shaped
  (`ox pilot connect`) — the actual install surface is decided in
  domain-design (could be a script or a small binary); the wireframe's copy
  is a placeholder for whatever ships.
