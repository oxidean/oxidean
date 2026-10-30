# Logical Components — oxpilot-ui (U8)

Consumes: `functional-design/{functional-spec,rules,
frontend-components}.md`, `nfr-requirements/tech-stack-decisions.md`
(TS-1–TS-10 + NFR7/NFR8 conformance), `nfr-design-questions.md`
(Q1–Q7), `contract-summary.md` (C1/C2), shipped
`apps/web/src/{routes,lib,components}` surfaces — every file
below is Octane `.tsrx` (`function X() @{ … }`, `@if`/`@for`/
`@switch`) under the pinned `components/oxpilot/` dir
(kebab-case, TS-1).

## Component map (`apps/web/src/`)

| file | owns | seams consumed |
|---|---|---|
| `routes/oxpilot.tsrx` | `OxpilotPage` — `auth.me` gate + `usePageRedirect`, `oxpilot.status` probe, section composition, dialog mounts | `authSessionQueryOptions`, `oxpilotStatusQueryOptions`, `oxpilotSessionListQueryOptions`, `repoListMineQueryOptions`, `invalidateOxpilotQueries` |
| `routes/oxpilot.$session.tsrx` | `OxpilotSessionPage` — auth gate, `matchPath` sessionId, `Seed` state + `reseed()` (the 4-read set), `seedError` banner, keyed remount `s-${seed.version}`; `OxpilotChatSurface` composition root | `fetchOxpilotSnapshot`, `sessionGet`/`grantList{session_id}`/`diffList{session_id}`, `useOxpilotChat`, `openOxpilotStream`, `createAppendDedupe`, `invalidateOxpilotQueries` |
| `components/oxpilot/flag-off.tsrx` * | `OxpilotFlagOff` — M7 `EmptyState`, reason→copy map, env names, doc link; ZERO interactive controls (SEC-9) | `EmptyState`, `flagOffReason` (lifted from the route) |
| `components/oxpilot/new-session-dialog.tsrx` * | `NewSessionDialog` — M3 fields (repo `Select` required, branch `Select` via `repo.refs`, goal `Textarea`, target `RadioGroup`), `sessionCreate` + navigate | `Dialog`, `Select`/`RadioGroup`/`Textarea`/`Label`, `repoListMine`/`repo.refs`, `sessionCreate`, `invalidateOxpilotQueries`, `toastSuccess` |
| `components/oxpilot/session-row.tsrx` * | `OxpilotSessionRow` — title `AppLink`, `owner/repo · branch`, status badge text, `formatRelative`, `[Delete]` → confirm | `AppLink`, `Badge`, `formatRelative`, `OxpilotConfirmDialog`, `sessionDelete` |
| `components/oxpilot/target-row.tsrx` * | `OxpilotTargetRow` — `label`, caps chips, lifecycle→presentation status text, `[Manage]` expander (`grantList{target_id}` + `[Allow]`/`[Deny]` → `grantRespond`), `[Disconnect]`/`[Revoke]` confirms; `pending_consent` `[Connect]` re-offer (BR7.4) | `grantList{target_id}`/`grantRespond`, `targetDisconnect`/`targetRevoke`, `OxpilotConfirmDialog`, `invalidateOxpilotQueries`, `toastSuccess` |
| `components/oxpilot/connect-dialog.tsrx` * | `ConnectDialog` — M4 two-step (caps `Checkbox` set + `target_name` `Input` + understanding `Checkbox`; step 2 `CopyCommand` + `targetList` poll matched on `pairing_id` bounded by `expires_at`; `grantList`→`grantRespond` consent apply) — the pairing-code custody owner (SEC-14) | `Dialog`, `Checkbox`/`Input`/`Button`, `pairingCreate`/`targetList`/`grantList`/`grantRespond`, `CopyCommand`, `PAIRING_POLL_MS`=2 s |
| `components/oxpilot/confirm-dialog.tsrx` * | `OxpilotConfirmDialog` — the shared M5 `AlertDialog` shell (aria-label names the object, Cancel `initialFocus`, danger styling) — the ONLY path to `diffApply` (SEC-12) | `AlertDialog` family |
| `components/oxpilot/sessions-rail.tsrx` * | `SessionsRail` — `nav` landmark, session list + active highlight, target status footer (`connected·busy` client-derived from in-flight exec/fs tool cards), `[‹]/[›]` collapse `aria-expanded`, `<768px` disclosure | `sessionList`/`targetList` queries, collapse `useState` |
| `components/oxpilot/transcript.tsrx` * | `OxpilotTranscript` — `main` landmark `aria-live="polite"` `aria-atomic="false"`, boundary announce machinery (A11Y-2), `aria-hidden` stream tail, autoscroll/jump-pill (BR5.6), card dispatch (SEC-6) | `parseUnifiedDiffLines`/`highlightDiffLines`/`useLazyDiffRows` (via diff card), `role="status"` announcer, `atBottom` scroll state |
| `components/oxpilot/tool-call-card.tsrx` * | `OxpilotToolCall` — `call_id`-keyed rows: name + `args_summary`/`output_summary` as `<code>` + status text (SEC-3) | `OxpilotToolCallCard` DTO |
| `components/oxpilot/diff-card.tsrx` * | `OxpilotDiffCard` — `diffGet`-resolved `ProposedDiff{title,base,files,status,pr_ref}`: file stats, `[View diff]`→drawer, `[Show patch]` `<pre>`, `[Create branch + PR]`→confirm→`diffApply`→`pr_ref` `AppLink` (SEC-4/SEC-12) | `diffGet`/`diffApply`, `parse-unified-diff`/`highlight-diff`/`lazy-diff-rows`, `AppLink`, `OxpilotConfirmDialog`, `toastSuccess` |
| `components/oxpilot/consent-card.tsrx` * | `OxpilotConsentCard` — `grantGet`-resolved `GrantView`, `role="group"` `aria-label="Action request"`, three verbatim buttons, freeze+refetch honesty (SEC-11/REL-7) | `grantGet`/`grantRespond`, `targetList` (label), `invalidateOxpilotQueries` |
| `components/oxpilot/queued-row.tsrx` * | `OxpilotQueuedRow` — `◐` + content + "queued — sends next" / "still queued — send after retry"; drops when `queueList` drains | `queueList` wire state |
| `components/oxpilot/composer.tsrx` * | `OxpilotComposer` — `form` landmark, `draft` local state, send/queue branch (idle POST via `useChat` vs `messageQueue`), pinned `Ctrl/⌘+Enter` keydown + `aria-describedby` hint, error banner slot, NEVER disables (REL-11) | `chat.sendMessage`, `messageQueue`, `crypto.randomUUID()` |
| `components/oxpilot/context-drawer.tsrx` * | `ContextDrawer` — `complementary` landmark (320px, overlay <1024px), repo context from `sessionGet`, diff pane via `useLazyDiffRows`, bound-target panel via `session_get.computer_use_target_id`→`targetList` (flagged amendment), Esc→toggle focus | `sessionGet`/`diffList`/`targetList`, `parse-unified-diff`/`highlight-diff`/`lazy-diff-rows` |
| `lib/oxpilot-stream.ts` (extended) | `OxpilotFrame` gains `tool_call` (`type:"tool_call"` verbatim — C2) + `card` variants (catch-all stays); `OxpilotMessage.tool_calls?`; `openOxpilotStream` installs `es.onerror` → `readyState` discriminator (CONNECTING: native retry; CLOSED: one `sessionGet` + `onFatal`) (REL-3) | `apiClient.oxpilot.sessionGet` (probe), `EventSource`, `parseSseBuffer` |
| `lib/oxpilot-queries.ts` (extended) | gains `sessionGet`/`targetList`/`grantList`/`grantGet`/`queueList`/`diffList`/`diffGet`/`usageGet` queryOptions + `pairingCreate`/`grantRespond`/`targetDisconnect`/`targetRevoke`/`sessionDelete`/`diffApply`/`messageQueue` mutations — all `enabled`-gated where a list is, `retry:false`, `staleTime` 10–15 s, `invalidateOxpilotQueries` shared | `@octanejs/tanstack-query`, `apiClient.oxpilot.*` |
| `lib/oxpilot-chat.tsrx` (unchanged) | `useOxpilotChat`/`toUiMessage` — the fetcher adapts C2 frames to AG-UI chunks; `(message_id,seq)` dedupe inside the POST path; `RUN_ERROR{code}` on non-done terminal | `@tanstack/ai-octane useChat`, `parseSseBuffer` |
| `routes/settings/ai.tsrx` | gains an `AppLink` to `/oxpilot#targets` in the integrations surface (AC1.7.2) | `AppLink` |

