# Refined Mockups — `/oxpilot`

Provenance: consumes `ideation/rough-mockups/wireframes.md`,
`ideation/rough-mockups/user-flow.md`, `inception/user-stories/stories.md`,
`inception/requirements-analysis/requirements.md`,
`inception/practices-discovery/team-practices.md`; answers Q1–Q4 in
`refined-mockups-questions.md`. Mid-fidelity ASCII + component specs; the
design-system mapping in `design-system-mapping.md` names the Octane
component for every element. Coverage: every interactive surface in
`stories.md` (Q2-A).

Legend: `▾` select, `●` selected radio, `☐/☒` checkbox, `▌` streaming
cursor, `◐` pending/queued, `⟨⟩` inline code chip, `…` truncated.

## M1 — `/oxpilot` index (default state)

```
┌ AppPage ──────────────────────────────────────────────────────────────┐
│ PageHeader: title "OxPilot"          [+ New session]  (Button,primary)│
│   subtitle: "Agentic chat, builder, and debugger — in your forge."     │
│                                                                       │
│ ┌─ Sessions (h2) ────────────────────────────────────────────────────┐ │
│ │ List of SessionRow, newest first                                  │ │
│ │ ▸ ⟨Fix failing actions run⟩  oxidean/oxidean · main ●active  2h   │ │
│ │ ▸ ⟨Add pagination to issues API⟩ oxidean/core · feat/pag  idle 1d │ │
│ │ ▸ ⟨Triage flaky test⟩ oxidean/oxidean · main           errored 3d │ │
│ │                                              [Delete ⟳ hover]      │ │
│ │ Empty (no sessions):                                               │ │
│ │   EmptyState "No sessions yet — describe a task to get started."   │ │
│ └───────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│ ┌─ Computer use (h2) ─────────────── [Connect a system] (Button) ───┐ │
│ │ TargetRow × N:                                                    │ │
│ │  ○ workstation-mbp   local    ●connected·idle   [Manage] [Revoke] │ │
│ │  ○ build-eu-1        remote   ○offline          [Manage] [Revoke] │ │
│ │ Empty (no targets):                                               │ │
│ │   EmptyState "No connected systems. Run the connector on a         │ │
│ │   machine you control to let OxPilot act there — nothing is       │ │
│ │   installed until you run it."                    [Connect…]      │ │
│ └───────────────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────┘
```

- SessionRow: title, `owner/repo`, branch, status badge, relative time;
  hover reveals `[Delete]` (opens M5 confirm). Statuses: `active`,
  `idle`, `errored`, `streaming`.
- TargetRow: name, type (`local`/`remote`), status dot + label
  (`connected·idle` / `connected·busy` / `offline` / `pairing`),
  `[Manage]` → expands inline manage card (rename, capability list,
  disconnect), `[Revoke]` → M5 confirm.
- `/settings/ai` links to the Computer use section (anchor `#targets`).

## M2 — `/oxpilot/$session` workspace (three panes)

```
┌ AppPage ──────────────────────────────────────────────────────────────┐
│ rail │  header: ⟨session title⟩ · owner/repo · branch  [Context ▸]    │
│ ──── │ ────────────────────────────────────────────────────────────  │
│ Sess │  Transcript (main, aria-live=polite, scroll region)            │
│ ions │   ┌ UserMsg — 14:02 ─────────────────────────────────────────┐│
│ ▸Fix │   │ The actions run on main is failing — find out why.       ││
│ ▸Pag │   └──────────────────────────────────────────────────────────┘│
│ ▸Tri │   ┌ AgentMsg · OxPilot — 14:02 ▌streaming ───────────────────┐│
│      │   │ text… ▌                                                 ││
│      │   │ ┌ ToolCallCard ────────────────────────────────────────┐││
│      │   │ │ ⟳ actions.getRun #8814 → log tail (read)            │││
│      │   │ └─────────────────────────────────────────────────────┘││
│      │   │ ┌ ToolCallCard ────────────────────────────────────────┐││
│      │   │ │ ✓ repo.readFile — ci/workflow.yml                   │││
│      │   │ └─────────────────────────────────────────────────────┘││
│      │   │ ┌ ProposedDiffCard ──────────────────────────────────┐││
│      │   │ │ ci/workflow.yml +1 −1   [View diff] [Show patch]   │││
│      │   │ │                        [Create branch + PR]        │││
│      │   │ └─────────────────────────────────────────────────────┘││
│      │   └─────────────────────────────────────────────────────────┘│
│      │   ┌ QueuedMsg ◐ ────────────────────────────────────────────┐│
│      │   │ "also bump the runner image" — queued, sends next       ││
│      │   └──────────────────────────────────────────────────────────┘│
│      │ ────────────────────────────────────────────────────────────  │
│ ●1   │  Composer (labelled form):                                    │
│ target│ [ Ask a follow-up…                                   ] [Send]│
│ [‹]  │                                                              │
├──────┴───────────────────────────────────────────────────────────────┤
│ Context drawer (complementary, opens right, overlays on <1024px):    │
│   Repo: owner/repo · branch · HEAD sha ⟨abc123⟩                      │
│   Proposed diff: file list + unified diff (syntax-marked, read-only) │
│   Computer use: target name · status · granted capabilities          │
└──────────────────────────────────────────────────────────────────────┘
```

