# Interaction Specification — `/oxpilot`

Provenance: `refined-mockups/mockups.md` screens M1–M8; component format
per `aidlc-design-agent/component-spec-template.md`; consumes
`inception/user-stories/stories.md` ACs (each spec cites its AC ids).
"Trigger" rows are user/network events; state names are normative —
construction maps them to component state verbatim.

## Component: SessionRow (index)

| Field | Value |
|---|---|
| Component | `OxpilotSessionRow` |
| Description | One session entry in the index list; navigates to the workspace |
| Category | navigation / display |

### States

| State | Description | Trigger |
|---|---|---|
| default | title, repo, branch, status badge, relative time | render |
| hover/focus | shows `[Delete]` action | mouseover / focus |
| streaming | status badge `streaming` (pulse) | session has open reply |
| errored | status badge `errored` (danger tone) | last reply failed |
| deleting | row inert, spinner in place | confirm in M5 |

### Props / Inputs

| Prop | Type | Required | Default | Description |
|---|---|---|---|---|
| session | `{id,title,repo,branch,status,updatedAt}` | yes | — | row data from `oxpilot.session_list` |
| onDelete | `(id)=>void` | yes | — | opens M5 delete confirm |

### Accessibility

| Requirement | Implementation |
|---|---|
| Role | `listitem` inside `ul`; navigate via `AppLink` on the title |
| Keyboard | Tab to row link; Delete button in tab order on focus-within |
| Status | badge text, never color-only (label `active/idle/errored`) |

## Component: TargetRow (index)

| Field | Value |
|---|---|
| Component | `OxpilotTargetRow` |
| Description | One connected/known target with status + manage/revoke |
| Category | display / input |

### States

| State | Description | Trigger |
|---|---|---|
| connected·idle | ● dot + "connected·idle" | channel open, no running action |
| connected·busy | ● dot + "busy" | action in flight on target |
| offline | ○ + "offline" | channel closed; `[Manage]` stays, actions inert |
| pairing | ◐ + "waiting for connector" | pending pairing window |
| revoking | inert + spinner | M5 confirm accepted |

### Accessibility

| Requirement | Implementation |
|---|---|
| Status | `role="status"` + text; never color-only |
| Buttons | real `Button`s; `aria-label` names the target ("Revoke workstation-mbp") |

## Component: SessionWorkspace (three-pane layout)

| Field | Value |
|---|---|
| Component | `OxpilotSessionPage` |
| Description | rail + transcript + context drawer around the composer |
| Category | layout |

### Responsive behaviour

| Breakpoint | Behaviour |
|---|---|
| <768px | single column; rail hidden behind a `[☰ Sessions]` disclosure; drawer overlays full-width |
| 768–1024px | transcript + drawer-overlay; rail collapsible (default open) |
| >1024px | three panes; rail 240px, transcript fluid, drawer 320px |

### Accessibility

| Requirement | Implementation |
|---|---|
| Landmarks | `nav` rail, `main` transcript, `complementary` drawer, `form` composer |
| Focus | on route open, focus goes to composer textarea |
| Rail collapse | `[‹]/[›]` toggle button with `aria-expanded` |

## Component: Transcript + AgentMessage

| Field | Value |
|---|---|
| Component | `OxpilotTranscript`, `OxpilotMessage` |
| Description | scrollable message region; SSE-streamed assistant text |
| Category | display / feedback |

### States

| State | Description | Trigger |
|---|---|---|
| streaming | text appends; `▌` cursor on open message | SSE append frame |
| complete | cursor removed | SSE done frame |
| error | inline error notice + `[Retry]` | SSE terminal error frame |
| reconnecting | snapshot replaces state, then appends resume | SSE snapshot frame after reconnect |

### Accessibility

| Requirement | Implementation |
|---|---|
| Live region | `aria-live="polite"`, `aria-atomic="false"`; announces message START and END boundaries — not every frame (NFR3 keeps echo under ~100ms so polite announcements stay cheap) |
| Scroll | auto-scrolls only when the user is at the bottom; a "jump to latest" pill appears otherwise |

## Component: ToolCallCard

| Field | Value |
|---|---|
| Component | `OxpilotToolCall` |
| Description | inline record of one agent tool invocation (AC1.3.6) |
| Category | feedback |

### States

| State | Description | Trigger |
|---|---|---|
| running | `⟳` + tool name + summarized inputs | tool call begins |
| done | `✓` + tool name + artifact link | result lands |
| failed | `✗` + tool name + error summary | tool errors |

### Props / Inputs

