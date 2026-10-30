# Code Summary — u1-skeleton-slice

Walking skeleton landed end-to-end: `/oxpilot` page → SessionOnly
`oxpilot.*` RPC → `StubProvider` → SSE-streamed reply, with session and
message persistence on `oxidean-db` across all three dialects.

## Backend

- **`crates/oxidean-api-oxpilot` (new crate)** — `oxpilot.status`
  (gate-exempt enablement probe), `oxpilot.session_create`,
  `oxpilot.session_list` RPC procedures (all `SessionOnly`), plus the
  SSE surface: `POST /api/oxpilot/sessions/{id}/chat` (atomic one-live-
  turn claim → detached turn task → `snapshot` → `append`* →
  `terminal` frames) and `GET /api/oxpilot/sessions/{id}/chat/stream`
  (snapshot + live attach; transport close detaches, never cancels).
  `run_turn` carries `ProviderError.code` into the terminal frame so
  provider failures surface stable codes, and marks the assistant row
  `complete`/`error`. Mid-turn attach replays buffered `emitted_deltas`;
  a `Lagged` reader resyncs from that buffer.
- **`crates/oxidean-api-core`** — `oxpilot.rs` slot module:
  `OxpilotSlot` (provider + enablement + `ActiveTurns`), `LlmProvider`
  trait seam, `StubProvider` (deterministic canned stream +
  `provider_error` injection), `ActiveTurn`/`ActiveTurns` (async mutex
  `HashMap<session_id>`; atomic claim → `turn_active` on second live
  turn; `turn_id`-guarded release). `RpcCtx` gained the `oxpilot`
  field; every `build_ctx` call-site propagates it.
- **`crates/oxidean-db`** — migration `0085_oxpilot.sql` ×3 dialects
  (`oxpilot_sessions`, `oxpilot_messages` with `seq`, `status`,
  `client_msg_id`, repo context); `oxpilot.rs` store (caller-scoped
  create/list, ownership-filtered fetch, ordered messages,
  client-msg-id dedupe lookup, status/content transitions);
  `sweep_oxpilot_streaming_messages` boot sweep (orphaned `streaming`
  rows → `error`, run from `main.rs`); `dialect_oxpilot.rs` parity
  test.
- **`crates/oxidean-core`** — `redact.rs`: dependency-free
  pattern-table secrets redactor (`***` mask convention) applied to
  assistant content before persistence.
- **`crates/oxidean-api`** — crate dep + proc registration +
  `PROCEDURES` parity, boot orphan sweep, generated client
  regeneration (`packages/api-client`), `ci.yml` `db-dialects` `-E`
  filter gained `dialect_oxpilot`, `TESTING.md` job table row.

## Frontend

- `routes/oxpilot.tsrx` — index: auth-gated, flag-off explanation
  (names `OXIDEAN_OXPILOT_ENABLED`), session list, new-session form
  with repo `Select`.
- `routes/oxpilot.$session.tsrx` — workspace: transcript hydration
  from snapshot, composer via TanStack AI Octane `useChat`, foreign-
  turn transient tail, reseed on reconnect/terminal.
- `lib/oxpilot-chat.tsrx` — `useOxpilotChat` adapter: `ChatFetcher`
  that POSTs `{content, client_msg_id}`, then translates the raw
  Oxpilot SSE frame stream into AG-UI events (`RUN_STARTED`,
  `TEXT_MESSAGE_START/CONTENT/END`, `RUN_FINISHED`/`RUN_ERROR`) —
  including `RUN_ERROR` from terminal `error` frames and HTTP denials
  (`409 turn_active`).
- `lib/oxpilot-stream.ts` — frame parsing, one-shot snapshot fetch,
  `EventSource` live attach, `(message_id, seq)` append dedupe.
- `lib/oxpilot-queries.ts` — queryOptions factories +
  invalidation helper.
- Route registration triangle: `shells.rs` ×2, `pages/oxpilot.astro` +
  `pages/oxpilot/[session].astro`, `RESERVED_TOP_SEGMENTS` += `oxpilot`.
- Tests: `oxpilot.integration.test.ts` (mount, flag-off, snapshot
  hydration, streamed reply, 409 denial), `oxpilot.browser.test.tsx`
  (Select/portal DOM-race gate), `oxpilot-stream.unit.test.ts`,
  `e2e/stack/oxpilot.stack.test.ts` (SSE frames + <250 ms first-frame/
  snapshot + <150 ms RPC smoke ceilings). Manifest rows ×2 + 1.

## Config

`.env.example` ×3 vars; `docs/CONFIGURATION.md` oxpilot section;
`.railway/railway.ts` `preserve()`; `run-stack-e2e.sh` exports
`OXIDEAN_OXPILOT_ENABLED=1`/`OXIDEAN_OXPILOT_PROVIDER=stub`;
`versioning.toml` component + `depends_on` edge; crate `CHANGELOG.md`.

## Notable deviation from plan

The reseed-remount interaction needed a real fix: the attach-time
snapshot is now skipped (it duplicates the seed) and terminal-triggered
reseed is suppressed while this mount owns the in-flight turn —
otherwise the remount discarded the streamed reply mid-render.