`*` = new file; the rest are shipped files extended in place.

## Component wiring

```text
routes/oxpilot.tsrx (index)
  auth.me ──gate──> usePageRedirect            [SEC-7]
  status ──enabled?──> OxpilotFlagOff | sections   [SEC-9]
    Sessions section ──> OxpilotSessionRow ×N ──> confirm → sessionDelete
    Computer use     ──> OxpilotTargetRow ×N
                          └─ Manage expander: grantList{target_id}
                             → grantRespond | targetDisconnect | targetRevoke
    [+ New session] ──> NewSessionDialog ──> sessionCreate → navigate
    [Connect a system] ──> ConnectDialog
        step1 ──> pairingCreate ──> step2: CopyCommand + targetList
        poll(pairing_id, ≤expires_at) ──> grantList → grantRespond

routes/oxpilot.$session.tsrx (workspace)
  matchPath → sessionId                       [SEC-10]
  reseed(): fetchOxpilotSnapshot + sessionGet
            + grantList{session_id} + diffList{session_id}
            → Seed{version,messages,streamingIds,activeTurn,
                   pendingQueue,toolCalls,cards,session}
            → setSeed(version+1) → keyed remount s-N   [Q3-A]
  OxpilotChatSurface ── useOxpilotChat + liveTail + dedupe
    ├─ SessionsRail (nav)  ── session_list + target chips
    ├─ main:
    │   OxpilotTranscript ── rows + cards + liveTail overlay
    │     ├─ OxpilotToolCall   (tool_call frames / snapshot.tool_calls)
    │     ├─ OxpilotDiffCard   (card{diff} → diffGet)
    │     ├─ OxpilotConsentCard(card{consent} → grantGet → grantRespond)
    │     └─ OxpilotQueuedRow  (card{queued} → queueList)
    │   OxpilotComposer ── idle: chat.sendMessage(POST, fetch-reader)
    │                      live: messageQueue{content, client_msg_id}
    └─ ContextDrawer (complementary) ── session/diff/target context
  attach effect: openOxpilotStream → frames
    snapshot(first)→skip · snapshot(later)→reseed ·
    append→dedupe→liveTail · tool_call/card→keyed rows ·
    terminal→invalidate(+reseed if foreign) ·
    es.onerror→readyState: CONNECTING→native retry;
    CLOSED→one sessionGet→typed fatal              [REL-3]
  cleanup: effect return → handle.close(); readers cancel
    in finally                                              [REL-4]
```

