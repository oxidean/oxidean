# Functional Spec — oxpilot-ui (U8)

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U8 — owns
  `OxpilotWeb` beyond the U1 shell: full index, complete workspace,
  all cards, dialogs, SSE client, flag-off, WCAG AA, coverage rows.
- [story-map] `inception/units-generation/unit-of-work-story-map.md`
  — U8 rows US1.1, US1.2, US1.7, US2.3, US4.5 (primary);
  contributing US1.3, US1.4, US1.6, US1.8, US2.1, US4.1, US4.2,
  US4.3, US4.4.
- [contract] `inception/contract-design/contract-summary.md` — C1
  (`oxpilot.*` families + amendments), C2 (SSE frames + error
  vocabulary + the 409 `turn_active` rule).
- [req] `inception/requirements-analysis/requirements.md` — FR1.x,
  FR2.x, FR3.x, FR5.2/FR5.6, NFR3, NFR7, NFR8, NFR9.
- [domain] `inception/domain-design/components.md` §OxpilotWeb —
  "Local UI state only… the UI enforces nothing itself."
- [ix] `inception/refined-mockups/interaction-spec.md` — component
  specs + event→RPC map (placeholder proc names resolved to C1
  below).
- [mock] `inception/refined-mockups/mockups.md` — M1–M8.
- [a11y] `inception/refined-mockups/accessibility-checklist.md` —
  WCAG 2.1 AA checklist.
- [ds-map] `inception/refined-mockups/design-system-mapping.md` —
  primitive mapping + `components/oxpilot/` + state ownership.
- [u3-fd] `construction/agent-runtime/functional-design/
  functional-spec.md` — the live stream contract (additive
  `call_id`/`call_index` on `tool_call` frames, `card{queued}` on
  queue-commit, chained turns via `active_turn`).
- [u5-fd] `construction/oxpilot-tools/functional-design/
  functional-spec.md` — `card{diff}`/`card{consent}` emit points;
  `diff_apply` is user-only; Denied-reason surface.
- [u6-fd] `construction/connector-gateway/functional-design/
  functional-spec.md` — pairing→pending-grant→`grant_respond`
  connect flow; `lifecycle`/`pairing_id` fields; re-consent re-mint
  on reads.
- [u7-fd] `construction/connector-client/functional-design/` — the
  pairing verb shown in the dialog is `ox pilot pair <code>`.
- [shipped] `apps/web/src/routes/oxpilot.tsrx`,
  `oxpilot.$session.tsrx`, `lib/oxpilot-chat.tsrx`,
  `lib/oxpilot-queries.ts`, `lib/oxpilot-stream.ts`,
  `lib/oxpilot-stream.unit.test.ts`,
  `routes/oxpilot.browser.test.tsx`,
  `routes/oxpilot.integration.test.ts`,
  `test/route-coverage.manifest.ts`,
  `test/browser-coverage.manifest.ts`,
  `packages/api-client/src/index.ts` (every C1 proc + DTO already
  generated), `components/settings/copy-command.tsrx`,
  `components/ui/{dialog,alert-dialog,checkbox,radio-group,select,
  empty-state,skeleton,button,input,label,textarea,list,badge,
  toaster}.tsrx`, `lib/{toast,markdown,parse-unified-diff,
  highlight-diff,lazy-diff-rows,relative-time}.ts`.
- [questions] `functional-design-questions.md` — Q1–Q12 decisions
  (written per the recommended option set pending conductor
  confirmation).

## Scope

U8 turns the shipped U1 skeleton — index list + inline new-session
card + flag-off (`oxpilot.tsrx`), single-column transcript +
composer over the SSE pair (`oxpilot.$session.tsrx`), the stream
lib, the `useChat` binding, the query layer — into the full
`OxpilotWeb` surface. In scope: the M1 index (sessions + targets
sections, all empty/loading/error states, delete confirm), the M3
new-session `Dialog`, the M4 connect/pair two-step `Dialog`, the M2
three-pane workspace (collapsible sessions rail, transcript with
boundary-announced live region, context drawer), all in-transcript
cards (`ToolCallCard`, `DiffCard`, `ConsentCard`, `QueuedRow`), the
queued-send path, the M5 confirm dialogs, the M7 flag-off state on
both routes, the SSE client extensions (tool_call/card frame kinds,
snapshot-replace + append), the `/settings/ai` → `/oxpilot#targets`
link, and the coverage-manifest rows. Everything the UI calls
already exists: every C1 proc is generated in `apiClient.oxpilot.*`
and the SSE routes are live.

Post-build scope addition: the `/admin/oxpilot` sys-admin settings
page (`admin-settings-nav` entry, `AdminSettingsLayout`) — provider
Select with an "Environment default" sentinel that writes
`provider_id: null` (resolved `stub` is release-refused as a direct
write), model/base_url/key fields, enablement Switch, user/instance
budget inputs, and `fetchAdminOxpilotSettings` /
`fetchAdminOxpilotProviders` hydration. Client-side requires
`base_url` for `openai-compatible`. `reason:"restricted"` renders
`PageNotFound` on every `/oxpilot` surface (WF1) and hides the
`/settings/ai` oxpilot section for non-admins.

