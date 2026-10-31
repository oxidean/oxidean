# Reliability Requirements — oxpilot-ui (U8)

Consumes: functional-design `rules.md` BR3/BR4/BR5/BR6/BR9,
`functional-spec.md` (WF3–WF6, WF12, Negative paths, State machines),
questions Q5-A, `inception/requirements-analysis/requirements.md`
NFR4 (failures surface in-session, never silent stalls — U8's half
is the SSE client lifecycle and the honest render of every outcome)
+ the NFR3 reconnect clause ("SSE reconnect resumes without
duplicating messages"), `contract-summary.md` C2.

## Stream lifecycle correctness

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | EventSource native auto-reconnect stays ON; every `snapshot` frame AFTER the first post-attach one performs a wholesale `setSeed(version+1)` keyed remount (`s-${version}`) — reconnect correctness is server-authoritative replace, never client-side merge; the first post-attach snapshot is skipped as a no-op reseed | BR3.2, BR4.2, WF5.2, FD Q5-A |
| NFR4.2 | `append` frames dedupe on `"{message_id}:{seq}"` — reconnect replays and live-tail overlays can never render a row twice; NFR3's "reconnect resumes without duplicating messages" lands here | BR4.2, C2, NFR3 |
| NFR4.3 | `es.onerror` ⇒ exactly ONE `session_get{session_id}` probe: `not_found`/`oxpilot.disabled` ⇒ `EventSource.close()` — FATAL, native reconnect would retry forever — plus a typed banner naming the outcome; any other probe result ⇒ native reconnect continues to the next `snapshot` reseed. Never blind infinite retry, never a dead spinner | BR9.3, WF5.4, WF12 |
| NFR4.4 | Cleanup is total on unmount/session-switch: the effect return calls `handle.close()` (ends auto-reconnect); `fetchOxpilotSnapshot` and the production `oxpilotFetcher` cancel their readers in `finally` (`postOxpilotChat` cancels on `terminal` — the test-seam variant) — NOTHING writes into a dead mount | BR4.3, WF5.5, WF5.1 |
| NFR4.5 | Chained turns reattach via snapshot: an attached reader ENDS on the first `terminal`; the next snapshot's `active_turn` drives a reseed that picks up the successor's buffered appends; a poll in the release→reclaim gap sees `active_turn:null` — transient, never rendered as an error | WF5.6, U3-WF6 |

## Truthfulness of rendered state

| id | requirement | basis |
|----|-------------|-------|
| NFR4.6 | EVERY negative path renders a typed textual notice — `turn_active`, `session_active_turn_absent`, `grant_not_pending`, `pairing_expired`, `diff_not_pending`, `apply_failed:*` (named per U5's apply-failure rule), `budget_exceeded` (naming the env vars), transport drop, `not_found` (foreign session) — errors never clear the transcript and never leave a spinner running | BR9.1, WF12 |
| NFR4.7 | Frozen cards tell the truth: consent cards freeze on respond and re-read `grant_get` for the outcome line (`allowed`/`denied`/`expired`/`revoked`/`cancelled — target offline`); `grant_not_pending`/`not_found` on respond ⇒ refetch + freeze with the resolved outcome — no optimistic lie | BR5.4, BR6.2, WF7.3 |
| NFR4.8 | Stale refs are states, not errors: resolved grant ⇒ frozen outcome line; absent queue row ⇒ `QueuedRow` drops on the next `queue_list`; applied/absent diff ⇒ the post-state card; unknown `card_kind` ⇒ the generic "update your client" notice card — never a crash, never silent | WF6, BR5.1, FD Q7-A |
| NFR4.9 | Mount/remount card recovery is complete or degraded — never silent: the mount seed pairs the snapshot with `grant_list{session_id}` (flagged C1 amendment) + `diff_list{session_id}` to reconstruct pending/frozen cards; a refused grant amendment degrades recovery to `diff_list` + the Manage-expander `grant_list{target_id}` path (BR7.4) with the narrowed claim recorded in the spec — pending consent still surfaces, just not as transcript cards | BR5.5, BR7.4, spec Negative paths |
| NFR4.10 | Send/queue honesty: `client_msg_id := crypto.randomUUID()` per send is the idempotency key; a `{null,null}` queue receipt (transcript-merge resend) ⇒ NO queued row; `409 turn_active` ⇒ typed banner — never auto-enqueue (C2's forbidden pair); `session_active_turn_absent` ⇒ banner + the row is not queued | WF4, BR4.1, C2 |
| NFR4.11 | The composer NEVER disables and stays usable after any error (AC1.4.2); `draft` clears only on successful send — an error never eats typed text | WF4.4, BR9.2, AC1.8.1 |
| NFR4.12 | `terminal{result:error, code}` ⇒ the inline transcript notice ("The provider stopped mid-reply — your messages are saved.") + `[Retry]` → reseed/reattach; `terminal{result:aborted}` ⇒ "Reply stopped"; both leave the transcript and composer intact | WF12, AC1.4.1/1.4.2 |
| NFR4.13 | Malformed frames are skipped by the shipped parser; the `OxpilotFrame` union keeps its catch-all arm so additive server frame kinds degrade to skip — never a crash; the `tool_call` arm matches the `type:"tool_call"` discriminant VERBATIM (a `type:"tool"` arm would silently drop every tool frame — pinned as a defect) | WF5.3, BR4.2, spec Contract deltas |

## Degraded-environment posture

| id | requirement | basis |
|----|-------------|-------|
| NFR4.14 | `oxpilot.disabled` mid-session ⇒ swap to the flag-off surface — a typed transition, never a half-disabled page with live affordances (a gated proc would still deny server-side; the UI mirrors the denial honestly) | WF12, NFR1.8, Negative paths |
| NFR4.15 | `turn_cancel` is the only optional affordance: `[Stop]` on a live own-turn may call it; `no_active_turn`/`{aborted:false}` ⇒ quiet no-op — the turn ended anyway; the affordance never blocks or errors the transcript | spec Negative paths, U3 semantics |
