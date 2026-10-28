# Functional Design Questions — oxpilot-ui

Unit: `oxpilot-ui` (U8, kind `ui`) — owns `OxpilotWeb` beyond the U1
shell: full index (sessions + targets sections, empty states),
complete workspace (rail + transcript + drawer), all cards
(tool-call, diff, consent, queued), dialogs (new session,
connect/pair two-step, confirms), SSE client (POST fetch-reader +
GET resume, snapshot-replace + append), flag-off state, WCAG AA
items, route/browser coverage rows for every interactive surface.
depends_on: U3 (real stream contract), U5 (diff card), U6 (consent
surfaces). The U1 page skeleton counts as its foundation.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U8 — the unit
  line above; §U1 — "minimal `/oxpilot` page + `$session` workspace
  shell" the skeleton already shipped.
- [story-map] `unit-of-work-story-map.md` — U8 rows: US1.1 → U8 (U2)
  "index page over session list proc"; US1.2 → U8 (U2, U6) "dialog +
  bind; optional target pick reads U6 state"; US1.7 → U8 (U6)
  "target manager surfaces"; US2.3 → U8 (U5) "plain patch view";
  US4.5 → U8 (U6) "status chips rail + index". Contributing rows:
  US1.3 (U3; rail/transcript/drawer + cards), US1.4 (U3; visible
  error), US1.6 (U3; snapshot-resume client half), US1.8 (U2;
  pending chip), US2.1 (U5; diff card), US4.1/US4.2 (U6; pairing +
  connect-grant consent), US4.3 (U7; consent card + visible denial),
  US4.4 (U6; revoke confirm).