Out of scope: any proc, route, DTO, or server behavior change
(the UI enforces nothing — [domain]); the `action_status` poll —
no C1 user proc exists for it and none is added (U5's
`action_status` is a model-facing tool verb); an inline
queue-cancel unless the flagged `queue_delete` amendment lands
(required-but-unprocced today — Contract deltas); stack-browser
e2e suites; markdown
authoring help in the composer.

## Component map

Feature components live under `apps/web/src/components/oxpilot/`
([ds-map], Q1-A) — inventory, props/state, a11y, and coverage rows
in `frontend-components.md`. The two route files stay thin: they
own auth gating (`usePageRedirect` on `auth.me` null/error →
`/login?returnTo=/oxpilot[/$session]`), the `oxpilot.status` probe,
and composition.

## Workflows

### WF1 — Index render (`/oxpilot`, M1) — FR1.2, FR1.4, FR1.5, AC1.1.x, AC1.7.x, AC5.1.1

1. `auth.me` → unauthenticated/error ⇒ `usePageRedirect` to
   `/login?returnTo=/oxpilot` (any authenticated user admitted —
   FR1.6, AC1.1.5; the post-build sys-admin gate applies only under
   the resolved `stub` provider — real providers admit any
   authenticated user as specified).
2. `oxpilot.status` (gate-exempt, retry:false, staleTime 15s —
   shipped query options) resolves `{enabled, provider, reason}`.
   Pending ⇒ `Skeleton` block (M8 loading row).
3. `enabled===false` ⇒ `OxpilotFlagOff` (M7): `EmptyState` naming
   the reason text (`flag_off`/`misconfigured`/`no_provider` →
   `flagOffReason` mapping, shipped), `OXIDEAN_OXPILOT_ENABLED`,
   `OXIDEAN_OXPILOT_PROVIDER`, `docs/CONFIGURATION.md` link. No
   interactive controls render (AC5.1.1, NFR8); every gated proc
   would still deny server-side (AC5.1.5).
   Post-build: `reason:"restricted"` (sys-admin-only `stub` resolved)
   renders `PageNotFound` instead — a non-admin learns nothing about
   the feature, including on the session route and mid-session
   disable swaps.