## Dependency direction

- Route files → `components/oxpilot/*` + `lib/*` + `components/ui`
  primitives — never the reverse; the feature dir depends DOWNWARD
  on primitives and generated types only.
- `transcript.tsrx` → the four card components; cards →
  `lib/oxpilot-queries` reads + the diff/highlighter libs; NO
  card→card imports (the closed card dispatch lives in the
  transcript).
- `lib/oxpilot-stream.ts` stays framework-free (plain `.ts`) — it
  calls `apiClient` for the probe and returns handles/callbacks;
  no Octane imports.
- `lib/oxpilot-queries.ts` → `apiClient.oxpilot.*` verbatim —
  hand-edited `packages/api-client` is never the source of truth
  (TS-6; flagged C1 amendments arrive through `make rpc-gen`
  outputs).
- Nothing in the unit imports `dangerouslySetInnerHTML` outside
  the SEC-2 two-sink allowlist — `MarkdownHtml` (completed
  assistant) + the shipped `contentHtml` diff-highlight sink
  (SEC-2 pin).

## Accessibility machinery (NFR7.1–7.5 — this unit OWNS the bar)

| element | mechanism | pins |
|---|---|---|
| A11Y-1 landmarks/headings | exactly one `h1` per route (`PageHeader`); `nav` rail / `main` transcript / `complementary` drawer / `form` composer landmarks; h2 "Sessions"/"Computer use" sequential | NFR7.1 |
| A11Y-2 boundary announce | transcript `aria-live="polite"` `aria-atomic="false"`; `aria-hidden` stream tail + `▌` suppress per-delta announcements; START = "Oxpilot is replying…" hint in-region; END = completed row swap; `role="status"` visually-hidden announcer for boundary strings + consent once-on-appearance | NFR7.2 |
| A11Y-3 dialog semantics | every `Dialog`/`AlertDialog` rides `@octanejs/base-ui` primitives — label/`aria-label`, focus trap, Esc; step swap re-announces ("Pair the connector"); confirms `initialFocus` Cancel | NFR7.3 |
| A11Y-4 interactive honesty | status never color-only (text labels); row actions `focus-within`-revealed; Deny keyboard-reachable; real labelled `Checkbox`/`Input`; contrast ≥4.5:1 / ≥3:1 | NFR7.4 |
| A11Y-5 keyboard contract | visible `Label` "Message OxPilot" + `Ctrl/⌘+Enter` `aria-describedby` hint BACKED by the pinned keydown (Enter=newline, `Ctrl/⌘+Enter`=send); Esc-in-pane → toggle focus; route open → composer focus; autoscroll/jump-pill scroll contract | NFR7.5 |

## Flag-off + fix-naming surface (NFR8.1)

