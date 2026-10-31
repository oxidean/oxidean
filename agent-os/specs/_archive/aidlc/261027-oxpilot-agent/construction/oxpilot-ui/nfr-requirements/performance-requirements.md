# Performance Requirements — oxpilot-ui (U8)

Consumes: functional-design `rules.md` BR4/BR5/BR7/BR8,
`functional-spec.md` (WF3–WF6, Pins, Accessibility),
`frontend-components.md` (state ownership), questions Q2-A/Q3-A,
`inception/requirements-analysis/requirements.md` NFR3 (typing-echo
< ~100 ms at 500 messages as the measurable proxy — aspirational;
first-token and provider budgets bind U3/U4, not the render path),
shipped `lib/oxpilot-stream.ts` (`createAppendDedupe`,
`openOxpilotStream`), shipped `lib/{lazy-diff-rows,markdown,
parse-unified-diff,highlight-diff,relative-time}.ts`.

## Frame-path cost

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | Each `append` frame costs O(delta): dedupe lookup + extend `liveTail[message_id]` — the full transcript NEVER re-renders per frame; `tool_call`/`card` frames update their keyed rows in place; `snapshot` is the only wholesale replace | BR4.2, WF5.2, WF6 |
| NFR3.2 | Assistant text is plaintext `whitespace-pre-wrap` while `streaming` — zero parse cost per delta; `renderGfm` runs ONCE per message on `complete` — never per frame, never per append | BR5.2, FD Q8-A |
| NFR3.3 | The typing-echo proxy stands as the design bar: composer input stays responsive (< ~100 ms echo) at a 500-message session — NFR3's measurable proxy, marked aspirational for later hardening; append-only rendering + boundary-only announcements + bottom-only autoscroll are the posture that keeps per-frame work under frame cadence | NFR3, BR5.6, BR10.1 |
| NFR3.4 | Announcement and scroll work is cadence-bounded: `aria-live` announcements fire at message START/END boundaries only — never per stream frame; scroll writes happen only on appended rows while `atBottom` — scrolled-up ⇒ the "jump to latest" pill renders and ZERO scroll work runs | BR5.6, BR10.1, [ix] scroll contract |
| NFR3.5 | Dedupe memory is bounded: `createAppendDedupe` holds `"{message_id}:{seq}"` keys for the attached lifetime; a reseed/remount rebuilds the set — no accumulation across sessions or remounts (the set persists for the attach lifetime — only `liveTail` clears on terminal) | BR4.2, spec Pins, WF5.2 |

## Read/refetch discipline

| id | requirement | basis |
|----|-------------|-------|
| NFR3.6 | The mount/reseed OXPILOT-DATA read set is the enumerated 4 — `fetchOxpilotSnapshot` + `session_get` + `grant_list{session_id}` + `diff_list{session_id}` — run once per mount and once per post-attach `snapshot` (the first is skipped); `reseed()` runs the identical set, nothing more (cold-cache query-layer reads — `auth.me`/`status`/`session_list`/`target_list` — are the shared shell's, counted separately) | WF3.3, BR3.2, BR5.5, WF5.2 |
| NFR3.7 | Query freshness rides the shipped pins: `staleTime` 15 s (status) / 10 s (session/target lists), `retry:false` on auth-gated probes, ZERO `refetchInterval` anywhere — list refresh arrives via `invalidateOxpilotQueries` on terminal frames + mutations | BR8.1, spec Pins, [ds-map] |
| NFR3.8 | The pairing wait is the ONLY client poll: `target_list` every `PAIRING_POLL_MS` (2 s) matched on `pairing_id`, hard-bounded by the pairing row's `expires_at` (U6's TTL — no shorter client cap invented); no other surface polls, including no `action_status` poll (no C1 user proc exists) | BR7.2, BR8.2, spec Scope |
| NFR3.9 | Card-ref resolution is bounded: each `card` frame triggers at most ONE proc read FROM THE RESOLUTION SET (`grant_get`/`diff_get`/`queue_list`; the consent card's label lookup `target_list` is a cached query-layer read, not a per-card proc call); mount recovery is the two list reads; no per-card polling or refetch loop exists | WF6, BR5.5, C2 card-ref table |
| NFR3.10 | Mutation fan-out is one proc per action: confirms fire a single `session_delete`/`target_disconnect`/`target_revoke`/`diff_apply`; consent a single `grant_respond`; queue a single `message_queue` — `invalidateOxpilotQueries` then batches the refresh | WF4, WF7, WF9, WF10 |

## Bundle & render weight

| id | requirement | basis |
|----|-------------|-------|
| NFR3.11 | Zero new dependencies — `components/oxpilot/` imports only shipped primitives and libs; route-level code splitting stays as shipped (the two `/oxpilot*` routes) | Q3-A, spec Procedure deltas, [codekb] |
| NFR3.12 | The diff pane rides the shipped `lazy-diff-rows` seam so render cost scales with viewport, not patch length; `[Show patch]` `<pre>` renders on demand, not on card mount | WF10.2/10.3, [shipped] |
| NFR3.13 | Index sections resolve independently (`loading → empty \| list \| error(retry)`) — one section's refetch or failure never re-renders or blanks the sibling; lists render server-returned order directly (newest-first), no client re-sort pass | BR1.2, BR1.3 |