4. `enabled===true` ⇒ two sections:
   - `[+ New session]` `Button` in the `PageHeader` actions slot →
     WF2.
   - **Sessions** (`h2`): `session_list` (enabled-gated query,
     staleTime 10s, `oxpilot.disabled` ⇒ `[]` — shipped) renders
     newest-first `OxpilotSessionRow`s — title `AppLink` to
     `/oxpilot/{id}`, `owner/repo · branch`, status badge (text
     label, never color-only — [a11y]), `formatRelative`
     last-activity. Hover/focus-within reveals `[Delete]` → WF9.
     Zero rows ⇒ `EmptyState` pointing at New session (AC1.1.4).
     Query error ⇒ inline notice + retry `Button` (M8).
   - **Computer use** (`h2`, `id="targets"` — the `/settings/ai`
     link anchor, AC1.7.2): `target_list` renders
     `OxpilotTargetRow`s — `label`, `lifecycle`-derived status dot +
     text (presentation maps the wire enum
     `{pending_consent, connected, disconnected, revoked}` →
     "needs consent"/`connected·idle`/`connected·busy`/`offline`/
     `revoked`; "waiting for connector" is the connect dialog's
     internal step-2 state — never a row state), `[Manage]`
     (inline expander: `label`, granted caps, pending connect
     grants via `grant_list{target_id}` with `[Allow]`/`[Deny]`
     driving `grant_respond`, `[Disconnect]`), `[Revoke]` → WF9;
     a `pending_consent` row's `[Connect]` action means "re-offer
     consent" — it opens that same Manage expander's pending-grant
     block (BR7.4). `[Connect a system]` `Button` → WF8. Zero
     targets ⇒ the M1
     explanatory `EmptyState` ("nothing is installed until you run
     it") + `[Connect…]` CTA (AC1.7.3).

### WF2 — New session (M3, Q2-A) — FR1.3, AC1.2.x

1. `Dialog` labelled "New session", focus-trapped, Esc-dismissable
   (AC1.2.3), opened from the header action.
2. Fields: repo `Select` (required; `repo.listMine`), branch
   `Select` (options from `repo.refs{owner,name}` — the shipped
   refs proc; defaults to the repo's `default_branch`; empty ⇒ the
   server defaults — shipped `session.rs` behavior), goal `Textarea`
   (autofocus, optional), target `RadioGroup` "Computer-use target
   (optional)": `None` (default) + connected `target_list` rows +
   a `[Connect new…]` shortcut into WF8.
3. Submit ⇒ `oxpilot.session_create{repo_owner, repo_name, branch?,
   goal?, target_id?}` (Q3-A additive amendment — flagged in
   Contract deltas) → success ⇒ `invalidateOxpilotQueries` +
   navigate `/oxpilot/{id}` (shipped pattern); typed error ⇒
   inline banner above the actions (M3 — `oxpilot.disabled` maps to
   the M7 wording, `rpc.bad_input` shows the field message).
4. Cancel/Esc ⇒ close, no call. Submit disabled while pending or
   repo unselected (shipped guard).

### WF3 — Workspace mount (`/oxpilot/$session`, M2) — FR2.1, FR2.4, AC1.3.2, AC1.3.3

1. Auth gate + status probe as WF1; disabled ⇒ the M7 notice
   (shipped workspace variant).
2. `matchPath("/oxpilot/$session", pathname).session` → `sessionId`.
3. Seed: `fetchOxpilotSnapshot(sessionId)` — GET stream's first
   `snapshot` frame, then reader-cancel (shipped) → `setSeed` with
   `toUiMessage`d messages + `streamingIds` (rows
   `status==="streaming"`) + `activeTurn` (the snapshot's
   `active_turn` — the seed is the ONLY pre-attach source: WF5's
   handler skips the first snapshot) + `pendingQueue` + the
   `tool_calls`/card rows + version bump. `session_get{session_id}`
   rides the same mount read — session metadata (title, owner/repo,
   branch) the M2 header and WF11 drawer need and the snapshot does
   not carry. Pending grant/diff cards recover alongside (WF6 —
   R-01): `grant_list{session_id}` (flagged C1 amendment, Contract
   deltas) refetches pending consent grants and
   `diff_list{session_id}` refetches pending proposals;
   reconstructed cards render pending or frozen from the resolved
   state. `reseed()` runs the identical read set, so a
   reconnect-snapshot remount recovers identically. Failure ⇒
   `seedError` banner (shipped). A remount re-seeds identically —
   reopening restores the transcript (AC1.3.3) and its
   still-actionable cards (AC4.3.2, AC2.2.1).
4. Mount `OxpilotChatSurface` keyed `s-${seed.version}` (Q5-A):
   three-pane layout — `nav` rail (WF11), `main` transcript +
   `form` composer (WF4–WF7), `complementary` context drawer
   (WF11). `OxpilotTranscript` autoscrolls on appended rows only
   while scrolled to the bottom; when the user has scrolled up, a
   "jump to latest" pill renders instead ([ix] scroll contract —
   BR5.6). Focus lands on the composer textarea on open ([a11y]).
   Layout breakpoints per [ix]: <768px single column + `[☰
   Sessions]` disclosure; 768–1024px rail collapsible default-open;
   >1024px three panes (rail 240px, drawer 320px).

### WF4 — Composer send (M2) — FR2.2, AC1.8.1

1. `send`: trim draft; empty ⇒ no-op (send `Button` disabled on
   empty — shipped). `client_msg_id := crypto.randomUUID()` per
   send (C2 idempotency key — shipped).
2. Live-turn check (`chat.isLoading` OR a foreign
   `active_turn` — read from the seed's `activeTurn` (the attach
   skips the first snapshot, so the seed is its only source)
   or a later reconnect snapshot):
   - **Idle** ⇒ `chat.sendMessage(content, {body:{client_msg_id}})`
     → the fetcher's POST `…/chat` fetch-reader maps frames to
     AG-UI chunks (`RUN_STARTED` → `TEXT_MESSAGE_START/CONTENT/
     END` → `RUN_FINISHED`/`RUN_ERROR{code}`) — shipped; `useChat`
     renders the optimistic user row + streamed assistant row.
   - **Live** ⇒ `oxpilot.message_queue{session_id, content,
     client_msg_id}` → `{queued_id, position}` receipt;
     `{null,null}` (transcript-merge resend) ⇒ no queued row
     (U3-WF5's guard). `queued_id` ⇒ `QueuedRow` (WF6); the
     matching `card{queued}` also arrives on the stream.
3. Non-2xx pre-stream ⇒ typed banner: `turn_active` (a race past
   the live check — never auto-enqueue, C2's forbidden pair),
   `budget_exceeded` ⇒ the M8 budget notice naming the
   `OXIDEAN_OXPILOT_*` budget env vars + CONFIGURATION.md
   (AC5.2.2), `oxpilot.disabled` ⇒ swap to the flag-off state,
   `forbidden`/`not_found` ⇒ generic denial text, `bad_input`/`422`
   ⇒ field message. `chat.error` renders the same banner path
   (shipped).
4. Composer never disables (M2/AC1.8.1); `draft` clears on
   successful send.

### WF5 — Stream attach / resume / cleanup — C2, AC1.3.1, AC1.6.1, NFR3

1. On mount (and on `sessionId` change — the effect re-runs, the
   old handle closes): `openOxpilotStream(sessionId)` = `EventSource`
   on `GET …/chat/stream` (Q4-A; native auto-reconnect stays ON).
2. Frame handling (shipped, now pinned): first `snapshot` ⇒ skip
   (the WF3 seed already rendered it — `attached` flag); every
   later `snapshot` ⇒ `liveTail` cleared + `onReseed()` (the
   reconnect replay — wholesale replace, Q5-A); `append` ⇒ dedupe
   `(message_id, seq)` (`createAppendDedupe`) then extend
   `liveTail[message_id]` (foreign/attached turn text overlaying
   the seeded `streaming` row, or a transient bubble for an
   unseeded id); `terminal` ⇒ `liveTail` cleared +
   `invalidateOxpilotQueries` + reseed when `!chat.isLoading` (a
   foreign turn's terminal refreshes the canonical transcript; an
   own-turn terminal is already handled by `useChat`).
3. `tool_call`/`card` frames feed WF6's dispatch (new — the shipped
   union's catch-all arm currently drops them; `OxpilotFrame`
   gains the `tool_call` and `card` variants additively — the wire
   discriminant is `type:"tool_call"` verbatim per C2; a
   `type:"tool"` arm would never match).
4. `es.onerror` ⇒ one `session_get{session_id}` probe
   (`openOxpilotStream` installs the handler — new): `not_found`
   (deleted/foreign session) or `oxpilot.disabled` ⇒
   `handle.close()` — the close is FATAL, native auto-reconnect
   would retry forever — plus a typed banner naming the outcome
   (BR9.3, WF12). Any other probe result ⇒ leave the EventSource
   to native reconnect → the next `snapshot` reseeds.
5. Cleanup: effect return ⇒ `handle.close()` — `EventSource.close()`
   ends auto-reconnect; `postOxpilotChat`/`fetchOxpilotSnapshot`
   readers cancel in `finally` (all shipped). Nothing streams past
   unmount.
6. Chained turns (U3-WF6): an attached reader ENDS on the first
   `terminal`; the next snapshot advertises the successor
   `active_turn` → reseed attaches its buffered appends. A poll in
   the release→reclaim gap sees `active_turn:null` — transient.

### WF6 — Card dispatch (M2) — AC1.3.6, AC1.8.2, AC2.1.1

`tool_call` frames (the C2 `type:"tool_call"` discriminant
verbatim + U3's additive `call_id`/`call_index`) and
`snapshot.messages[].tool_calls` (durable rows, same card)
render `ToolCallCard`: `⟳ running` → `✓ done` / `✗ error`, tool
name + `args_summary`/`output_summary` as inline `<code>` (never
markdown — Q8). `card` frames dispatch on `card_kind`:

| `card_kind` | Ref resolution (C2) | Renders |
|---|---|---|
| `consent` | `grant_get{grant_id: ref_id}` | `OxpilotConsentCard` (WF7) |
| `diff` | `diff_get{proposal_id: ref_id}` | `OxpilotDiffCard` (WF10) |
| `queued` | `queue_list{session_id}` | `OxpilotQueuedRow` set |

- Mount/remount recovery (BR5.5): `card`/`tool_call` frames live
  only in their turn's `emitted_deltas` buffer — replayed to
  attachers while the turn is live, gone after its terminal. A
  pending action grant (U6's 5-min TTL) or pending diff proposal
  outlives its turn, so the WF3 seed pairs the snapshot with
  `grant_list{session_id}` (flagged C1 amendment — Contract
  deltas) + `diff_list{session_id}`: every pending grant
  reconstructs a pending `OxpilotConsentCard`, every pending
  proposal a pending `OxpilotDiffCard`, resolved states
  reconstruct frozen. Remounts and second clients keep the
  approve/deny and apply affordances (AC4.3.2, AC2.2.1).
- Unknown `card_kind` ⇒ generic "update your client" notice card,
  never a crash (Q7-A — C2 additive tolerance).
- Stale refs are states not errors: resolved grant ⇒ frozen
  outcome line; absent queue row ⇒ `QueuedRow` drops on the next
  `queue_list` (drained rows resolve client-side — U3-WF5);
  applied/absent diff ⇒ post-state card.
- `QueuedRow`: `◐` + content + "queued — sends next"; after a
  terminal error "still queued — send after retry" (M8). The
  M8/[ix]-pinned inline cancel is required-but-unprocced — no C1
  proc exists, so `queue_delete` is flagged as a third proposed C1
  amendment (Q6-A, Contract deltas); v1 ships without the
  affordance only if the amendment is refused.

### WF7 — Consent card round-trip (M6) — FR5.2, AC4.2.1, AC4.3.2, AC4.3.4, AC4.3.5

1. `card{consent, ref_id}` ⇒ `grant_get` ⇒ `GrantView{kind,
   capability, args_summary, state, expires_at}` + target label via
   `target_list` ⇒ `OxpilotConsentCard` `role="group"`
   `aria-label="Action request"` inside the transcript live region
   (announced once on appearance — [a11y]).
2. `state==="pending"` ⇒ three `Button`s DOM-order: `[Allow once]`
   → `grant_respond{grant_id, allow:true, scope:"once"}`;
   `[Allow for this session]` → `scope:"session"`; `[Deny]` →
   `allow:false` (Q9-A — C1 amendment; no "always" exists).
3. On respond ⇒ card freezes + re-read `grant_get` ⇒ outcome line:
   `allowed`/`denied` ("Denied. The agent continues without this
   capability." — M6); refetch-time `expired` ⇒ "Request expired";
   `revoked` ⇒ "revoked — target access ended" (`target_revoke`
   sets ALL the target's grants `revoked` incl. pending — U6
   BR3.1); target disconnect while pending ⇒
   `cancelled — target offline` (AC4.3.4). Denial never blocks the
   transcript (AC4.3.5's "tells
   the user" = the frozen denial line + the agent's next turn).
4. Re-pending re-mints a NEW pending grant server-side (U6
   dedupe/re-mint) ⇒ a new pending card — history stays honest.

### WF8 — Connect a system (M4 two-step, Q10-A) — FR5.2, AC4.1.2, AC4.1.4, AC4.2.x, AC1.7.1

1. Step 1 "Connect a system" (`Dialog`, labelled, focus-trapped):
   name `Input` (the `target_name` wire field), capabilities
   `Checkbox`es — `exec_shell` + `fs_read`/`fs_write` pre-checked,
   GUI `disabled` + "not available in this version" note (FR5.3
   bound, AC4.2.1) — required understanding `Checkbox` (real
   labelled input — [a11y]) gating `[Continue]`. M4's
   `local/remote` `RadioGroup` drops — `pairing_create` takes
   `{target_name, caps}` only and `OxpilotTargetView` has no type
   field to render; the deviation is recorded in Contract deltas.
   `[Cancel]`/Esc ⇒ close, nothing created (AC4.2.2).
2. Continue ⇒ `oxpilot.pairing_create{target_name, caps}` ⇒
   `{pairing_id, pairing_code, expires_at}` → step 2 "Pair the
   connector" (dialog label re-announced — [a11y]): `CopyCommand`
   showing `ox pilot pair <pairing_code>` ([u7-fd] verb), "shown
   once — not stored or logged" note; the code lives ONLY in this
   dialog field — never re-fetched, never in transcript/audit
   (AC4.1.4), never announced into the live region.
3. Poll `target_list` every `PAIRING_POLL_MS` for the `pairing_id`
   correlation (U6 amendment field) until `expires_at`
   (`PAIRING_TIMEOUT_MS`):
   - `pending_consent` target ⇒ `grant_list{target_id}` for the
     pending connect grant ⇒ `grant_respond{grant_id, allow:true}`
     — step-1's named-caps consent applied when the grant
     materializes (Q10-A) ⇒ `lifecycle:"connected"` ⇒ "connected —
     target registered as {name}" + `[Done]`.
   - Timeout ⇒ error state + `[Retry]` (re-poll) / `[Done]` close.
4. Esc/close mid-step-2 abandons the wait — the minted pairing row
   ages out server-side and never becomes an index row (target
   rows mint at connector `/pair` redemption, as
   `pending_consent`). A late-arriving connect grant stays
   `pending` and U6-BR3.2's read-time re-mint keeps re-minting it
   on owner reads — the pinned answer surface is the row's
   `[Connect]`/Manage expander, which lists pending connect grants
   via `grant_list{target_id}` with `[Allow]`/`[Deny]` →
   `grant_respond` (WF1, BR7.4). The index `target_list`
   invalidates on `Done` — a `pending_consent` row shows "needs
   consent" on the index.

### WF9 — Confirms (M5 shared `AlertDialog`) — AC1.5.1, AC4.4.1, AC2.2.1

One `OxpilotConfirmDialog` wrapper (aria-label names the object,
focus starts on Cancel, danger styling on destructive confirms):

| Action | Title | Confirm | Then |
|---|---|---|---|
| Delete session | "Delete this session?" | Delete session | `session_delete` → invalidate + toast; row vanishes (AC1.5.3) |
| Revoke target | "Revoke {label}?" | Revoke | `target_revoke` → invalidate + toast; body: ends pending grants incl. in-flight (AC4.4.1) |
| Disconnect target | "Disconnect {label}?" | Disconnect | `target_disconnect` → invalidate + toast |
| Create branch + PR | "Create branch + PR?" | Create PR | `diff_apply` → toast + `pr_ref` link (WF10) |

### WF10 — Diff card + patch + materialize (M2) — AC2.1.x, AC2.2.x, AC2.3.1

1. `card{diff, ref_id}` ⇒ `diff_get{proposal_id}` ⇒
   `ProposedDiff{title, base, files[{path,patch}], status, pr_ref}`
   ⇒ `OxpilotDiffCard`: file stats (+/− counts), `[View diff]`,
   `[Show patch]`, `[Create branch + PR]` (`pending` only).
2. `[View diff]` ⇒ context drawer diff pane: per-file patch via
   `parseUnifiedDiffLines`/`highlightDiffLines` (shipped libs —
   read-only, syntax-marked).
3. `[Show patch]` ⇒ inline `<pre>` toggle of the raw unified diff,
   copyable (AC2.3.1).
4. `[Create branch + PR]` ⇒ WF9 confirm (names branch + repo) ⇒
   `diff_apply{proposal_id}` ⇒ `{pr_ref}` ⇒ toast + card renders
   the PR `AppLink` to `/{owner}/{repo}/pull/{n}` (AC2.2.2). The UI
   NEVER calls `diff_apply` outside the explicit confirm path —
   no auto-apply exists (AC2.2.3's UI half).
5. `diff_not_pending`/`apply_failed:*`/`forbidden`/`not_found` ⇒
   typed error banner; card stays in its prior state (BR5.7's
   stay-pending semantics — U5).

### WF11 — Workspace chrome: rail + drawer + target chips — FR2.1, AC1.3.2, AC4.5.1

1. Sessions rail (`nav` landmark): session list (same
   `session_list` query — rows carry `computer_use_target_id` via
   the flagged read-side amendment) with active highlight; target
   status footer — `●N targets` + per-target name + status text
   chip (AC4.5.1; `connected·busy` derives client-side from
   in-flight exec/fs tool cards — Q11-A). `[‹]/[›]` collapse
   `Button` `aria-expanded` (rail state is local `useState` —
   [domain]'s "local UI state only").
2. Context drawer (`complementary`, 320px, overlay <1024px):
   repo `owner/name · branch` (from the WF3 `session_get` mount
   read), proposed-diff file list (from `diff_list{session_id}` /
   the last `diff` card), computer-use target name+status+granted
   caps — the session's bound target resolved via
   `session_get.computer_use_target_id` → the `target_list` row
   (same flagged amendment; no proc returns the binding today);
   open state local, `[Context ▸]` toggle `aria-expanded`; Esc
   inside closes + returns focus to the toggle ([a11y]).
3. Rail/drawer content refreshes on `invalidateOxpilotQueries` —
   no polling (Q11-A).

### WF12 — Error surfaces — FR2.2, AC1.4.x, AC4.3.4, NFR4/NFR8

| Condition | Surface |
|---|---|
| `terminal{result:error, code}` | Inline transcript error notice ("The provider stopped mid-reply — your messages are saved.") + `[Retry]` → reseed/reattach (AC1.4.1, AC1.4.2) |
| `terminal{result:aborted}` | "Reply stopped" notice; composer usable |
| Pre-stream POST denial | Typed composer banner by code (WF4.3) |
| `budget_exceeded` | Budget notice naming the env vars (AC5.2.2) |
| `oxpilot.disabled` mid-session | Swap to flag-off state |
| Seed/stream transport failure | `seedError` banner / `es.onerror` ⇒ one `session_get` probe — `not_found`/`oxpilot.disabled` ⇒ fatal close + typed banner; otherwise EventSource auto-reconnect → snapshot reseed (BR9.3) |
| List/query failure (sessions, targets, grants, diffs) | Inline notice + `[Retry]` (M8) |
| Offline/disconnected target during dispatch | tool-call error card + `Denied` reason text (AC4.3.4); consent card `cancelled` state |
| Unknown card kind / malformed frame | generic notice card / frame skipped ([shipped] parser) |

## State machines

**Index page**: `loading → disabled(M7) | enabled`; `enabled`
sections each `loading → empty | list | error(retry)`.
**Target row**: `needs-consent → connected·idle ⇄ connected·busy →
offline → revoked` — presentation over the wire `lifecycle` enum
`{pending_consent, connected, disconnected, revoked}`:
`pending_consent` ⇒ "needs consent" label + the `[Connect]`
re-offer action (BR7.4); `disconnected` ⇒ "offline". No `pairing`
row state exists — a target row mints at `/pair` redemption
already `pending_consent`, so "waiting for connector" is the
ConnectDialog's internal step-2 state, never a row state. `busy`
is client-derived — Q11-A.
**ConnectDialog**: `step1(consent) → pairing → waiting →
connected | timeout(retry) | abandoned` (dialog-internal states —
NOT target-row states).
**ConsentCard**: `pending → allowed | denied | expired | cancelled
| revoked` ("revoked — target access ended"; [ix] set + U6
BR3.1's revoke-all; frozen = buttons inert + outcome line).
**QueuedRow**: `queued → delivered(on drain, drops from
`queue_list`) | still-queued-after-error`.
**Assistant message**: `streaming(▌) → complete | error(+Retry) |
aborted`.
**Workspace panes**: rail `open ⇄ collapsed`; drawer `closed ⇄
open`; both survive route-local state only.

## Contract deltas this unit carries

- **C1 consumed verbatim** — `status`, `session_{list,create,get,
  delete}`, `message_queue`, `queue_list`, `turn_cancel` (the Stop
  affordance — optional; see Negative paths), `usage_get` (not
  rendered in v1 — see reverse note), `pairing_create`,
  `target_{list,get,disconnect,revoke}`, `grant_{list,get,respond}`,
  `diff_{list,get,apply}` — all generated client-side already.
  `target_get` is generated but unread in v1 — the pairing poll
  substituted `target_list`-by-`pairing_id` for the [ix]-sketched
  `target_get` poll (`pairing_create` returns no `target_id`);
  it stays available for a later surface.
- **C1 amendments flagged (Q3-A/Q7-A/Q6-A)** — three
  consumer-proposed, additive deltas flagged per the contract's
  deviation rule:
  1. `session_create` gains optional `target_id`, AND the session
     reads `session_get`/`session_list` carry
     `computer_use_target_id` — the binding's read side, which the
     rail chips/drawer/picker need and no proc returns today
     (write-only otherwise). The backing column needs the
     migration U1 deferred to U6 ("`oxpilot_targets`/
     `computer_use_target_id` intentionally absent — U6 creates
     both together so the FK lands with its referent", U1 fd) and
     U6's `0088` shipped without it — single-line ownership:
     THIS unit lands the additive migration as
     `0090_oxpilot_session_target.sql` (all three dialects under
     `crates/oxidean-db/migrations/`) in U8's codegen. If the
     conductor picks Q3-C instead, the picker drops from M3's
     radio group and nothing else changes.
  2. `grant_list` gains optional `session_id` — the WF3/WF6
     mount-time recovery read needs the session's pending consent
     grants (`OxpilotGrantListRequest` today takes only
     `target_id`; `OxpilotGrantView` exposes no `session_id`).
     `diff_list{session_id}` needs NO amendment — it already
     takes `session_id`; only its use extends from the drawer to
     pending-proposal card reconstruction.
  3. `queue_delete` — the M8/[ix]-pinned inline queued-message
     cancel is required-but-unprocced; flagged here rather than
     silently dropped (Q6-A). v1 ships "still queued — send after
     retry" only if the amendment is refused.
- **C2 consumed verbatim** — the six frame kinds incl. U3's
  additive `call_id`/`call_index` and `card{queued}` pushes; the
  tool frame's discriminant is `type:"tool_call"` VERBATIM (C2
  yaml; verified `crates/oxidean-api-oxpilot/src/runtime.rs`) —
  a `type:"tool"` arm never matches. The client-side
  `OxpilotFrame` union gains `tool_call`/`card` variants (its
  catch-all arm stays for forward tolerance).
  `append`-dedupe rule honored: `(message_id, seq)`.
- **api-client frame divergence noted** — `packages/api-client`
  carries an older `OxpilotFrame` sketch (`delta`/`tool_call`/
  `done`/`error`); the shipped `lib/oxpilot-stream.ts` union
  (snapshot/append/`tool_call`/card/terminal/ping) is the real C2
  wire — the lib type stays authoritative; no codegen change
  requested here (the api-client types are unused by the UI).
- **[ix] placeholder procs resolved** — `action_grant`/`action_deny`
  → `grant_respond{allow, scope?}`; `pr_create` → `diff_apply`;
  `target_pair_begin` → `pairing_create`; `oxpilot.tool_call`
  frame → C2 `tool_call` frame + `OxpilotToolCallCard` rows.
- **M4 deviation — `local/remote` radio dropped** — M4's step-1
  `type RadioGroup` has no wire field: `OxpilotPairingCreateRequest
  {target_name, caps}` takes none and `OxpilotTargetView` has none
  to render in the M1 column either. Recorded as a deviation, not
  an amendment (Q3-B's silently-ignored-input pattern is exactly
  what keeping it would be).
- **M1 deviation — `rename` affordance dropped** — M1's Manage
  expander pins a target `rename`; no `target_rename` proc exists,
  so the expander ships label/caps/pending-grants/Disconnect only.
- **`usage_get` unread** — no budget meter is pinned on any mockup;
  the proc stays available for a later surface (reverse-traced).
- **M2 deviation — HEAD sha dropped** — M2's context drawer pins a
  "HEAD sha" line; no proc returns a repo HEAD sha for the
  session context, so the drawer ships without it (recorded
  deviation, same class as the M1 `rename` drop).

## Procedure/file deltas

- `routes/oxpilot.tsrx` — reworked to the M1 composition (header
  action + two sections + dialogs mount); `NewSessionCard` inline
  form replaced by `NewSessionDialog` (Q2-A); flag-off kept.
- `routes/oxpilot.$session.tsrx` — reworked to the three-pane
  `OxpilotChatSurface` (rail + transcript + drawer + card render);
  the seed/attach/send/dedupe/reseed core is kept; the `Seed`
  shape extends to `{version, messages, streamingIds, activeTurn,
  pendingQueue, toolCalls, cards, session}` (WF3) with `reseed()`
  running `session_get` + `grant_list{session_id}` +
  `diff_list{session_id}` alongside `fetchOxpilotSnapshot`.
- `lib/oxpilot-stream.ts` — `OxpilotFrame` gains the `tool_call`
  (`type:"tool_call"` verbatim — C2) and `card` variants;
  `OxpilotMessage` gains `tool_calls?: OxpilotToolCallCard[]`
  (the WF6 `snapshot.messages[].tool_calls` merge and the Q5-A
  reseed need it); `openOxpilotStream` installs `es.onerror` →
  the one-shot `session_get` probe + fatal close (WF5.4, BR9.3);
  `postOxpilotChat` kept for the unit test seam (the
  `useChat` fetcher is the production send path — shipped).
- `lib/oxpilot-chat.tsrx` — fetcher unchanged; `toUiMessage`
  unchanged.
- `lib/oxpilot-queries.ts` — gains `sessionGet`/`targetList`/
  `grantList`/`grantGet`/`queueList`/`diffList`/`diffGet`/`usageGet`
  queryOptions + `pairingCreate`/`grantRespond`/`targetDisconnect`/
  `targetRevoke`/`sessionDelete`/`diffApply`/`messageQueue`
  mutation helpers; all `enabled`-gated where a list is.
- `components/oxpilot/*.tsrx` — the new feature dir (Q1-A).
- `routes/settings/ai.tsrx` — gains an `AppLink` to
  `/oxpilot#targets` in the integrations surface (AC1.7.2).
- `test/route-coverage.manifest.ts` — the two `oxpilot.*` rows
  stay (happy-dom on `oxpilot.integration.test.ts`, extended).
- `test/browser-coverage.manifest.ts` — rebinds the repo-`Select`
  row (`surface:` → `components/oxpilot/new-session-dialog.tsrx`);
  gains subject-keyed browser rows only on gate-discovered
  surfaces (`browser-coverage-check.ts:255-259` orphan-fails
  undiscovered files): `oxpilot-new-session` on
  `new-session-dialog.tsrx`, `oxpilot-connect` on
  `connect-dialog.tsrx`. Button-only surfaces (consent card,
  rail collapse, diff card cluster, session/target rows,
  drawer) exercise inside the
  happy-dom route rows — Q12-A, narrowed at infra-design review.
- codegen (server side, flagged amendment #1) —
  `crates/oxidean-db/migrations/{mysql,postgres,sqlite}/
  0090_oxpilot_session_target.sql` lands the additive
  `oxpilot_sessions.computer_use_target_id` column this unit owns
  (U6's `0088` didn't; Contract deltas), plus the generated
  `target_id`/`computer_use_target_id`/`session_id` request- and
  view-field additions the amendments name.
- Tests: `oxpilot.integration.test.ts` extends (targets section,
  dialogs, cards, flag-off, queue path); `oxpilot.browser.test.tsx`
  extends per the manifest rows; `oxpilot-stream.unit.test.ts`
  covers the new frame variants + dedupe.

## Negative paths

- Unauthenticated ⇒ `/login?returnTo=…` redirect (both routes).
- `oxpilot.disabled` anywhere ⇒ flag-off surface; a proc-level
  `oxpilot.disabled` error mid-flow ⇒ same swap.
- Foreign/absent session (`not_found` on snapshot/`session_get`)
  ⇒ error notice + back link (AC5.5.4's UI half).
- `409 turn_active` ⇒ typed banner; never auto-enqueue (C2).
- `session_active_turn_absent` on `message_queue` (turn ended
  between check and commit) ⇒ the proc's error ⇒ banner + the row
  is not queued — the next send goes through the idle path.
- `grant_not_pending`/`not_found` on `grant_respond` ⇒ card
  freezes with the resolved outcome line on refetch.
- `pairing_expired` on `pairing_create` ⇒ dialog error + back to
  step 1.
- `diff_not_pending`/`apply_failed:*` ⇒ typed banner; card holds
  prior state (U5 BR5.7).
- `budget_exceeded` on send/queue ⇒ the named-env budget notice
  (AC5.2.2).
- SSE transport drop ⇒ `es.onerror` probes `session_get` once —
  `not_found`/`oxpilot.disabled` ⇒ fatal close + typed banner
  (BR9.3); otherwise EventSource auto-reconnect → snapshot
  reseed; malformed frames skip (shipped parser).
- Unmount/session switch ⇒ stream close + reader cancels; no
  frame writes into a dead mount.
- `turn_cancel` is the only optional affordance: a `[Stop]` button
  on a live own-turn may call it; `no_active_turn`/`{aborted:false}`
  ⇒ quiet no-op (the turn ended anyway — U3's semantics).
- Refused `grant_list{session_id}` amendment ⇒ the mount/recovery
  read degrades to `diff_list{session_id}` only; pending consent
  then surfaces through the Manage expander's
  `grant_list{target_id}` path (BR7.4) rather than transcript card
  reconstruction — the recovery claim narrows, never silently.

## Accessibility (NFR7 + [a11y], all pinned)

- Exactly one `h1` per route; `nav`/`main`/`complementary`/`form`
  landmarks; sequential heading order (h2 "Sessions"/"Computer
  use").
- Transcript `aria-live="polite"` + `aria-atomic="false"`;
  announcements fire at message START/END boundaries — never per
  stream frame.
- Every `Dialog`/`AlertDialog`: visible label or `aria-label`, focus
  trap, Esc dismissal; step change re-announces the label
  ("Pair the connector"); confirms focus Cancel on open.
- Consent: real labelled `Checkbox`; `ConsentCard` `role="group"`
  `aria-label="Action request"`; Deny always keyboard-reachable.
- Pairing code: read-only `Input`/`CopyCommand` + labelled copy —
  never rendered into the transcript live region.
- Composer: visible `Label` "Message OxPilot" +
  `Ctrl/⌘+Enter` `aria-describedby` hint — backed by a pinned
  keydown handler on the `<Textarea>`: Enter inserts a newline,
  `Ctrl/⌘+Enter` submits the WF4 send path (the handler IS the
  pinned behavior; without it the hint is a false affordance).
- Transcript scroll contract ([ix] pin on `OxpilotTranscript`):
  autoscroll on appended rows only while scrolled to the bottom;
  scrolled up ⇒ position never moves and a "jump to latest" pill
  renders — activating it scrolls to the newest row (BR5.6).
- Rail/drawer toggles `aria-expanded`; Esc in a pane returns focus
  to its toggle; route open focuses the composer.
- Status is never color-only — text labels everywhere
  (`connected·idle`, `offline`, `needs consent`, `errored`,
  `streaming`);
  row actions reachable via keyboard (`focus-within` reveal, not
  hover-only).
- Contrast ≥4.5:1 text / ≥3:1 badges; error states are textual
  notices.

## Pins (constants)

- `PAIRING_POLL_MS` = 2s; `PAIRING_TIMEOUT_MS` = the pairing row's
  `expires_at` (U6's 10-min TTL — no client-side cap invents a
  shorter window).
- `staleTime`: status 15s, session/target lists 10s (shipped
  values); `retry:false` on auth-gated probes (shipped); no
  `refetchInterval` anywhere ([ds-map]).
- `client_msg_id` = `crypto.randomUUID()` per send/queue.
- Dedupe key = `"{message_id}:{seq}"` (C2).
- Reseed = `setSeed(version+1)` ⇒ keyed remount `s-${version}`
  (Q5-A).
- `GOAL_MAX_CHARS`/`title` derive server-side (U1 pins) — the
  client sends raw; `branch`/`goal` omitted when blank (shipped).
- Copy verb shown: `ox pilot pair <code>` ([u7-fd] Q2-A).
- File layout: `components/oxpilot/` kebab-case (Q1-A, [ds-map]).

## Observability

- No UI-side telemetry in v1 — server audit (U5/U6) owns
  who/what/when; the UI's contribution is only that every mutation
  rides a real C1 proc (never a fabricated state).
- Toasts on mutation success (PR created, target connected/
  revoked, session deleted) — `toast.ts`.
