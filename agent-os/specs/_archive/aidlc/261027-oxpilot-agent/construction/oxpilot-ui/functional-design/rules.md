# Business Rules — oxpilot-ui (U8)

## Rule set

### BR1 — Index page (`/oxpilot`, M1)

- **BR1.1** `/oxpilot` renders two sections — `Sessions` and
  `Computer use` (`id="targets"` anchor) — behind an
  `auth.me` gate only; no role check exists or is added
  (FR1.6). `oxpilot.disabled` (flag or proc error) swaps the
  whole page to the flag-off surface (M7).
- **BR1.2** Each section resolves
  `loading → empty | list | error(retry)` independently:
  `EmptyState` + CTA on empty, textual notice + `[Retry]` on
  error; a section's failure never blanks its sibling.
- **BR1.3** Session rows render newest-first server order with
  title link → `/oxpilot/$session`, `owner/repo · branch`,
  status badge (text, never color-only), `formatRelative`
  time. Target rows render `label` (the `OxpilotTargetView`
  wire field), caps chips, the presentation lifecycle
  (`needs-consent → connected·idle ⇄ connected·busy →
  offline → revoked` mapped over the wire enum
  `{pending_consent, connected, disconnected, revoked}` —
  `pending_consent` shows "needs consent"; no `pairing` row
  state exists: "waiting for connector" is the connect
  dialog's internal step-2 state only) and row actions
  (`Connect`, `Disconnect`, `Revoke`). On a `pending_consent`
  row, `Connect` means "re-offer consent": it opens the
  Manage expander, which lists pending connect grants via
  `grant_list{target_id}` with `[Allow]`/`[Deny]` driving
  `grant_respond` (BR7.4).
- **BR1.4** The targets `EmptyState` explains the connector and
  carries the `Connect a system` CTA (M1 copy); the
  `settings/ai.tsrx` integrations surface links to
  `/oxpilot#targets` (AC1.7.2).

### BR2 — New-session dialog (M3, Q2-A/Q3-A)

- **BR2.1** `[+ New session]` opens the M3 `Dialog` titled
  "New session" — labelled, focus-trapped, Esc-dismissable;
  the shipped inline `NewSessionCard` is replaced (Q2-A).
- **BR2.2** Fields: repo `Select` (required), branch `Select`
  (options from `repo.refs{owner,name}` — the shipped refs
  proc; defaults to the repo default), goal `Textarea`,
  optional target `RadioGroup` ("No target" default +
  connected targets). `branch`/`goal` are omitted from the
  payload when blank; `GOAL_MAX_CHARS`/`title` derive
  server-side — the client sends raw (U1 pins).
- **BR2.3** `[Start]` calls `oxpilot.session_create
  {repo_owner, repo_name, branch?, goal?, target_id?}` then
  navigates to the new session route. `target_id` rides the
  additive C1 amendment (Q3-A); if the amendment is refused
  the radio group drops and nothing else changes.
- **BR2.4** Submit failure keeps the dialog open with the
  proc error inline; success toasts. `[Cancel]` restores
  focus to the trigger.

### BR3 — Workspace mount (`/oxpilot/$session`, M2)

- **BR3.1** The workspace is three panes: `nav` rail
  (sessions), `main` transcript, `complementary` context
  drawer — landmarked per the a11y pins; rail `open ⇄
  collapsed` and drawer `closed ⇄ open` are route-local
  state only.
- **BR3.2** Mount seeds from `session_get`/`snapshot`; the
  stream attaches once and every `snapshot` frame AFTER THE
  FIRST performs a wholesale `setSeed(version+1)` keyed
  remount (`s-${version}` — the first post-attach snapshot
  is skipped as a no-op reseed per WF5.2/Q5-A); reopening a
  session restores the persisted transcript.
- **BR3.3** Target chips in the rail/drawer bind the
  session's target (or "No target") — read from
  `session_get.computer_use_target_id`, the read side of the
  flagged C1 amendment (`session_get`/`session_list` carry
  the field; THIS unit lands the additive column migration
  `0090_oxpilot_session_target.sql` in its codegen — U6's
  `0088` didn't); unauthenticated ⇒
  `/login?returnTo=…`; foreign/absent session
  (`not_found`) ⇒ error notice + back link (AC5.5.4 UI
  half).

### BR4 — Send + stream transport (C2)

- **BR4.1** Send keeps the shipped split: `POST …/chat` over a
  fetch reader; the stream is `GET …/chat/stream` via
  `EventSource`. Composer is never disabled (M2 pin); a send
  during a live turn routes to `oxpilot.message_queue`
  (`client_msg_id = crypto.randomUUID()` per send/queue)
  instead of the idle POST path (Q6-A). A `409 turn_active`
  on the idle path ⇒ typed banner, never silent enqueue.
- **BR4.2** Frame handling: `snapshot` = wholesale replace +
  remount (except the skipped first post-attach snapshot —
  WF5.2); `append` dedupes on `"{message_id}:{seq}"`;
  `tool_call`/`card` update keyed rows (the wire discriminant
  is `type:"tool_call"` verbatim — a `type:"tool"` arm never
  matches); `terminal` settles the
  turn; `ping` is ignored. Malformed frames are skipped by
  the shipped parser. EventSource auto-reconnect ⇒ the next
  `snapshot` reseeds (no Last-Event-ID).
- **BR4.3** Unmount or session switch closes the stream and
  cancels the reader — no frame writes into a dead mount.
  `turn_cancel` is the only optional affordance:
  `no_active_turn`/`{aborted:false}` ⇒ quiet no-op.

### BR5 — Transcript cards (M2)

- **BR5.1** Card dispatch is closed (Q7-A): `tool_call`
  frames create/update `OxpilotToolCallCard` rows keyed by
  `call_id` (name + summarized inputs + status);
  `card{diff}` → `OxpilotDiffCard`; `card{consent}` →
  `OxpilotConsentCard`; `card{queued}` → `OxpilotQueuedRow`
  (`◐ queued` chip); unknown `kind` renders the generic
  fallback card — never a crash, never silent.
- **BR5.2** Assistant text renders plaintext
  (`whitespace-pre-wrap`) while `streaming` with the `▌`
  cursor; on `complete` the final text re-renders with the
  GFM renderer (Q8-A).
- **BR5.3** Diff cards resolve `ref_id` to the persisted
  proposal and render the per-file patch via
  `parseUnifiedDiffLines`/`highlightDiffLines`;
  `[Create branch + PR]` fires only through the WF9 confirm
  then `oxpilot.diff_apply` — never implicitly.
- **BR5.4** Consent cards freeze on resolution
  (`pending → allowed | denied | expired | cancelled |
  revoked`, [ix] set + U6 BR3.1's revoke-all — `revoked`
  renders "revoked — target access ended"); a resolved card
  shows its outcome line with buttons inert.
- **BR5.5** Mount/remount card recovery (R-01): `card`/
  `tool_call` frames live only in their turn's
  `emitted_deltas` buffer — gone after its terminal — while
  pending grants/proposals outlive it. The mount seed
  therefore refetches `grant_list{session_id}` (flagged C1
  amendment) + `diff_list{session_id}` alongside the
  snapshot and reconstructs pending/frozen cards — remounts
  and second clients keep the approve/deny and apply
  affordances.
- **BR5.6** `OxpilotTranscript` autoscrolls on appended rows
  only while scrolled to the bottom; scrolled up ⇒ the view
  never moves and a "jump to latest" pill renders —
  activating it scrolls to the newest row ([ix] scroll
  contract).

### BR6 — Consent + confirms (M6/M5)

- **BR6.1** Consent card buttons (M6 verbatim): `[Allow once]`
  → `grant_respond{allow:true, scope:"once"}`,
  `[Allow for session]` → `{allow:true, scope:"session"}`,
  `[Deny]` → `{allow:false}`; the real `Checkbox` is
  labelled; the card is `role="group"` `aria-label="Action
  request"`; Deny is always keyboard-reachable.
- **BR6.2** `grant_not_pending`/`not_found` on respond ⇒ the
  card refetches and freezes with the resolved outcome —
  no optimistic lie.
- **BR6.3** Destructive actions ride M5's shared
  `AlertDialog` (session delete, target disconnect,
  target revoke, `[Create branch + PR]`); the confirm
  button carries the destructive label verbatim; Cancel
  takes initial focus.

### BR7 — Connect/pair two-step (M4, Q10-A)

- **BR7.1** One `Dialog`, two steps. Step 1
  (`aria-label="Connect a system"`): requested-capability
  `Checkbox` set + target name `Input` (the `target_name`
  wire field). M4's `local/remote` `RadioGroup` drops —
  `pairing_create` carries no type field; deviation recorded
  in the spec's contract deltas. Step 2
  (`aria-label="Pair the connector"`): the minted code in a
  read-only `Input` + labelled copy + the verbatim
  `ox pilot pair <code>` instruction; the step swap
  re-announces the label.
- **BR7.2** Step 1 mints via `oxpilot.pairing_create` (its
  response is only `{pairing_id, pairing_code, expires_at}`
  — the client never holds a `target_id`); step 2 polls
  `target_list` every `PAIRING_POLL_MS` (2s), matching the
  `pairing_id` amendment field on the returned rows,
  bounded by the pairing row's `expires_at` (the server
  TTL — no shorter client cap). Success swaps to a
  connected confirm; timeout offers retry;
  `pairing_expired` returns to step 1 with the error
  inline.
- **BR7.3** The pairing code never renders into the
  transcript live region and is never logged.
- **BR7.4** A `pending_consent` target row is never a dead
  end: its `[Connect]` action (BR1.3) re-offers consent —
  opening the Manage expander, which lists pending connect
  grants via `grant_list{target_id}` and offers
  `[Allow]`/`[Deny]` driving `grant_respond`. U6's read-time
  re-mint (its BR3.2) keeps minting pending grants on owner
  reads, so the affordance exists wherever the row does
  (WF1, WF8.4).

### BR8 — Status freshness (Q11-A)

- **BR8.1** Push-first: live state rides the stream (`tool_call`
  frames → busy, `terminal` → idle); lists use
  `staleTime` 15s (status) / 10s (sessions, targets) —
  shipped values — with `retry:false` on auth-gated probes
  and no `refetchInterval` anywhere.
- **BR8.2** Pairing wait is the sole client poll (BR7.2);
  action results arrive via stream/`queue_list`
  invalidation, never a per-action spinner loop.

### BR9 — Errors + negative paths

- **BR9.1** Every negative path surfaces a textual notice —
  `turn_active`, `session_active_turn_absent`,
  `grant_not_pending`, `pairing_expired`,
  `diff_not_pending`, `apply_failed:*` (named per U5's
  apply-failure rule), `budget_exceeded` (named env), transport drop
  (auto-reconnect + reseed), `not_found` (foreign
  session). Errors never clear the transcript and never
  leave a spinner running.
- **BR9.2** `budget_exceeded` on send/queue renders the
  named-env budget notice (AC5.2.2); after any error the
  composer stays usable (AC1.4.2).
- **BR9.3** `es.onerror` never retries blind: the handler
  probes `session_get{session_id}` once — `not_found`
  (deleted/foreign session) or `oxpilot.disabled` ⇒
  `EventSource.close()` (fatal — native reconnect would
  retry forever) + a typed banner naming the outcome; any
  other probe result ⇒ native auto-reconnect continues to
  the next `snapshot` reseed. No infinite silent retry, no
  dead spinner (BR9.1's guarantee).

### BR10 — Accessibility (NFR7 + [a11y])

- **BR10.1** Exactly one `h1` per route;
  `nav`/`main`/`complementary`/`form` landmarks;
  sequential heading order. Transcript
  `aria-live="polite"` + `aria-atomic="false"` announces at
  message start/end boundaries only — never per frame.
- **BR10.2** Every `Dialog`/`AlertDialog`: visible label or
  `aria-label`, focus trap, Esc dismissal; confirms focus
  Cancel on open. Rail/drawer toggles carry
  `aria-expanded`; Esc in a pane returns focus to its
  toggle; route open focuses the composer.
- **BR10.3** Status is never color-only; row actions are
  `focus-within`-revealed, not hover-only; contrast ≥4.5:1
  text / ≥3:1 badges.
- **BR10.4** Composer carries visible `Label` "Message
  OxPilot" + `Ctrl/⌘+Enter` `aria-describedby` hint — backed
  by a pinned keydown handler on the `<Textarea>`: Enter
  inserts a newline; `Ctrl/⌘+Enter` submits the WF4 send
  path.

### BR11 — Coverage + tests (Q12-A)

- **BR11.1** `route-coverage.manifest.ts` keeps the two
  `oxpilot.*` rows (happy-dom,
  `oxpilot.integration.test.ts` extended);
  `browser-coverage.manifest.ts` rebinds the repo-`Select`
  row onto `new-session-dialog.tsrx` and gains subject-keyed
  browser rows only on gate-discovered surfaces
  (`browser-coverage-check.ts:255-259` orphan-fails
  undiscovered files): `oxpilot-new-session`,
  `oxpilot-connect`. Button-only surfaces (consent card,
  rail collapse, diff card cluster, session/target rows,
  drawer) exercise inside the happy-dom route rows —
  narrowed at infra-design review.
- **BR11.2** `oxpilot.integration.test.ts` extends to the
  targets section, dialogs, card kinds, flag-off, and the
  queue path; `oxpilot.browser.test.tsx` extends per the
  manifest rows; `oxpilot-stream.unit.test.ts` covers the
  new `tool_call`/`card` frame variants + append dedupe.

## Notes

- BR numbering is U8-local (BR1–BR11) — cross-unit references say
  "U3-BRx.y"/"U5-BRx.y"/"U6-BRx.y" for the runtime, tools, and
  gateway rule sets.
- BR5.5's mount-time recovery read and BR7.2's `pairing_id` match
  ride flagged C1 amendments (`grant_list{session_id}`,
  `target_list` carries `pairing_id`); a refused
  `grant_list{session_id}` degrades the recovery read to
  `diff_list{session_id}` only — pending consent cards then
  surface via the Manage expander's `grant_list{target_id}`
  path (BR7.4) instead of transcript reconstruction, and the
  card-recovery claim narrows accordingly; `pairing_id` is
  already generated client-side (`OxpilotTargetView`), not a
  pending amendment — it rides in this list for provenance only.
- The three flagged C1 amendments (`session_create.target_id` +
  read fields + `0090_oxpilot_session_target.sql`,
  `grant_list{session_id}`, `queue_delete`) are additive,
  consumer-proposed, owner-committed contract deltas — recorded in
  `functional-spec.md` §"Contract deltas this unit carries".
