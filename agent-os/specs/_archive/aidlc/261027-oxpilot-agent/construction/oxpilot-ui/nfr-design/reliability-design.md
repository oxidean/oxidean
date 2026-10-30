# Reliability Design — oxpilot-ui (U8)

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.15), `functional-design/rules.md` (BR3/BR4/BR5/BR6/BR9),
`functional-spec.md` (WF3–WF6, WF12, Negative paths, State
machines), `nfr-design-questions.md` (Q2-A/Q3-A), `requirements.md`
NFR4 (failures surface in-session, never silent stalls) + the NFR3
reconnect clause, `contract-summary.md` C2 (frame rules + non-2xx
vocabulary + the 409 forbidden pair), shipped
`lib/oxpilot-stream.ts`, `lib/oxpilot-chat.tsrx`,
`routes/oxpilot.$session.tsrx` (the shipped seed/attach/dedupe
core U8 extends).

The unit's reliability contract is honest surfaces over a self-
healing stream: every failure lands on a typed textual notice —
never a silent stall, never a dead spinner, never a fabricated
success — and the stream's reconnect path is server-authoritative
replace, never client-side merge.

## Stream lifecycle correctness (NFR4.1–4.5)

### REL-1 — Reconnect → snapshot-reseed chain (NFR4.1)

```text
mount/sessionId change
  -> openOxpilotStream(sessionId, onFrame, {onFatal})   // EventSource,
     native auto-reconnect ON (no Last-Event-ID — C2)
  -> first `snapshot` AFTER attach: attached=true, SKIP
     (the WF3 seed already rendered it — no-op reseed)
  -> every LATER `snapshot`: liveTail.clear() + onReseed()
     -> reseed() runs the 4-read set -> setSeed(version+1)
     -> OxpilotChatSurface remounts keyed `s-${version}`
        — server-authoritative WHOLESALE replace, never merge
```

Reconnect correctness lives entirely in the server's replayed
snapshot; the client keeps no merge logic to get wrong.

### REL-2 — Dedupe across replay (NFR4.2)

`append` frames dedupe on `"{message_id}:{seq}"` through
`createAppendDedupe` — the SAME set covers the snapshot replay
overlay and the live fanout re-delivery after a reconnect
(oxpilot-stream.ts:74-83; the fetcher's own `seen` set covers the
POST-path duplicates, oxpilot-chat.tsrx:74-87). NFR3's "reconnect
resumes without duplicating messages" lands here — replayed and
live copies of a delta can never both render.

### REL-3 — `es.onerror` one-shot probe (NFR4.3, Q2-A)

```text
es.onerror
  -> es.readyState === CLOSED
       => HTTP-failed (EventSource only re-establishes on network
          error; a non-2xx response closes permanently) — es.close()
          + ONE apiClient.oxpilot.sessionGet{session_id} to name it:
          not_found => notice + back link; oxpilot.disabled =>
          flag-off swap; any other code => generic stream-error
          banner (onFatal(code)); NEVER a silent dead spinner
          (the probe call itself is try/catch'd — its throw degrades
          to the same generic banner; an unanswerable probe is never
          a dead stream)
  -> es.readyState === CONNECTING
       => genuine network drop — native retry already in flight;
          no probe spent; the next snapshot reseeds wholesale
```

Never blind infinite retry, never a dead spinner — `readyState`
is the fatal-vs-transient discriminator (it costs zero procs on
the common network path); `session_get` runs only on the CLOSED
arm to name the denial (NFR-S5's bound stands — one call per
fatal event, not per drop).

Recorded deviation (not an amendment): upstream NFR4.3/NFR-S5
and FD WF5.4 phrase the probe as one `session_get` per drop
event. WHATWG SSE semantics make that probe meaningless on the
common arm — a CONNECTING error is a network drop with native
retry already in flight, and only CLOSED (HTTP-denial class)
needs naming. The probe's purpose — distinguish a dead session
from a transient drop — is preserved; the discriminator moved to
`readyState` so the wire call fires only where it answers.