- Streaming: reply text appends per SSE frame; `▌` on the active
  message; ToolCallCard `⟳` → `✓`/`✗` as the tool resolves.
- Error terminal frame: inline error notice (danger styling) "The
  provider stopped mid-reply — your messages are saved." + `[Retry]`
  re-requests the stream.
- Budget refusal (AC5.2.2): an over-budget send renders an inline error
  notice in the transcript, "Session budget reached — an operator can
  raise it via the OXIDEAN_OXPILOT_* budget env vars
  (docs/CONFIGURATION.md)."; composer stays usable for later turns once
  the budget resets.
- Reconnect: snapshot frame replaces transcript state; append frames
  resume (no `Last-Event-ID`; exactly-once per message id).
- Composer stays enabled while streaming (queued sends, US1.8).
- `[Create branch + PR]` opens a confirm dialog naming branch + target
  repo; on success a toast + transcript card links the PR (provenance
  line in PR body per AC2.2.4).

## M3 — "New session" dialog

```
┌ Dialog · aria-label "New session" ────────────────────────────────┐
│ Repository   [ oxidean / oxidean-app        ▾ Select ]            │
│ Branch       [ main                       ▾ Select ]              │
│                                                                  │
│ Label "What should OxPilot work on?"                             │
│ Textarea (autofocus):                                            │
│ ┌──────────────────────────────────────────────────────────────┐ │
│ │ e.g. "find why actions run #8814 fails and propose a fix"    │ │
│ └──────────────────────────────────────────────────────────────┘ │
│                                                                  │
│ Fieldset "Computer-use target (optional)":                       │
│   (●) None   ( ) workstation-mbp   ( ) build-eu-1  [Connect new…]│
│                                                                  │
│                                   [Cancel] [Start session]       │
└──────────────────────────────────────────────────────────────────┘
```

- `Start session` disabled until repo + non-empty goal; submit →
  `oxpilot.session_create` → navigate to `/oxpilot/$session`.
