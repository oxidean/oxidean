# Code Summary — oxpilot-ui (U8)

The `/oxpilot` agentic chat, builder, and debugger surface: an Octane `.tsrx`
index route (sessions + computer-use targets), a `/oxpilot/$session`
transcript workspace driven by TanStack AI Octane (`@tanstack/ai-octane`
`useChat`), the C2 SSE attach/queue/dedupe layer, and the three flagged
server-side contract amendments (`computer_use_target_id` binding,
`oxpilot.queue_delete`, `grant_list{session_id}`) with the `0090` dialect
migrations and regenerated `packages/api-client`.

## New components `apps/web/src/components/oxpilot/`

- **`flag-off.tsrx`** — `OxpilotFlagOff`: the flag-off `EmptyState` naming
  `OXIDEAN_OXPILOT_ENABLED`/`OXIDEAN_OXPILOT_PROVIDER` and linking
  `docs/CONFIGURATION.md` (single external anchor, `// raw-anchor-ok`).
- **`session-row.tsrx`** — `OxpilotSessionRow`: title `AppLink` to
  `/oxpilot/{id}`, owner/repo·branch, status text, relative time; owns the
  `OxpilotConfirmDialog` + `session_delete` mutation.
- **`sessions-rail.tsrx`** — `SessionsRail`: `nav` landmark, single
  responsive `[☰ Sessions]`/`‹›` toggle (`aria-expanded`,
  `oxpilot-rail-toggle`), `aria-current` links, `●N targets` footer.
- **`new-session-dialog.tsrx`** — `NewSessionDialog`: repo Select
  (`oxpilot-repo-trigger`/`oxpilot-repo-option`), `refs/heads/*`-derived
  branch Select with repository-default sentinel, optional goal, connected
  target `RadioGroup`, inline errors; `session_create` then navigate.
- **`target-row.tsrx`** — `OxpilotTargetRow`: lifecycle text map
  (needs-consent/connected·idle·busy/offline/revoked), granted caps,
  pending connect grants with Allow/Deny via `grant_respond`; owns
  disconnect + revoke confirms.
- **`connect-dialog.tsrx`** — `ConnectDialog`: two-step "Connect a system"
  → "Pair the connector"; capability checkboxes + required consent;
  `pairing_code` stays local to `CopyCommand` — never re-fetched, stored,
  logged, or announced; polls `target_list` while pairing.
- **`confirm-dialog.tsrx`** — `OxpilotConfirmDialog`: shared `AlertDialog`,
  Cancel before confirm in DOM order, opt-in `danger` styling, awaits
  async `onConfirm`.
- **`transcript.tsrx`** — `OxpilotTranscript`: `main` landmark,
  boundary-only `aria-live` announcements, plaintext streaming +
  `MarkdownBody` on completed rows only, card-ref resolvers
  (`consent`/`diff`/`queued`/generic), queued rows with the inline `×`
  `queue_delete` cancel, jump-to-latest, explicit `OxpilotConfirmDialog`
  before `diff_apply`.
- **`tool-call-card.tsrx`** — `OxpilotToolCall`: name + status word (never
  icon-only), args/output as `<code>` text nodes.