### REL-4 — Total cleanup; no dead-mount writes (NFR4.4)

The attach effect's return calls `handle.close()` —
`EventSource.close()` ends auto-reconnect (the shipped
oxpilot.$session.tsrx:131-156 effect shape). Reader lifecycles are
`finally`-cancelled: `fetchOxpilotSnapshot` cancels its reader in
`finally` (oxpilot-stream.ts:170-176); the production
`oxpilotFetcher` cancels in `finally` (oxpilot-chat.tsrx:108-113);
`postOxpilotChat` cancels on `terminal` (the unit-test seam,
oxpilot-stream.ts:124-131). On session-switch the effect re-runs
— the old handle closes before the new attach. NOTHING writes
into a dead mount: frame handlers are closures over a mounted
surface that no longer exists once the effect cleans up.

### REL-5 — Chained-turn reattach (NFR4.5)

An attached reader's stream ENDS on the first `terminal` (server
closes the route); EventSource reconnect yields the next
`snapshot` whose `active_turn` advertises the successor — the
reseed picks up the successor's buffered appends. A probe or
reseed landing in the release→reclaim gap sees
`active_turn:null` — transient, rendered as an idle session,
never an error (WF5.6, U3-WF6).

## Truthfulness of rendered state (NFR4.6–4.15)

### REL-6 — The negative-path taxonomy (NFR4.6)

Every negative path renders a typed textual notice; errors never
clear the transcript and never leave a spinner running:

