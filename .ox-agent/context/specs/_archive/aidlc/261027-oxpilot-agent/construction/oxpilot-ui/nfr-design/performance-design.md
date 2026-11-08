# Performance Design — oxpilot-ui (U8)

Consumes: `nfr-requirements/performance-requirements.md`
(NFR3.1–3.13), `functional-design/rules.md` (BR4/BR5/BR7/BR8),
`functional-spec.md` (WF3–WF6, Pins, Accessibility), `nfr-design-
questions.md` (Q2-A/Q4-A/Q5-A), `requirements.md` NFR3 (typing-echo
< ~100 ms at 500 messages — aspirational proxy), shipped
`lib/oxpilot-stream.ts` (`createAppendDedupe`, `openOxpilotStream`,
`parseSseBuffer`), `lib/oxpilot-queries.ts` (`staleTime` 15 s/10 s,
`retry:false`), `lib/{markdown,highlight-diff,lazy-diff-rows,
relative-time,toast}.*`, `components/ui/*`.

The render path's contract is O(delta) work per frame — the full
transcript NEVER re-renders per frame, and the only wholesale
replace is the `snapshot` reseed. Every mechanism below keeps
per-frame cost under frame cadence so the typing-echo proxy holds
by construction (Q5-A — no runtime instrumentation).

## Per-frame cost budget (NFR3.1/3.2)