- **`diff-card.tsrx`** — `OxpilotDiffCard` (+`OxpilotDiffFileRows`):
  title/base/status/files, View/Show-patch/Copy/Create-branch+PR; the only
  `dangerouslySetInnerHTML` sink — `useLazyDiffRows` `contentHtml`
  (SEC-12's reviewed sink); `pr_ref` → `/{owner}/{repo}/pull/{n}`.
- **`consent-card.tsrx`** — `OxpilotConsentCard`: Allow once/session +
  Deny (never disabled mid-request), freezes post-response, textual
  outcomes incl. "Revoked — target access ended.".
- **`queued-row.tsrx`** — `OxpilotQueuedRow`: `role="status"` row — `◐` +
  explicit queued text (BR10.3) + the `×` `queue_delete` affordance.
- **`composer.tsrx`** — `OxpilotComposer`: `form` (`oxpilot-composer`),
  labelled "Message OxPilot" textarea, Enter=newline /
  Ctrl·⌘+Enter=submit, Send↔Queue swap, `{children}` error-banner slot.
- **`context-drawer.tsrx`** — `ContextDrawer`: `complementary`, Esc→close
  with focus return to `oxpilot-context-toggle`, overlay <1024px,
  repo/diff-file-rows/target+caps sections.

## Libraries and routes

- **`lib/oxpilot-chat.tsrx`** — `useOxpilotChat` wrapper over TanStack AI
  Octane `useChat`; the idle-turn send path (fetcher → POST `/chat`
  SSE-reader) with client `client_msg_id` minting.
- **`lib/oxpilot-stream.ts`** — `fetchOxpilotSnapshot`,
  `openOxpilotStream` (EventSource), `postOxpilotChat` (fetch-reader),
  `parseSseBuffer`, `createAppendDedupe` (`message_id:seq`), `toCallCard`,
  wire-carrier types (`snapshot.streaming_ids`/`tool_calls`/`cards`).
- **`lib/oxpilot-queries.ts`** — the full `queryOptions`/mutation set:
  `status`, `sessionList`, `sessionGet`, `targetList` (enabled-gated),
  `grantList` (`{target_id?}`/`{session_id?}` filter shapes), `grantGet`,
  `queueList`, `diffList`, `diffGet`, `usageGet` (`@public` generated
  seam, unread in v1), and mutations `sessionCreate`, `sessionDelete`,
  `pairingCreate`, `targetDisconnect`, `targetRevoke`, `grantRespond`,
  `diffApply`, `messageQueue`, `queueDelete`, `turnCancel`;
  `invalidateOxpilotQueries` batches the `["oxpilot"]` prefix.
- **`lib/route-params.ts`** — the `$session` param reader.
- **`routes/oxpilot.tsrx`** — `OxpilotPage`: `auth.me` + `status` +
  `session_list` + `target_list`; flag-off short-circuit; sessions and
  targets each render pending/error/empty/list states;
  disconnect/revoke ride the shared confirm dialog; success invalidates +
  one toast, failure keeps the dialog open with a typed error; the
  Connect↔New-session dialog transition.
- **`routes/oxpilot.$session.tsrx`** — `OxpilotSessionPage`: the pinned
  4-read reseed (`snapshot` + `sessionGet` + `grantList{session_id}` +
  `diffList{session_id}`, degrading to 3 on amendment refusal), live
  EventSource attach with first-snapshot skip + append dedupe +
  foreign-turn detection, tool/card frames, idle `useChat` sends vs busy
  `message_queue` sends, optimistic queued rows reconciled on `queued_id`
  null, `[Stop]` `turn_cancel` on own turns, `SessionNotFound` shared by
  seed-time and fatal paths, `mountedRef` unmount guard.
- **`routes/settings/ai.tsrx`** — gains the `AppLink` to `/oxpilot#targets`
  (AC1.7.2).
- **`pages/oxpilot.astro` + `pages/oxpilot/[session].astro`** — the
  Astro static entrypoints mounting the two routes.

## Flagged server amendments (landed — Q2-A)

- **`computer_use_target_id`** — nullable binding on `oxpilot_sessions`:
  `0090_oxpilot_session_target.sql` for postgres/mysql/sqlite (append-only,
  `ON DELETE SET NULL` over `0088`'s table); carried atomically through
  `CreateSession`, selected/inserted on all three dialects, rebind/clear
  via `set_session_computer_use_target`, validated owner + connected
  lifecycle at create; surfaced on `OxpilotSession` JSON + generated
  client.
- **`oxpilot.queue_delete`** — registered + dispatched; deletes by
  `(session_id, id)` under `SessionService::delete_queued`'s ownership
  check; returned `queued_id`/`deleted` shapes in the client.
- **`grant_list{session_id}`** — session-scoped listing with optional
  `target_id` filter (`list_grants_for_session`, `GrantListReq`,
  `ConnectorGateway::list_grants{session_id}`), preserving lazy expiry.
- Dialect tests cover the binding round-trip, clearing, `SET NULL`,
  session-scoped grants, target filtering, and queue-delete isolation;
  `rpc_gen.rs` emits byte-identical client output (`rpc-sync` clean).

## Vite build fix

- **`vite-plugins/tanstack-ai-octane.ts`** — `@octanejs/astro`'s
  `shouldSkipOctaneTransform` skips installed packages outside
  `@octanejs/*`, so `@tanstack/ai-octane`'s shipped `.tsrx` sources hit
  rolldown uncompiled (`PARSE_ERROR Expected 'from' but found '{'`). The
  plugin serves the package's `.tsrx` files under virtual ids (no
  `node_modules` segment) so the Octane compiler owns them; registered in
  `astro.config.mjs`.

## Tests and coverage

- **`routes/oxpilot.integration.test.ts`** — flag-off state, index
  enabled state, transcript mount + composer POST shape, 409 denial
  banner; mocks the full `apiClient.oxpilot` surface the reworked routes
  consume.
- **`routes/oxpilot.browser.test.tsx`** — Chromium mounts of the
  new-session repo Select portal + connect flow (zero DOM races).
- **`e2e/stack/oxpilot.stack.test.ts`** — stack-level rows for the
  browser-coverage subjects.
- **`lib/oxpilot-stream.unit.test.ts`** — SSE buffer/dedupe unit rows.
- **`test/browser-coverage.manifest.ts`** — subjects rebound onto the
  regex-discovered surfaces (`new-session-dialog`, `connect-dialog`); the
  corrupted `AUTH_UI` path restored.
- **`test/route-coverage.manifest.ts`** — route rows for `/oxpilot` +
  `/oxpilot/$session`.

## Sources

- `unit-of-work`: `oxpilot-ui` (U8) — the unit this summary covers;
  scope and ownership per `inception/units-generation/unit-of-work.md`
  and the delivery plan.

## Deviations from the plan

- `queue_delete` landed (Q2-A), so the inline `×` cancel ships on
  `OxpilotQueuedRow` rather than the refused-amendment fallback
  ("still queued — send after retry").
- `oxpilotUsageGetQuery` is kept as the planned generated seam but marked
  `/** @public */` — no budget meter is pinned in v1, so the dead-code
  gate treats it as an intentional export.
- Local `bun`/`cargo` builds were not run (standing instruction); compile
  verification is deferred to CI. `oxlint`, `oxfmt`, and the
  `oxpilot.integration.test.ts` vitest file pass locally.