- [contract] `inception/contract-design/contract-summary.md` — C1
  proc families the UI calls (`oxpilot.status`, `session_*`,
  `message_queue`, `turn_cancel`, `diff_*`, `target_*` incl. the
  `target_get` + `lifecycle`/`pairing_id` amendment, `grant_*` incl.
  `grant_respond.scope`, `usage_get`, `grant_get`, `queue_list`);
  C2 SSE frame vocabulary (`snapshot`/`append`/`tool`/`card`/
  `terminal`/`ping`) + the non-2xx error set + the 409 `turn_active`
  rule ("the client calls `oxpilot.message_queue` instead;
  auto-enqueue and silent accept are both forbidden") + the
  card-ref resolution table (consent→`grant_get`, queued→
  `queue_list`, diff→`diff_get`).
- [req] `inception/requirements-analysis/requirements.md` — FR1.1–
  FR1.7 (index, new-session surface, target manager, flag-off, no
  role gate, delete), FR2.1/FR2.2/FR2.4/FR2.5 (three-pane workspace,
  SSE, persistence, aria-live + labelled composer), FR3.1/FR3.2/
  FR3.3 (diff view, explicit materialization, never auto), FR5.2/
  FR5.6 (connect consent, revoke), NFR3 (SSE reconnect without
  duplication), NFR7 (WCAG AA), NFR8 (explanatory disabled state),
  NFR9 (route + browser coverage rows, happy-dom client test).
- [domain] `inception/domain-design/components.md` §OxpilotWeb —
  "Renders session/target lists, streamed transcript, tool-call and
  diff cards, consent cards, and all dialogs… Owns SSE client
  lifecycle (POST fetch-reader send stream, GET resume stream,
  snapshot-replace + append); Local UI state only (drawer, rail
  collapse, queued-message row)… the UI enforces nothing itself."
- [ix] `inception/refined-mockups/interaction-spec.md` — per-
  component states/props/a11y: `OxpilotSessionRow`,
  `OxpilotTargetRow`, `OxpilotSessionPage` (three-pane responsive),
  `OxpilotTranscript`/`OxpilotMessage`, `OxpilotToolCall`,
  `OxpilotDiffCard`, `OxpilotComposer`, `OxpilotConsentCard`,
  `OxpilotConnectDialog`, `ConfirmActionDialog`, `DisabledState`;
  the event→RPC map (placeholder proc names — canonical names are
  C1's).
- [mock] `inception/refined-mockups/mockups.md` — M1 index, M2
  workspace, M3 new-session dialog, M4 connect two-step, M5
  confirms, M6 consent card, M7 flag-off, M8 remaining states.
- [a11y] `inception/refined-mockups/accessibility-checklist.md` —
  WCAG 2.1 AA pins: landmarks, boundary-only live announcements,
  focus rules, dialog labels/traps, status never color-only.
- [ds-map] `inception/refined-mockups/design-system-mapping.md` —
  every element → existing primitive; `components/oxpilot/` feature
  dir; state-ownership table; coverage obligations.
- [u3-fd] `construction/agent-runtime/functional-design/
  functional-spec.md` — the real stream the client consumes:
  `tool` frames carry `call_id` + `call_index` (additive C2 fields),
  `card{queued}` pushed on `message_queue` commit, terminal
  `{done|error+code|aborted}`, chained turns surface via
  `active_turn` on the next snapshot.
- [u5-fd] `construction/oxpilot-tools/functional-design/
  functional-spec.md` — `card{diff, ref_id}` + `card{consent,
  ref_id: grant_id}` emit semantics; `diff_apply` is a USER proc
  (no model-callable equivalent); denied-action surface.
- [u6-fd] `construction/connector-gateway/functional-design/
  functional-spec.md` — WF1 pairing→pending connect grant→
  `grant_respond` flips `connected`; re-consent re-mints a pending
  grant on reads; WF2 consent-required denial carries `grant_id`;
  target `lifecycle`/`pairing_id` correlation fields.
- [u7-fd] `construction/connector-client/functional-design/
  functional-design-questions.md` — the connector verb the dialog
  teaches is `ox pilot pair <code>` (Q2-A decided).
- [shipped] `apps/web/src/routes/oxpilot.tsrx` (index + inline
  `NewSessionCard` + `OxpilotFlagOff`), `oxpilot.$session.tsrx`
  (seed → `OxpilotChatSurface`: EventSource attach, `liveTail`
  overlay, dedupe, reseed-on-terminal), `lib/oxpilot-chat.tsrx`
  (`useOxpilotChat` — `@tanstack/ai-octane` `useChat` + AG-UI
  chunk mapping over the POST fetch-reader), `lib/oxpilot-queries.ts`
  (status/session-list queryOptions + `sessionCreate` +
  `invalidateOxpilotQueries`), `lib/oxpilot-stream.ts`
  (`parseSseBuffer`, `createAppendDedupe`, `postOxpilotChat`,
  `fetchOxpilotSnapshot`, `openOxpilotStream`), the three test
  files + both coverage manifests. `packages/api-client` already
  exposes every C1 proc incl. `target_*`/`grant_*`/`diff_*`/
  `queue_list`/`usage_get`/`turn_cancel`. `components/settings/
  copy-command.tsrx` (`CopyCommand`), `Dialog`/`AlertDialog`/`Checkbox`/
  `RadioGroup`/`Select`/`EmptyState`/`toast.ts` all ship.
- [gap] schema reality: `oxpilot_sessions` has no
  `computer_use_target_id` column and `session_create` takes
  `{repo_owner, repo_name, branch?, goal?}` — the M3/AC1.2.1
  "optional target select" has no persistence seam today (U1
  deferred the column to U6; U6's 0088 migration did not add it;
  U8 lands it as `0090_oxpilot_session_target.sql` in its codegen —
  Q3-A).

## Q1. Feature file layout — where do U8's components live?

- A. New `apps/web/src/components/oxpilot/` dir, kebab-case files
  (`session-row.tsrx`, `target-row.tsrx`, `new-session-dialog.tsrx`,
  `connect-dialog.tsrx`, `sessions-rail.tsrx`, `transcript.tsrx`,
  `tool-call-card.tsrx`, `diff-card.tsrx`, `consent-card.tsrx`,
  `queued-row.tsrx`, `composer.tsrx`, `context-drawer.tsrx`,
  `confirm-dialog.tsrx`, `flag-off.tsrx`), routes stay thin —
  the design-system-mapping's pinned layout ("feature components
  live under `apps/web/src/components/oxpilot/`"), matching the
  repo's `components/settings/` + `components/repo/` precedent.
- B. Everything inline in the two route files — the routes are
  already 260–270 lines each and the unit adds ~14 components; one
  file per surface would crowd past every repo norm.
- C. `components/ui/` — wrong tier; that dir is shared primitives,
  not feature surfaces (ds-map draws exactly this line).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — New `apps/web/src/components/oxpilot/` dir, kebab-case files. Conductor-decided under standing delegation; ratify at PR review.


## Q2. New-session surface — inline card (shipped) or M3 dialog?

- A. Convert to the M3 `Dialog` ("New session"): `[+ New session]`
  button in the PageHeader actions slot opens a labelled,
  focus-trapped, Esc-dismissable dialog with repo `Select`, branch
  `Select` (defaults to the repo's `default_branch`), goal
  `Textarea` (autofocus), optional target `RadioGroup`, Cancel +
  Start actions — AC1.2.1 + AC1.2.3 verbatim, and the modal matches
  M1's chrome (the inline card disappears).
- B. Keep the shipped inline `NewSessionCard` — less churn, but it
  cannot satisfy AC1.2.3 ("labelled, focus-trapped, dismissable with
  Esc") and M1/M3 pin the dialog; an inline section also has no
  target-select seam the story-map assigns.
- C. A separate `/oxpilot/new` route — a third route for one form
  adds a routeTree+manifest row and breaks the M-flow the mockups
  pin.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Convert to the M3 `Dialog` ("New session"): `[+ New session]`. Conductor-decided under standing delegation; ratify at PR review.


## Q3. Optional target pick in the new-session dialog — how does it
bind, given no session↔target column exists?

- A. Ship the picker AND the bind: `session_create` gains an
  additive optional `target_id`, AND the session reads
  `session_get`/`session_list` carry `computer_use_target_id`
  (consumer-proposed C1 amendment, flagged per the contract's
  ownership rules — needs the `computer_use_target_id` column U1
  deferred and U6's `0088` never added; single-line ownership:
  THIS unit lands the additive migration
  `0090_oxpilot_session_target.sql` in its codegen — the read
  side is what the rail chips/drawer bind to). The dialog reads
  `target_list` and offers `None`
  (default) + connected targets; a non-connected pick is refused
  client-side before submit. This lands the story-map's "dialog +
  bind" assignment literally.
- B. Ship the picker as display-only, bound at dispatch time — the
  tools' `target_id`-absent auto-select (U5 BR7.3) resolves a single
  connected target anyway. Dishonest UI: the user's explicit pick
  would be silently ignored.
- C. Drop the picker for v1 — honest, but fails AC1.2.1's literal
  "optional target select" and the story-map row; the dialog then
  only differs from shipped by being a dialog.
- D. Bind via a separate post-create proc — a second round-trip for
  a create-time field; the C1 amendment path is cleaner and additive
  either way.
- X. Other (please specify)

[Answer]: A — Ship the picker AND the bind: `session_create` gains an optional `target_id` plus `computer_use_target_id` on the session reads; U8's codegen lands the additive column migration (`0090_oxpilot_session_target.sql`). Conductor-decided under standing delegation; ratify at PR review.


## Q4. SSE transport + reconnect — the resume strategy?

- A. Keep the shipped split: `POST …/chat` streams over a fetch
  `ReadableStream` (request body rules out `EventSource` — the
  interaction-spec's pinned reason); `GET …/chat/stream` attaches
  over `EventSource` with native auto-reconnect left ON — every
  reconnect replays a `snapshot` frame, which replaces client state
  wholesale (C2 has no `Last-Event-ID`; exactly-once is
  `(message_id, seq)` dedupe on `append`).
- B. Fetch-reader on the GET route too, with a manual backoff loop —
  re-implements EventSource's reconnect badly; the shipped
  EventSource attach + snapshot-resync already covers transient
  drops.
- C. `Last-Event-ID` resume — the contract deliberately has none
  (C2: "No `Last-Event-ID`; exactly-once is by `message_id` +
  `seq`"); inventing it is a contract change.
- D. WebSocket — a second protocol for no gain; `text/event-stream`
  already rides the compression exclusion (FR6.3).
- X. Other (please specify)

[Answer]: A — Keep the shipped split: `POST …/chat` streams over a fetch. Conductor-decided under standing delegation; ratify at PR review.


## Q5. Snapshot vs append reconciliation — how do frames meet state?

- A. `snapshot` = wholesale replace + keyed remount (the shipped
  `key={s-${seed.version}}` pattern): reseed messages, `streamingIds`,
  cards, and queue from the frame, clear `liveTail`; `append` =
  dedupe `(message_id, seq)` then extend the matching message (or
  the `liveTail` bubble for a foreign/attached turn). On attach the
  first snapshot is skipped (the seed already rendered it); every
  later snapshot means a reconnect replay → reseed (AC1.6.1).
- B. Field-level merge of snapshot into existing state — preserves
  optimistic rows, but re-implements the server's authority
  client-side; a row the server deleted would linger.
- C. Refetch transcript via `session_get` + re-render — there is no
  transcript-read proc by design (the snapshot IS the read, C2);
  this invents a round-trip the contract doesn't offer.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — `snapshot` = wholesale replace + keyed remount (the shipped. Conductor-decided under standing delegation; ratify at PR review.


## Q6. Send while a turn is live — the queued path (US1.8)?

- A. Composer never disables (M2 pin). The send path branches on
  live-turn state (own `chat.isLoading` OR a foreign
  `snapshot.active_turn`): live → `oxpilot.message_queue{session_id,
  content, client_msg_id}` → render a `QueuedRow` ("queued — sends
  next") from `queue_list`/`card{queued}` refs; idle → POST `/chat`
  through `useOxpilotChat`. A 409 `turn_active` that races the check
  surfaces the typed error banner (never a silent auto-enqueue —
  C2's forbidden pair). On a terminal error, queued rows stay
  pending with the "still queued — send after retry" line (M8).
  The M8/[ix]-pinned inline queued-message cancel is
  REQUIRED-but-unprocced — C1 ships no queue-delete proc — so
  `queue_delete` is flagged as a THIRD proposed C1 amendment
  beside `session_create.target_id` and `grant_list.session_id`;
  v1 ships without the affordance only if the amendment is
  refused.
- B. Keep the shipped behavior — always POST, surface the 409
  `turn_active` error and let the user retry. Satisfies the
  contract's minimum but fails AC1.8.1 ("sending mid-stream queues
  the message") — the story-map assigns the pending-chip half to U8.
- C. Disable the composer while streaming — directly contradicts
  AC1.8.1 and M2's "composer stays enabled while streaming".
- D. Optimistically insert the queued row locally and skip
  `queue_list` reconciliation — the row then survives nothing; a
  remount or the drain removes the illusion.
- X. Other (please specify)

[Answer]: A — Composer never disables (M2 pin); live sends route to `message_queue` → `QueuedRow`; the M8/[ix] inline cancel is required-but-unprocced → `queue_delete` flagged as a third proposed C1 amendment. Conductor-decided under standing delegation; ratify at PR review.


## Q7. Card dispatch — how do `tool_call`/`card` frames render, and
what happens on unknown kinds or stale refs?

- A. Closed dispatch: `tool_call` frames (`call_id`-keyed — the
  wire discriminant is `type:"tool_call"` verbatim) create/update
  `ToolCallCard`s in place (running→done|error, durable rows from
  `snapshot.messages[].tool_calls` merge identically); `card{consent}`
  → `grant_get` → `ConsentCard`; `card{diff}` → `diff_get` →
  `DiffCard`; `card{queued}` → `queue_list` → `QueuedRow`. Unknown
  `card_kind` renders a generic "update your client" notice card —
  never a crash (C2's additive-field rule means newer card kinds
  can arrive before the UI learns them). Stale refs are states, not
  errors: a resolved grant → consent card's frozen outcome line; a
  drained queue row → the `QueuedRow` drops on the next `queue_list`;
  an applied/absent diff → the card's post-state. Mount/remount
  recovery: `card`/`tool_call` frames die with their turn (the
  `emitted_deltas` buffer is not durable) while pending grants and
  proposals outlive it, so the WF3 mount seed refetches
  `grant_list{session_id}` (a SECOND flagged C1 amendment — the
  proc takes only `target_id` today) and `diff_list{session_id}`
  (already session-scoped), reconstructing pending/frozen cards —
  remounts and second clients keep the approve/deny and apply
  affordances.
- B. Render cards only from live frames; ignore snapshot-resident
  state — a remount mid-consent would silently drop the pending
  consent card (the durable tool_call rows and `pending_queue`
  exist precisely for replay).
- C. Throw/route to an error boundary on unknown kinds — a server
  deploy ahead of the web build would hard-error the whole
  transcript; additive tolerance is the contract's own rule.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Closed dispatch: `tool_call` frames (`call_id`-keyed) create/update; mount recovery refetches `grant_list{session_id}` (flagged second C1 amendment) + `diff_list{session_id}` to reconstruct pending/frozen cards after remount. Conductor-decided under standing delegation; ratify at PR review.


## Q8. Assistant text — markdown, plaintext, or hybrid?

- A. Plaintext (`whitespace-pre-wrap`) while `streaming`; on
  `complete`/`terminal` the row renders via the shipped
  `renderGfm` markdown path. Rationale: streaming fragments are not
  well-formed GFM (a mid-token `**` or fence re-parses wrongly and
  re-layouts every delta — NFR3's typing-echo budget), while the
  builder/debugger stories produce code blocks and lists where GFM
  pays. ToolCallCard `args_summary`/`output_summary` stays
  verbatim `<code>` text, never markdown.
- B. Plaintext everywhere (the shipped behavior) — simplest and
  honest, but diffs/stack traces/lists the agent emits lose all
  structure; M2's inline-code presentation already assumes more
  than pre-wrap.
- C. `renderGfm` on every append — worst of both: re-parse per
  delta at SSE cadence and flickering partial-fence artifacts.
- D. Sanitized HTML from the server — the server emits plain text
  deltas (C2); inventing a second content channel is out of scope.
- X. Other (please specify)

[Answer]: A — Plaintext (`whitespace-pre-wrap`) while `streaming`; on. Conductor-decided under standing delegation; ratify at PR review.


## Q9. Consent card actions — what do the buttons send?

- A. M6's three buttons: `[Allow once]` →
  `grant_respond{grant_id, allow:true, scope:"once"}`; `[Allow for
  this session]` → `scope:"session"`; `[Deny]` → `allow:false` —
  the C1 amendment's once/session choice is the user's at consent
  time (U6 WF2). Card then freezes: re-read `grant_get` renders the
  outcome line (`allowed`/`denied`/`expired`/`cancelled — target
  offline`). No "always allow" affordance exists anywhere (AC4.3.2).
  Buttons are real `Button`s in DOM order with Deny always
  reachable (`role="group"`, `aria-label="Action request"`).
- B. Allow/Deny only — drops the amendment's scope field; a user
  who wants the session-scoped grant (the common computer-use flow)
  would be re-prompted per action.
- C. Add an "always" option — explicitly out of scope for v1
  ("no cross-session 'always allow'", FR5.2/AC4.3.2); the server
  has no such state to stamp.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — M6's three buttons: `[Allow once]` →. Conductor-decided under standing delegation; ratify at PR review.


## Q10. Connect/pair two-step dialog — the grant and pairing dance?

- A. One `Dialog`, two steps per M4. Step 1 (aria-label "Connect a
  system"): name `Input` (the `target_name` field — M4's
  local/remote `RadioGroup` drops; `pairing_create` carries no
  type field, deviation recorded in contract deltas), caps
  `Checkbox`es (exec_shell, fs_read+fs_write pre-checked; GUI
  disabled + note), required understanding `Checkbox` gating
  `[Continue]` → `pairing_create{target_name, caps}` → step 2
  (aria-label re-announces "Pair the connector"): `CopyCommand`
  showing `ox pilot pair <code>` (the U7-decided verb), code
  rendered ONCE and never re-fetched/stored/logged (AC4.1.4), then
  poll `target_list` every `PAIRING_POLL_MS` for the `pairing_id`
  match → on `pending_consent` target, `grant_list{target_id}` for
  the pending connect grant → `grant_respond{allow:true}` — the
  step-1 checkbox+Continue IS the explicit connect consent
  (AC4.2.1's named-caps dialog) applied when the grant materializes
  → `lifecycle:"connected"` → Done. Timeout (pairing row expiry,
  U6's 10-min TTL) renders an error state + retry. Esc/close on
  step 2 abandons the wait — nothing else is created client-side;
  a late-arriving grant stays `pending` server-side and U6's
  BR3.2 re-mint keeps offering it on owner reads — the pinned
  surface is the target row's `[Connect]`/Manage expander, which
  lists pending connect grants via `grant_list{target_id}` with
  `[Allow]`/`[Deny]` → `grant_respond` (BR7.4).
- B. Two clicks of consent — re-ask Allow/Deny on the step-2 grant
  after the connector pairs. Adds a second modal beat for a consent
  the user already gave explicitly in step 1 of the same dialog;
  the mockup shows no such beat.
- C. Manual refresh instead of polling — the "waiting for connector"
  beat is the whole point of the step; making the user poll a list
  is the UX the dialog exists to remove.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — One `Dialog`, two steps per M4. Step 1 (aria-label "Connect a. Conductor-decided under standing delegation; ratify at PR review.


## Q11. Action/target status freshness — polling or push?

- A. Push-first: live state rides the stream (`tool` frames →
  card status; `terminal`; `snapshot.pending_queue`), plus
  targeted invalidation — `invalidateOxpilotQueries` on terminal and
  after every mutation (respond/disconnect/revoke/delete/apply).
  The ONLY client-side polling is the connect dialog's bounded
  pairing window (Q10); the index/rail rely on TanStack Query
  `staleTime` (shipped 10–15 s) — no `refetchInterval` (the
  ds-map's pinned SSE-not-polling stance). `connected·busy` derives
  client-side from in-flight exec/fs tool cards; there is no C1
  `action_status` user proc to poll (it's a model-facing tool verb
  inside U5's catalog — not a UI surface).
- B. `refetchInterval` on `target_list`/`grant_list` for "live"
  chips — honest freshness but turns the index into a poller for
  status that changes on the connector's clock, not the user's;
  contradicts the ds-map's "no refetchInterval polling" row.
- C. New `oxpilot.action_status` user proc + subscribe — contract
  expansion for marginal value; the stream already carries the
  frames that matter.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Push-first: live state rides the stream (`tool` frames →. Conductor-decided under standing delegation; ratify at PR review.


## Q12. Coverage manifests — what rows does U8 add?

- A. Keep + extend the existing rows: `route-coverage.manifest.ts`
  keeps its two `oxpilot.*` happy-dom rows (the integration test
  file grows the new mounts — index sections, dialogs, cards,
  flag-off). `browser-coverage.manifest.ts` rebinds the repo-`Select`
  row onto `new-session-dialog.tsrx` and gains subject-keyed rows
  only on gate-discovered surfaces (`browser-coverage-check.ts:
  255-259` orphan-fails undiscovered files): `oxpilot-new-session`
  (`Select`+`RadioGroup` in a `Dialog`), `oxpilot-connect`
  (`Checkbox`/`Input` + step swap); the button-only surfaces —
  consent card, rail collapse, diff card cluster, session/target
  rows, drawer — exercise inside the happy-dom route rows
  (narrowed at infra-design review).
  Stack-browser rows stay out of scope for this unit (no new e2e
  harness ships here).
- B. One catch-all browser row for the whole surface — the manifest
  is subject-keyed; a single row would leave Select/Radio/Checkbox
  click-race classes (the exact Octane DOM-race risk the manifest
  exists to gate) unasserted.
- C. Stack-browser e2e rows instead — heavier than the unit needs;
  the mandate is route rows + browser rows for high-risk controls,
  not a new e2e suite (NFR9).
- D. Not decided.
- X. Other (please specify)

[Answer]: A — Keep + extend the existing rows: `route-coverage.manifest.ts`. Conductor-decided under standing delegation; ratify at PR review.


## Consolidated Summary Confirmation

Decisions recap:

- Layout: `components/oxpilot/` kebab-case feature dir; routes stay
  thin (Q1).
- New session: M3 `Dialog` from a `[+ New session]` header action;
  repo/branch selects, goal textarea, optional target radio,
  focus-trapped + Esc (Q2).
- Target pick: additive `session_create.target_id` +
  `computer_use_target_id` on `session_get`/`session_list` (flagged
  C1 amendment; U8's codegen lands the `0090_oxpilot_session_target`
  column migration — U6's `0088` didn't); picker reads
  `target_list` (Q3).
- Transport: POST fetch-reader + GET `EventSource` with native
  auto-reconnect; no `Last-Event-ID` (Q4).
- Reconciliation: snapshot wholesale-replaces via keyed remount;
  append dedupes `(message_id, seq)`; first attach snapshot skipped,
  reconnect snapshots reseed (Q5).
- Queue: composer never disables; live-turn sends go
  `message_queue` → `QueuedRow`; 409 races surface typed errors;
  the M8/[ix] inline queue-cancel is required-but-unprocced →
  `queue_delete` flagged as a third C1 amendment (Q6).
- Cards: closed `card_kind` dispatch; unknown kinds render a
  generic notice; stale refs render their resolved/absent state;
  mount recovery refetches `grant_list{session_id}` (flagged
  second amendment) + `diff_list{session_id}` to reconstruct
  pending/frozen cards (Q7).
- Text: plaintext while streaming, `renderGfm` on complete; tool
  summaries stay verbatim `<code>` (Q8).
- Consent: Allow once / Allow for this session / Deny →
  `grant_respond` + `scope`; frozen outcome states; never "always"
  (Q9).
- Connect: M4 two-step in one `Dialog`; `CopyCommand` +
  `ox pilot pair <code>`; bounded `target_list`/`grant_list` poll on
  `pairing_id` → `grant_respond` applies step-1 consent; timeout →
  error + retry; Esc abandons (Q10).
- Freshness: push via stream + targeted invalidation; bounded poll
  only inside the pairing window; busy derived from in-flight tool
  cards (Q11).
- Coverage: keep + extend happy-dom route rows; browser rows only
  on the two discovered surfaces (`oxpilot-new-session` +
  `oxpilot-repo-trigger` rebound, `oxpilot-connect`); button-only
  subjects ride happy-dom (Q12, narrowed at infra-design review).

Does this all look correct before I write the functional-design
artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