- Errors render inline above actions (e.g. `oxpilot.disabled` →
  "OxPilot isn't enabled on this deployment — see
  docs/CONFIGURATION.md").

## M4 — Connect-a-system flow (two-step dialog)

Step 1 — grant:
```
┌ Dialog · aria-label "Connect a system" ─────────────────────────┐
│ Intro copy: "Connect OxPilot to a system you control. It acts   │
│ only while the connector runs, only on capabilities you grant." │
│                                                                  │
│ Name         [ workstation-mbp                    ]  Input       │
│ Type         (●) Local    ( ) Remote                RadioGroup   │
│ Capabilities (fieldset legend "Capabilities"):                   │
│   [x] Run shell commands                                         │
│   [x] Read/write files                                           │
│   [ ] GUI control — not available in this version (disabled)     │
│                                                                  │
│ ☐ I understand the agent will act on this system only with my    │
│   consent, and I can revoke it at any time.          (required)  │
│                                                                  │
│                                   [Cancel]  [Continue]           │
└──────────────────────────────────────────────────────────────────┘
```

Step 2 — pair (replaces step-1 body in the same dialog):
```
┌ Dialog · aria-label "Pair the connector" ───────────────────────┐
│ 1. On that system, run:                                         │
│    ┌───────────────────────────────────────────┐                │
│    │ ox pilot connect --pair oxp_pair_••••     │ [⧉ copy]       │
│    └───────────────────────────────────────────┘                │
│    (pairing code shown once here — it is not stored or logged)  │
│ 2. The connector dials out to this forge — nothing listens on   │
│    your machine.                                                │
│                                                                  │
│ Status: ◐ waiting for connector… (polls; auto-advances)         │
│         ● connected — target registered as workstation-mbp      │
│                                                                  │
│                                            [Done]               │
└──────────────────────────────────────────────────────────────────┘
```

- `[Continue]` disabled until the consent checkbox is checked —
  denial path: `[Cancel]` closes, nothing is created (AC4.2.2).
- The pairing code lives only in this dialog's field: never in the
  transcript, audit detail, or re-fetchable RPC response (AC4.1.4).

## M5 — Confirmations (AlertDialog family)

All destructive/confidential actions use the shared confirm pattern
(`type-to-confirm` is NOT required; single explicit confirm per repo
convention — e.g. `pat-revoke-dialog`):

| Action | Title | Confirm label | Note |
|---|---|---|---|
| Delete session | "Delete this session?" | "Delete session" | body: transcript + tool-call records are permanently removed |
| Revoke target | "Revoke workstation-mbp?" | "Revoke" | body: ends all pending grants, including any in-flight action |
| Create PR | "Create branch + PR?" | "Create PR" | body names branch, repo, commit scope |
| Deny action | (inline, no dialog) | — | denial is itself an action on the consent card |

## M6 — Per-action consent card (in-transcript, Screen-4 refined)

```
┌ ConsentCard · role="group" aria-label "Action request" ──────────┐
│ Action request · ⟨workstation-mbp⟩                              │
│ Run shell: `bun test`                                           │
│   in ~/work/oxidean  · 30s timeout  · output returns to chat    │
│                                                                  │
│   [Allow once] [Allow for this session] [Deny]                   │
└──────────────────────────────────────────────────────────────────┘
```

States: `pending` (buttons enabled) → `allowed`/`denied`/`expired`
(buttons freeze, outcome line renders — "Denied. The agent continues
without this capability."). An `expired` state renders if the grant
window lapses unanswered. If the target goes offline while pending,
the card renders `cancelled — target offline` (AC4.3.4).

## M7 — Disabled / flag-off state

```
┌ AppPage ────────────────────────────────────────────────────────┐
│ PageHeader "OxPilot"                                             │
│ EmptyState "OxPilot isn't enabled on this deployment."           │
│   body: "An operator enables it by setting the OXIDEAN_OXPILOT_* │
│   env vars documented in docs/CONFIGURATION.md."                 │
│   [Open configuration docs] (AppLink)                            │
└──────────────────────────────────────────────────────────────────┘
```

Rendered whenever `oxpilot.status` reports disabled; composer/target
controls are not rendered at all (server still denies — AC5.1.5).

## M8 — Remaining states (per surface)

| Surface | Loading | Empty | Error |
|---|---|---|---|
| Index sessions | `Skeleton` rows | EmptyState (M1) | inline notice + retry |
| Index targets | `Skeleton` rows | EmptyState (M1) | inline notice + retry |
| Transcript | history `Skeleton` on first open | "Describe a task to begin." | terminal error notice + Retry |
| Transcript (budget) | — | — | over-budget inline notice (AC5.2.2) |
| Composer | — | — | input error inline above field |
| Context drawer | `Skeleton` lines | "No proposed diff yet." | — |
| Consent card | `pending` | — | `expired`/`cancelled` |

(On a terminal error, a queued message stays pending and renders a
"still queued — send after retry" line; the user may also cancel it
inline — drain, never auto-drop, per AC1.8.x.)

## Assumptions & Open Questions

- [assumption] Session route shape is `/oxpilot/$session` (param under
  the new top segment); final filename decided against `routeTree` at
  construction.
- [assumption] Connector binary name/UX is placeholder (`ox pilot
  connect`); the real install surface is decided in domain-design.
- [assumption] "Allow for this session" is the deepest shortcut (no
  cross-session grant in v1, per requirements Out of Scope).
