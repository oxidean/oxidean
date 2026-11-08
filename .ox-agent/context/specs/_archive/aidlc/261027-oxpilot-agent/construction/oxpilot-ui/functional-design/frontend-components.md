# Frontend Components — oxpilot-ui (U8)

Component hierarchy, props/state, interaction flows, validation
rules, and API integration points for `OxpilotWeb`. Sources:
`functional-spec.md` (WF/QA pins), `inception/refined-mockups/
interaction-spec.md` (component specs), `design-system-mapping.md`
(primitive map + `components/oxpilot/` dir), `accessibility-
checklist.md` (a11y column), the shipped `apps/web` files (what
already exists). Component files are kebab-case under
`apps/web/src/components/oxpilot/`; all are Octane `.tsrx`
(`function X() @{ … }`, `@if`/`@for`/`@switch`, `onInput`) — never
React JSX patterns. `*` = new file; no `*` = shipped, extended.

## Hierarchy

```text
routes/oxpilot.tsrx                    (index route — auth+status gate)
└─ components/oxpilot/
   ├─ flag-off.tsrx          (M7 DisabledState)
   ├─ new-session-dialog.tsrx (M3)
   ├─ session-row.tsrx       (index rows)
   ├─ target-row.tsrx        (index rows + manage expander)
   ├─ connect-dialog.tsrx    (M4 two-step)
   └─ confirm-dialog.tsrx    (M5 shared AlertDialog wrapper)
routes/oxpilot.$session.tsrx           (workspace route — seed+gate)
└─ components/oxpilot/
   ├─ sessions-rail.tsrx     (nav pane + target chips)
   ├─ transcript.tsrx        (main live region + message/cards)
   │  ├─ tool-call-card.tsrx
   │  ├─ diff-card.tsrx
   │  ├─ consent-card.tsrx
   │  └─ queued-row.tsrx
   ├─ composer.tsrx          (form + send/queue branch)
   ├─ context-drawer.tsrx    (complementary pane + diff view)
   └─ confirm-dialog.tsrx    (shared, from index import)
lib: oxpilot-stream.ts (frame union+parse+attach), oxpilot-chat.tsrx
     (useChat binding), oxpilot-queries.ts (queryOptions+mutations)
```

## Inventory