`flag-off.tsrx` owns the explanatory M7 state: `EmptyState` naming
`flagOffReason(reason)`, `OXIDEAN_OXPILOT_ENABLED`,
`OXIDEAN_OXPILOT_PROVIDER`, the `docs/CONFIGURATION.md` link — zero
interactive controls. Typed-error copy (NFR8.2) lands in
observability-design §OBS-4.

## Tech-decision landing (TS-1–TS-10)

| id | lands as |
|---|---|
| TS-1 | Octane `.tsrx` for every file above — `components/oxpilot/` kebab-case feature dir |
| TS-2 | `lib/oxpilot-queries.ts` owns ALL proc reads/mutations; `useState` owns dialog/rail/drawer/`liveTail`/`draft`/`showPatch`/expander — never a global store |
| TS-3 | Transport split verbatim: POST fetch-reader (`oxpilotFetcher`) + `EventSource` attach (`openOxpilotStream`) — extended, never replaced |
| TS-4 | Mockup elements map 1:1 onto shipped primitives — `Dialog`/`AlertDialog`/`Checkbox`/`RadioGroup`/`Select`/`EmptyState`/`Button`/`Input`/`Label`/`Textarea`/`List`/`Badge`/`toaster` + `CopyCommand` reuse |
| TS-5 | Zero new dependencies — the render needs are exactly the libs already linked |
| TS-6 | Generated `apiClient.oxpilot.*` verbatim + `repo.listMine`/`repo.refs`; amendments land via `make rpc-gen` |
| TS-7 | The SEC-1 plaintext/once-GFM split + verbatim `<code>` + escaped diffs — never HTML injection |
| TS-8 | `crypto.randomUUID()` per send/queue — the C2 idempotency key |
| TS-9 | Shipped test trio: `oxpilot.integration.test.ts` (happy-dom, extended), `oxpilot.browser.test.tsx` (subject-keyed rows), `oxpilot-stream.unit.test.ts` — manifests gain the pinned rows |
| TS-10 | The flagged amendment #1's additive column lands as `0090_oxpilot_session_target.sql` ×3 dialects in this unit's codegen — the rail-chip/drawer read side of `computer_use_target_id` |

## Reused seams (zero new deps — TS-5)

| seam | owner | how U8 uses it |
|---|---|---|
| `useChat`/`ChatFetcher` | `@tanstack/ai-octane` + `lib/oxpilot-chat.tsrx` | optimistic user row + streamed assistant row; `RUN_ERROR{code}` → `chat.error` banner |
| `EventSource` + `parseSseBuffer`/`createAppendDedupe`/`openOxpilotStream`/`fetchOxpilotSnapshot` | `lib/oxpilot-stream.ts` | attach, dedupe, seed fetch — extended with `tool_call`/`card` + the probe |
| `renderGfm` + `MarkdownHtml` | `lib/markdown.ts` + `components/repo/markdown-body.tsrx` | the one sanitize-carrying HTML seam — completed assistant text only |
| `parseUnifiedDiffLines`/`highlightDiffLines`/`useLazyDiffRows` | `lib/parse-unified-diff.ts`, `lib/highlight-diff.ts`, `lib/lazy-diff-rows.tsrx` | diff card + drawer pane — escaped, viewport-scaled |
| `formatRelative` | `lib/relative-time.ts` | row timestamps |
| `toastSuccess`/`appToastManager` | `lib/toast.ts` | one toast per confirmed mutation |
| `Dialog`/`AlertDialog`/`Checkbox`/`RadioGroup`/`Select`/`EmptyState`/`Button`/`Input`/`Label`/`Textarea`/`List`/`Badge`/`Skeleton`/`toaster`/`AppLink`/`PageHeader`/`AuthErrorBanner` | `components/ui/*`, `components/auth-shell.tsrx`, `components/app-page.tsrx` | every mockup element — primitives carry focus trap/Esc/`initialFocus` |
| `CopyCommand` | `components/settings/copy-command.tsrx` | the pairing command's sole render site |
| `authSessionQueryOptions`/`usePageRedirect`/`matchPath`/`usePathname` | `lib/session-queries.ts`, `lib/nav.tsrx`, `lib/route-params.ts`, `lib/pathname.tsrx` | auth gate + route params |
| `apiClient.oxpilot.*` + `repo.listMine`/`repo.refs` | `packages/api-client` (generated) | every C1 proc — verbatim, no hand-edited types |
| `route-coverage`/`browser-coverage` manifests + happy-dom/`@octanejs/testing-library` | `test/`, shipped test projects | the pinned coverage rows — extended, not replaced |
| new deps | — | NONE |