| frame | work per frame |
|---|---|
| `snapshot` | FIRST post-attach ⇒ skip (the mount already rendered the seed — `attached` flag); later ⇒ `liveTail` clear + `onReseed()` → wholesale `setSeed(version+1)` keyed remount — the ONLY wholesale replace (NFR4.1) |
| `append` | `dedupe(frame)` Set lookup O(1) + `liveTail[message_id]` string extend — one keyed row's text node mutates; nothing else re-renders |
| `tool_call` | keyed `call_id` row update in place — map insert/patch, never a transcript pass |
| `card` | keyed row insert/update + ≤1 resolution-read dispatch (§"Card-ref resolution bound (NFR3.9)") |
| `terminal` | `liveTail` clear + `invalidateOxpilotQueries` + conditional `onReseed` (`!chat.isLoading` — own-turn terminals are `useChat`'s) |
| `ping` | ignored |
| malformed | skipped by `parseSseBuffer`'s try/catch — zero render |

Assistant text costs ZERO parse while `streaming`
(`whitespace-pre-wrap` text — SEC-1); `renderGfm` runs ONCE per
message on `complete` — never per frame, never per append
(NFR3.2).

## Cadence-bounded work (NFR3.3/3.4)

The NFR3 typing-echo proxy (< ~100 ms echo at a 500-message
session) is the design bar — aspirational, guarded structurally:

- Announcements fire at message START/END boundaries only — the
  `aria-hidden` stream tail (Q4-A) means per-delta mutations never
  queue a live-region announcement; boundary strings ride a
  `role="status"` announcer. Consent cards announce once on
  appearance.
- Scroll writes happen only on appended rows while `atBottom`;
  scrolled up ⇒ the "jump to latest" pill renders and ZERO scroll
  work runs (BR5.6's contract).
- Per-frame GC pressure is one string concat + one Set insert;
  `liveTail` entries die with their row's terminal.

## Dedupe memory bound (NFR3.5)

`createAppendDedupe` holds `"{message_id}:{seq}"` string keys for
the attached lifetime (oxpilot-stream.ts:74-83 — one `Set<string>`
per attach). Size is bounded by append frames seen this attach
(≈ deltas of the live turns); a reseed/remount rebuilds the set —
`setSeed(version+1)` remounts the surface wholesale so nothing
accumulates across sessions or reconnects (NFR-S6). `liveTail`
itself clears on `terminal` and on every reseed.

## The mount/reseed read set (NFR3.6, Q3-A)

Exactly FOUR reads — run once per mount and once per post-attach
`snapshot` reseed (the first post-attach snapshot is skipped):
`fetchOxpilotSnapshot(sessionId)` + `sessionGet{session_id}` +
`grantList{session_id}` + `diffList{session_id}` — parallel awaits
folded into ONE `Seed` and one keyed remount (Q3-A). `reseed()`
runs the identical set, nothing more; a refused
`grant_list{session_id}` amendment drops the set to 3. The shared
shell's own cold-cache reads (`auth.me`/`status`/`session_list`/
`target_list`) are counted separately — they ride the shipped
query layer, not the OXPILOT-DATA set.

## Query freshness (NFR3.7)

The shipped pins stand verbatim — `staleTime` 15 000 on
`oxpilotStatusQueryOptions` (oxpilot-queries.ts:31), 10 000 on
`oxpilotSessionListQueryOptions` (:49) and the target list,
`retry:false` on every auth-gated probe, ZERO `refetchInterval`
anywhere. List refresh arrives via `invalidateOxpilotQueries(qc)`
on `terminal` frames + every mutation — push-first, never polling
(BR8.1).

## Polls — exactly one (NFR3.8)

The pairing wait is the ONLY client poll: `targetList` every
`PAIRING_POLL_MS` (2 s) inside ConnectDialog step 2, matched on
the `pairing_id` field of returned rows
(`OxpilotTargetView.pairing_id`), hard-bounded by the pairing row's
`expires_at` (U6's TTL — no shorter client cap). No other surface
polls — including NO `action_status` poll (no C1 user proc
exists; U5's `action_status` is a model-facing verb).

## Card-ref resolution bound (NFR3.9)

Each `card` frame triggers AT MOST ONE proc read from the
resolution set — `grantGet` / `diffGet` / `queueList` (SEC-6's
table); the consent card's target-label lookup rides the cached
`targetList` query-layer read, not a per-card proc call. Mount
recovery is the two list reads inside the seed. No per-card
polling or refetch loop exists.

## Mutation fan-out (NFR3.10)

One proc per action — confirms fire a single
`sessionDelete`/`targetDisconnect`/`targetRevoke`/`diffApply`;
consent a single `grantRespond`; queue a single `messageQueue`;
stop a single `turnCancel`. `invalidateOxpilotQueries` then
batches the refresh across the `["oxpilot"]` key prefix —
no per-mutation refetch storms.

## Bundle + render weight (NFR3.11–3.13)

- ZERO new dependencies (TS-5/Q3-A): `components/oxpilot/`
  imports only shipped primitives (`Dialog`/`AlertDialog`/
  `Checkbox`/`RadioGroup`/`Select`/`EmptyState`/`Button`/`Input`/
  `Label`/`Textarea`/`List`/`Badge`/`toaster`) and shipped libs
  (`renderGfm`, `parse-unified-diff`/`highlight-diff`/
  `lazy-diff-rows`, `relative-time`, `toast`, `CopyCommand`).
  Route-level code splitting stays as shipped — the two
  `/oxpilot*` routes.
- The diff pane rides the shipped `useLazyDiffRows` seam
  (lib/lazy-diff-rows.tsrx): plain escaped rows paint immediately,
  Shiki highlighting runs only when the block nears the viewport
  inside an idle slice (`scheduleIdle` + IntersectionObserver —
  render cost scales with viewport, not patch length);
  `DIFF_HIGHLIGHT_SOFT_MAX_CHARS` (200 000) keeps first paint off
  the tokenizer. `[Show patch]`'s `<pre>` renders on demand, not
  on card mount.
- Index sections resolve independently
  (`loading → empty | list | error(retry)`) — one section's
  refetch or failure never re-renders or blanks the sibling;
  lists render server-returned order directly (newest-first), no
  client re-sort pass (BR1.2/1.3).

## Headroom statement

The dominant per-frame cost is a `liveTail` string extend plus one
keyed row update — the same order of work a keystroke already
costs. The only multi-millisecond paths are the once-per-message
`renderGfm` on complete and the keyed-remount reseed — both
boundary events, both bounded by the 4-read set + patch size, not
by session age. The 500-message proxy stands as the documented
bar with the append-only posture as its guard; a windowed
transcript is a recorded non-goal (NFR-S1).