| condition | surface |
|---|---|
| `turn_active` (409 on idle POST — raced past the live check) | typed composer banner; NEVER auto-enqueue (C2's forbidden pair) |
| `session_active_turn_absent` on `message_queue` | banner; the row is not queued — next send goes through the idle path |
| `grant_not_pending`/`not_found` on `grantRespond` | refetch + freeze with the resolved outcome (REL-7) |
| `pairing_expired` | dialog error inline + back to step 1 |
| `diff_not_pending`/`apply_failed:*`/`forbidden`/`not_found` on apply | typed banner; card holds prior state (U5's stay-pending) |
| `budget_exceeded` on send/queue | the named-env budget notice — `OXIDEAN_OXPILOT_*` vars + CONFIGURATION.md (AC5.2.2) |
| `not_found` on seed/probe | error notice + back link (foreign uniform) |
| `oxpilot.disabled` anywhere | flag-off swap (REL-14) |
| transport drop | `es.onerror` → `readyState` → CONNECTING: native retry / CLOSED: one `session_get` → `onFatal` typed surface |
| `terminal{result:error, code}` | inline transcript notice + `[Retry]` (REL-12) |
| `terminal{result:aborted}` | "Reply stopped" notice |
| unknown `card_kind` / malformed frame | generic notice card / frame skipped (REL-13) |
| list/query failure (sessions, targets, grants, diffs) | inline notice + `[Retry]` — section-scoped (M8) |

`code` strings map to copy verbatim — the banner text names what
happened; `RUN_ERROR{code}` from the fetcher lands on the same
`chat.error` banner path (oxpilot-chat.tsrx:97-102).

### REL-7 — Frozen-card truthfulness (NFR4.7)

Consent cards freeze on respond and re-read `grantGet` for the
outcome line — `allowed` / `denied` / `expired` / `revoked`
("revoked — target access ended") / `cancelled — target offline`
(target disconnect while pending). `grant_not_pending`/`not_found`
on respond ⇒ refetch + freeze with the RESOLVED outcome — no
optimistic lie: the buttons go inert and the outcome line states
only what the server resolved.

### REL-8 — Stale refs are states, not errors (NFR4.8)

Resolved grant ⇒ frozen outcome line; absent queue row ⇒ the
`QueuedRow` drops on the next `queueList` (drained rows resolve
client-side — U3-WF5); applied/absent diff ⇒ the post-state card;
unknown `card_kind` ⇒ the generic "update your client" notice
card — never a crash, never silent.

### REL-9 — Mount/remount recovery: complete or degraded (NFR4.9, Q3-A)

The mount seed pairs the snapshot with `grantList{session_id}`
(flagged C1 amendment) + `diffList{session_id}` to reconstruct
pending/frozen cards — `card`/`tool_call` frames die with their
turn's `emitted_deltas` buffer while pending grants/proposals
outlive it (BR5.5). Recovery mapping: pending grant ⇒ pending
`OxpilotConsentCard`; non-pending ⇒ frozen; pending proposal ⇒
pending `OxpilotDiffCard`; applied/absent ⇒ post-state. A refused
grant amendment degrades recovery to `diffList` + the Manage-
expander `grantList{target_id}` path (BR7.4) — pending consent
still surfaces, just not as transcript cards; the narrowed claim
is recorded in the spec, never silent.

### REL-10 — Send/queue honesty (NFR4.10)

`client_msg_id := crypto.randomUUID()` per send is the C2
idempotency key. A `{queued_id:null, position:null}` receipt (the
transcript-merge resend guard, U3-WF5) ⇒ NO queued row renders.
`409 turn_active` ⇒ typed banner — NEVER auto-enqueue and never
silent-accept (C2's forbidden pair). `session_active_turn_absent`
⇒ banner + the row is not queued — the turn ended between check
and commit; the next send uses the idle POST path.

### REL-11 — Composer honesty (NFR4.11)

The composer NEVER disables and stays usable after any error
(AC1.4.2 — the shipped `disabled={busy || empty}` guard keeps it
editable); `draft` clears ONLY on successful send — an error never
eats typed text (AC1.8.1's "types a reply while the agent works"
survives every failure class).

### REL-12 — Terminal honesty (NFR4.12)

`terminal{result:error, code}` ⇒ the inline transcript notice
("The provider stopped mid-reply — your messages are saved.") +
`[Retry]` → reseed/reattach; `terminal{result:aborted}` ⇒
"Reply stopped". Both leave the transcript and composer intact —
the row's prior content persists, the failure is additive.

### REL-13 — Parser tolerance + the discriminant pin (NFR4.13)

Malformed frames are skipped by `parseSseBuffer`'s per-line
try/catch (oxpilot-stream.ts:60-63) — a bad `data:` line never
kills the stream. The `OxpilotFrame` union keeps its catch-all arm
(`{ type: string; [k: string]: unknown }`, :44) so additive server
frame kinds degrade to skip — never a crash. The `tool_call` arm
matches `type:"tool_call"` VERBATIM — a `type:"tool"` arm never
matches the wire and would silently drop every tool frame; the
verbatim discriminant is pinned in `oxpilot-stream.unit.test.ts`
(Q7-A).

### REL-14 — Degraded-environment swap (NFR4.14)

`oxpilot.disabled` mid-session ⇒ swap to the flag-off surface —
a typed transition, never a half-disabled page with live
affordances; a gated proc would still deny server-side and the UI
mirrors the denial honestly (SEC-9).

### REL-15 — Optional affordance (NFR4.15)

`turnCancel` is the only optional affordance: `[Stop]` on a live
own-turn may call it; `no_active_turn`/`{aborted:false}` ⇒ quiet
no-op — the turn ended anyway; the affordance never blocks or
errors the transcript.

## What the design deliberately does NOT recover

- No `Last-Event-ID` resume — C2 deliberately has none; the
  snapshot reseed IS the recovery (inventing resume is a contract
  change, rejected at FD Q4-C).
- No client-side merge of overlapping transcripts — wholesale
  replace on every post-first snapshot; partial-merge drift is a
  defect class the keyed remount removes by construction.
- No optimistic mutation states — pending visuals exist only as
  the server's own `pending` states; a client-invented "applying"/
  "granting" state would be the optimistic lie NFR4.7 forbids.
- A renderer or fetcher crash inside a frame handler is a defect,
  not a mapped outcome — the stream stays attached (the exception
  propagates out of the `onmessage` callback without closing the
  EventSource) and the next snapshot reseeds; no arm pretends to
  recover component exceptions.