| Prop | Type | Required | Description |
|---|---|---|---|
| call | `{name, inputsSummary, status, artifactRef?}` | yes | from `oxpilot.tool_call` stream frame / message payload |

## Component: ProposedDiffCard

| Field | Value |
|---|---|
| Component | `OxpilotDiffCard` |
| Description | proposed-change card: file stats + three actions |
| Category | input / display |

Actions: `[View diff]` opens drawer diff; `[Show patch]` toggles raw
patch block (copyable); `[Create branch + PR]` → M5 confirm →
`oxpilot.pr_create` → toast + transcript link (AC2.2.x).

## Component: Composer

| Field | Value |
|---|---|
| Component | `OxpilotComposer` |
| Description | labelled `form`: `Textarea` + Send `Button` |
| Category | input |

### States

| State | Description | Trigger |
|---|---|---|
| default | enabled; placeholder "Ask a follow-up…" | render |
| sending | brief disable until first stream frame | submit |
| queued | submitted while streaming → QueuedMsg row (US1.8) | submit during stream |

(No `disabled` composer state exists — flag-off renders the M7 page in
place of the workspace entirely; a queued row survives a terminal error
and offers send-after-retry or inline cancel.)

### Accessibility

| Requirement | Implementation |
|---|---|
| Label | visible `Label` "Message OxPilot" |
| Submit | `Ctrl/⌘+Enter` hint; button is a real submit |

## Component: ConsentCard (per-action)

| Field | Value |
|---|---|
| Component | `OxpilotConsentCard` |
| Description | in-transcript grant request for one target action (AC4.3.2) |
| Category | input / feedback |

### States

| State | Description | Trigger |
|---|---|---|
| pending | buttons enabled; `role="group"` + `aria-label` | request arrives |
| allowed | frozen + outcome line | Allow once/session |
| denied | frozen + "Denied…" line | Deny |
| expired | frozen + "Request expired" | grant window lapses |
| cancelled | frozen + "target offline" | target disconnects while pending |

### Accessibility

| Requirement | Implementation |
|---|---|
| Announced | inside transcript live region; new pending card is announced once |
| Order | Allow/Deny are tab stops in DOM order; Deny is never hidden |

## Component: ConnectSystemDialog (two-step)

| Field | Value |
|---|---|
| Component | `OxpilotConnectDialog` |
| Description | M4 grant step → pairing step; produces a target + consent |
| Category | input |

| Requirement | Implementation |
|---|---|
| Dialog | `Dialog`, labelled per step, focus-trapped, Esc cancels (pairing-step cancel abandons the pending grant) |
| Consent checkbox | required real `Checkbox`; `[Continue]` disabled until checked (AC4.2.x) |
| Pairing code | read-only `Input` + copy `Button`; rendered once; never re-fetched, never in transcript/audit (AC4.1.4) |
| Capabilities | `Checkbox`es: shell, files; GUI `disabled` + note |
| Type | `RadioGroup` local/remote |
| Poll | pairing step polls `oxpilot.target_get` until connected/timeout; timeout renders an error state with retry |

## Component: ConfirmActionDialog

| Field | Value |
|---|---|
| Component | shared `AlertDialog` per M5 |
| Description | delete session / revoke target / create-PR confirmations |
| Category | feedback |

`AlertDialog`, `aria-label` names the destructive object, focus starts
on Cancel, confirm uses danger styling for delete/revoke.

## Component: DisabledState

`EmptyState` + `AppLink` to `docs/CONFIGURATION.md`; renders when
`oxpilot.status` reports `disabled` (AC5.1.1); no interactive controls.

## Event → RPC/stream map

| UI event | Call |
|---|---|
| index mount | `oxpilot.session_list`, `oxpilot.target_list`, `oxpilot.status` |
| Start session | `oxpilot.session_create` → navigate |
| send message | `POST /api/oxpilot/sessions/{id}/chat` read as a streamed `ReadableStream` (fetch-reader parsing SSE frames — the message body rides the same request; `EventSource` is GET-only so it is not the carrier) |
| reconnect | `GET /api/oxpilot/sessions/{id}/chat/stream`; snapshot frame resets state |
| consent reply | `oxpilot.action_grant` / `oxpilot.action_deny` |
| delete session | `oxpilot.session_delete` (after confirm) |
| revoke target | `oxpilot.target_revoke` (after confirm) |
| pair | `oxpilot.target_pair_begin` → poll `target_get` |
| Create PR | `oxpilot.pr_create` (after confirm) |

## Assumptions & Open Questions

- [assumption] Procedure names above are placeholders — the canonical
  `oxpilot.*` surface is fixed in contract-design; state semantics here
  are the contract, not the literal strings.
