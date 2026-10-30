# Scalability Design — oxpilot-ui (U8)

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR-S1–S7), `functional-design/rules.md` (BR1/BR4/BR5/BR7),
`functional-spec.md` (WF3–WF6, WF10/WF11, Pins, State machines),
`frontend-components.md` (state ownership), `nfr-design-
questions.md` (Q3-A/Q5-A), `requirements.md` NFR3 (the
500-message proxy), U5's `MAX_PROPOSAL_FILES`=50 write boundary,
shipped `lib/oxpilot-stream.ts`, `lib/lazy-diff-rows.tsrx`.

Scalability for a render surface is a bound posture: every growth
axis is a per-session collection bounded server-side, one
EventSource per mounted workspace, and a fixed two-route bundle.
Nothing here scales by adding machinery — the bounds are
structural.

## Stream + attach model (NFR-S5)

- Exactly ONE EventSource exists per mounted workspace — the
  attach effect owns it (open on `sessionId` change, `close()` in
  the effect return); session-switch/unmount closes it, so stream
  count cannot grow across navigation or remount (WF5.1/5.5).
- The `es.onerror` probe costs one `sessionGet` per CLOSED
  episode — zero on CONNECTING drops (native retry in flight) —
  and fires once per failure episode, never per retry (REL-3).
- `postOxpilotChat`/`fetchOxpilotSnapshot`/`oxpilotFetcher`
  readers cancel in `finally`/on `terminal` — a send-path stream
  can never outlive its turn or mount.

## Per-session growth axes (NFR-S1–S4)

| axis | bound |
|---|---|
| transcript rows | append-only FULL render — NO windowing/virtualization in v1: rows render once on append and never re-layout per frame; the 500-message session is NFR3's design proxy and the documented bound. A windowed transcript is a recorded non-goal (no shipped transcript-windowing lib exists — `lazy-diff-rows` is diff-scoped), revisited only if the proxy regresses (NFR-S1) |
| session/target lists | server-returned rows in server order (newest-first) — per-user collections small by construction; the Manage expander lists pending connect grants bounded by live pending count; no client-side sort/filter accumulation (NFR-S2) |
| queued-message surface | `QueuedRow`s render from `queueList` wire state only and drop on drain — no client-side queue copy can outgrow what the server reports (NFR-S3) |
| drawer diff render | file list ≤ proposal file count (U5's `MAX_PROPOSAL_FILES`=50 write boundary); the diff pane rides `useLazyDiffRows` — render cost scales with viewport, not patch length (NFR-S4) |

## Client-state bounds (NFR-S6)

Dedupe/`liveTail`/seed state is PER-ATTACH: the dedupe `Set` is
built at attach and persists for the attach lifetime; `liveTail`
clears on `terminal`; EVERYTHING clears on reseed/remount —
`setSeed(version+1)` remounts `OxpilotChatSurface` keyed
`s-${version}`, so the dedupe set, tail map, dialog state, and
scroll position rebuild with the new mount. Nothing accumulates
across sessions, remounts, or reconnects (NFR3.5's memory half).

## Concurrent-client consistency (NFR-S7)

A second tab/client attaches its own stream and reseeds from
snapshots; mount recovery reads (`grantList{session_id}` +
`diffList{session_id}`) reconstruct pending/frozen cards
identically — the UI holds NO shared client-side state to
synchronize ([domain] "local UI state only"): server state is the
single consistency source and each client is an independent
projection of it.

## What this unit deliberately does NOT scale

- No transcript windowing library — recorded non-goal (NFR-S1);
  the append-only posture + the 500-message proxy stand until
  measurement says otherwise.
- No shared cross-tab bus or client-side session cache — each tab
  is an independent projection; server rows carry the truth.
- No per-kind lazy component imports — the card set is four
  components sharing one dispatch; suspense boundaries inside the
  `aria-live` region would fragment announcements (nfr-req Q3-C).
- Session count, attachment fan-out, and stream lifetime are
  server axes (U1/U3): the client holds one stream, one dedupe
  set, and one seed per mounted workspace — concurrent-session
  contention never enters the render surface.