| Component | File (`components/oxpilot/` unless noted) | Props / state | Owner WF | A11y notes | Coverage row |
|---|---|---|---|---|---|
| `OxpilotPage` (index) | `routes/oxpilot.tsrx` | queries: `auth.me`, `status`, `session_list`, `target_list`; local `useState` for open dialog | WF1 | one `h1` "OxPilot"; skeletons on pending; redirect on unauth | route-coverage `oxpilot.tsrx` (happy-dom, shipped) |
| `OxpilotFlagOff` | `flag-off.tsrx`* (lifted from route file) | `props.reason` → `flagOffReason` copy | WF1 | `EmptyState`; zero interactive controls; doc link named | none (static) |
| `OxpilotSessionRow` | `session-row.tsrx`* | `props.session{title,repo_owner,repo_name,branch,status,last_activity_at}`, `onDelete`; state: `deleting` | WF1, WF9 | `listitem`-in-`List`; title `AppLink`; Delete in tab order via `focus-within`; status badge text not color | happy-dom subject `oxpilot-session-row` inside the route row (button-only — undiscovered by the browser-coverage gate) |
| `OxpilotTargetRow` | `target-row.tsrx`* | `props.target{id,label,status,lifecycle,granted_caps}`, `onDisconnect`, `onRevoke`, `onManage`, `onConnect`; state: `expanded`, `busy`(derived), `pendingGrants` | WF1, WF9, WF11 | `role="status"` text label (`needs consent`/`connected·idle`/`offline`/`revoked` — `pending_consent` maps to "needs consent"; no `pairing` row state); `aria-label`d buttons name the target; `pending_consent` `[Connect]` opens the Manage expander (`grantList{target_id}` + `[Allow]`/`[Deny]` → `grantRespond`) | happy-dom subject `oxpilot-target-row` inside the route row (button-only — undiscovered) |
| `NewSessionDialog` | `new-session-dialog.tsrx`* | `props.open,onClose,onCreated`; fields: `repoKey`, `branch`, `goal`, `targetId`; mutation `sessionCreate` | WF2 | `Dialog` labelled, focus-trapped, Esc; repo `Select` `aria-label`; goal `Textarea` autofocus; errors inline above actions | browser row `oxpilot-new-session` (Select+RadioGroup in Dialog); repo-select row kept (shipped) |
| `ConnectDialog` | `connect-dialog.tsrx`* | `props.open,onClose`; step state `1|2`, `name`(→`target_name`), `caps[]`, `understood`; step-2 `pairing{pairing_id,code,expires_at}`, poll timer (M4's `local/remote` radio drops — no wire field; contract-deltas deviation) | WF8 | per-step `aria-label` ("Connect a system"→"Pair the connector"); required real `Checkbox`; code in read-only `CopyCommand`, never in live region; Esc on step 2 abandons | browser row `oxpilot-connect` (Checkbox/Input/step swap) |
| `OxpilotConfirmDialog` | `confirm-dialog.tsrx`* | `props{open,title,body,confirmLabel,danger,onConfirm,onClose}` — generic M5 shell for delete/revoke/disconnect/PR | WF9 | `AlertDialog`; `aria-label` names the object; focus starts on Cancel | none new (AlertDialog primitive already covered via FORGE_BRANCHES row) |
| `OxpilotSessionPage` | `routes/oxpilot.$session.tsrx` | `sessionId` from `matchPath`; `seed{version,messages,streamingIds,activeTurn,pendingQueue,toolCalls,cards,session}`, `seedError`; `reseed()` runs `fetchOxpilotSnapshot` + `sessionGet` + `grantList{session_id}` + `diffList{session_id}` (mount-time card recovery — pending grants/proposals outlive their turn's frames) | WF3, WF5 | focus → composer on open; M7 variant when disabled | route-coverage `oxpilot.$session.tsrx` (happy-dom, shipped) |
| `OxpilotChatSurface` | inside `routes/oxpilot.$session.tsrx` (composition root) | `props{sessionId,seed,onReseed}` (`seed` supplies `initialMessages`/`streamingIds`/`activeTurn`/`pendingQueue`/`toolCalls`/`cards`/`session`); `chat=useOxpilotChat`, `liveTail`, `dedupe` | WF3–WF5 | landmark trio; keyed remount `s-${seed.version}` | covered by the route row |
| `SessionsRail` | `sessions-rail.tsrx`* | `props{sessionId,sessions,targets,collapsed,onToggle}` | WF11 | `nav` landmark; collapse `Button` `aria-expanded`; `[☰ Sessions]` disclosure <768px | happy-dom subject `oxpilot-rail-toggle` inside the route row (button-only — undiscovered) |
| `OxpilotTranscript` | `transcript.tsrx`* | `props{messages,streamingIds,liveTail,tails,busy,cards,queued}`; local `atBottom` scroll state | WF4–WF7, WF10 | `main` landmark `aria-live="polite"` `aria-atomic="false"`; boundary-only announcements; `▌`/`typing…` text not color; autoscroll only while scrolled to bottom — "jump to latest" pill otherwise ([ix] scroll contract) | covered by route row |
| `OxpilotToolCall` | `tool-call-card.tsrx`* | `props.call{tool,args_summary,output_summary,status}` (`call_id`-keyed updates) | WF6 | status conveyed as text ("running"/"done"/"failed") not icon-only; args as `<code>` | none (derived render) |
| `OxpilotDiffCard` | `diff-card.tsrx`* | `props.diff{title,base,files,status,pr_ref}`, `onView`, `onApply`; state: `showPatch` | WF10 | three real `Button`s; patch `<pre>` toggle copyable | happy-dom subject `oxpilot-diff-card` inside the route row (button-only — undiscovered) |
| `OxpilotConsentCard` | `consent-card.tsrx`* | `props.grant{id,kind,capability,args_summary,state,expires_at},targetLabel`, mutation `grantRespond`; frozen on respond — outcome enum `allowed`/`denied`/`expired`/`cancelled`/`revoked` ("revoked — target access ended") | WF7 | `role="group"` `aria-label="Action request"`; Allow once/session/Deny DOM order; Deny always reachable; announced once | happy-dom subject `oxpilot-consent` inside the route row (button-only — undiscovered) |
| `OxpilotQueuedRow` | `queued-row.tsrx`* | `props.queued{id,content}`, `stillQueued` | WF4, WF6 | `◐` + "queued"/"still queued" text; announce once on queue + once on send | none (derived render) |
| `OxpilotComposer` | `composer.tsrx`* | `props{busy,turnLive,onSend,onQueue}`; `draft` local; error banner slot | WF4 | `form` landmark; visible `Label` "Message OxPilot" + `Ctrl/⌘+Enter` `aria-describedby`; pinned keydown handler — Enter inserts a newline, `Ctrl/⌘+Enter` submits; send `type="submit"` real `Button` | covered by route row (send path covered by existing tests) |
| `ContextDrawer` | `context-drawer.tsrx`* | `props{open,onClose,session,diff,target}`; diff pane uses `parseUnifiedDiffLines`/`highlightDiffLines` | WF10, WF11 | `complementary` landmark; Esc closes + focus back to toggle; overlay <1024px | happy-dom subject `oxpilot-drawer` inside the route row (button-only — undiscovered) |
| `CopyCommand` reuse | `components/settings/copy-command.tsrx` (existing import) | `props{command,ariaLabel}` | WF8 | labelled copy button | none (existing covered surface) |
| `DisabledState`/notices | shared `EmptyState`, `AuthErrorBanner`, `toast.ts` | — | WF1, WF4, WF12 | textual notices, never color-only | — |

## Interaction flows

- **Index → new session**: `[+ New session]` → `NewSessionDialog`
  open → repo select (required) → optional branch/goal/target →
  `session_create` → navigate `/oxpilot/{id}` (WF2).
- **Index → connect**: `[Connect a system]` → step 1 consent →
  `pairing_create` → step 2 copy + poll `target_list`(2s,
  `pairing_id`) → `grant_list` → `grant_respond` → connected →
  `[Done]` → invalidate (WF8).
- **Index → re-offer consent**: a `pending_consent` row's
  `[Connect]` (or `[Manage]`) opens the Manage expander →
  `grant_list{target_id}` lists pending connect grants →
  `[Allow]`/`[Deny]` → `grant_respond` → invalidate (WF1/WF8.4,
  BR7.4).
- **Index → destructive**: row `[Delete]`/`[Revoke]`/`[Disconnect]`
  → `OxpilotConfirmDialog` (focus Cancel) → proc → invalidate +
  toast (WF9).
- **Workspace send**: composer → live-turn branch (`message_queue`
  → `QueuedRow`) vs idle (`POST /chat` fetch-reader → AG-UI chunks
  → streamed row) (WF4); attach stream merges `tool_call`/`card`/`append`
  frames → card dispatch (WF5–WF6).
- **Consent**: `card{consent}` → `grant_get` → pending buttons →
  `grant_respond{scope}` → freeze + outcome line (WF7).
- **Diff**: `card{diff}` → `diff_get` → `[View diff]` drawer /
  `[Show patch]` `<pre>` / `[Create branch + PR]` → confirm →
  `diff_apply` → toast + `pr_ref` `AppLink` (WF10).

## Form validation rules

- New session: repo required (client check — `!owner||!name` ⇒
  "Pick a repository"); `branch`/`goal` omitted when blank;
  `target_id` only when a connected pick (client refuses
  non-`connected` lifecycle); submit disabled while pending
  (shipped guard + M3's goal-required note is advisory — goal stays
  optional per the shipped `session_create` contract).
- Connect step 1: `name` non-empty; ≥1 cap checked (else Continue
  disabled — a target with no caps is useless); understanding
  `Checkbox` required (AC4.2.x); GUI box is `disabled` +
  explanatory note (FR5.3).
- Composer: `draft.trim()` empty ⇒ send `Button` disabled (shipped);
  `content` trims server-side too.
- Confirms: single explicit confirm (no type-to-confirm — M5).
- Grant respond: only while `state==="pending"`; buttons inert
  post-respond; server still enforces `grant_not_pending`.

## API integration points

| UI surface | Calls (`apiClient.oxpilot.*` unless noted) |
|---|---|
| Index mount | `status` (gate-exempt), `sessionList`, `targetList`, `repo.listMine` |
| New session | `sessionCreate{repo_owner,repo_name,branch?,goal?,target_id?}`; branch options via `apiClient.repo.refs{owner,name}` |
| Workspace seed/attach | `GET …/chat/stream` (snapshot; `fetchOxpilotSnapshot` + `EventSource`); mount/recovery reads `sessionGet` + `grantList{session_id}` + `diffList{session_id}`; `es.onerror` → one `sessionGet` probe → fatal close or native reconnect |
| Send | `POST …/chat` fetch-reader (`useOxpilotChat` fetcher) or `messageQueue{session_id,content,client_msg_id}` |
| Cards | `tool_call` frame rows; `grantGet`, `diffGet`, `queueList` on `card` frames; `diffList` for the drawer |
| Consent | `grantRespond{grant_id,allow,scope?}` |
| Connect | `pairingCreate`, `targetList` (poll matched on `pairing_id`), `grantList`, `grantRespond` |
| Manage | `grantList{target_id}` + `grantRespond` (pending connect grants — `[Connect]` re-offer), `targetDisconnect`, `targetRevoke`, `sessionDelete` (confirms) |
| Materialize | `diffApply{proposal_id}` → `pr_ref` link |
| Stop (optional) | `turnCancel{session_id}` on an own live turn |
| Auth | `auth.me` via `authSessionQueryOptions()` + `usePageRedirect` |

## State ownership ([ds-map] pinned)

TanStack Query owns `status`, `sessionList`, `sessionGet`,
`targetList`,
`grantList`/`grantGet`, `queueList`, `diffList`/`diffGet`,
`usageGet` — all `retry:false`, `staleTime` 10–15s, invalidated via
`invalidateOxpilotQueries` on terminal frames + every mutation.
Local `useState` owns dialog open/steps, rail collapse, drawer
open, `liveTail`, `draft`, `showPatch`, manage-expander — never
global stores ([domain]'s "local UI state only"). The stream is a
dedicated channel into local state (snapshot-replace + append,
dedupe `(message_id,seq)`) — never `refetchInterval` polling.
