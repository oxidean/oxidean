# Scalability Requirements — oxpilot-ui (U8)

Consumes: functional-design `rules.md` BR1/BR4/BR5/BR7,
`functional-spec.md` (WF3–WF6, WF10/WF11, Pins, State machines),
`frontend-components.md` (state ownership), questions Q2-A/Q3-A,
`inception/requirements-analysis/requirements.md` NFR3 (the
500-message session proxy), U5's MAX_PROPOSAL_FILES=50 write
boundary.

## Per-session growth axes

| id | requirement | basis |
|----|-------------|-------|
| NFR-S1 | The transcript is append-only full render — NO windowing/virtualization in v1: rows render once on append and never re-layout per frame; the 500-message session is NFR3's design proxy and the documented bound. A windowed transcript is a recorded non-goal (no shipped transcript-windowing lib exists — `lazy-diff-rows` is diff-scoped; stack-browser e2e is out of scope), revisited only if the proxy regresses | NFR3, BR5.6, spec Scope, Q2-A |
| NFR-S2 | Session/target lists render server-returned rows in server order (newest-first) — per-user collections small by construction (a user's own sessions and connected systems); the Manage expander lists pending connect grants bounded by live pending count; no client-side sort/filter accumulation | BR1.3, BR7.4 |
| NFR-S3 | The queued-message surface is bounded by the server's queue: `QueuedRow`s render from `queue_list` wire state only and drop on drain — no client-side queue copy can grow beyond what the server reports | WF4, WF6, BR5.1 |
| NFR-S4 | Drawer diff rendering is bounded by proposal size: the file list is ≤ proposal file count (U5's MAX_PROPOSAL_FILES=50 at the write boundary); the diff pane rides `lazy-diff-rows` so render cost scales with viewport, not patch length | WF10/WF11, NFR3.12 |
| NFR-S5 | Exactly ONE EventSource exists per mounted workspace; session-switch/unmount closes it — stream count cannot grow across navigation; the `es.onerror` probe costs one `session_get` per drop event, not per retry | WF5.1/5.4/5.5, BR9.3 |

## Client-state bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR-S6 | Dedupe/`liveTail`/seed state is per-attach (bounded by attach lifetime — `liveTail` clears on terminal, dedupe persists) and cleared on reseed/remount (`setSeed(version+1)`); nothing accumulates across sessions or reconnects | BR4.2, WF5.2, NFR3.5 |
| NFR-S7 | Concurrent clients are server-consistent by construction: a second tab attaches its own stream and reseeds from snapshots; mount recovery reads reconstruct pending/frozen cards identically — the UI holds no shared client-side state to synchronize ([domain] "local UI state only") | BR5.5, WF5, [ds-map] |

Scalability is otherwise N/A for this unit — the render surface's
growth axes are per-session collections bounded server-side and a
fixed two-route bundle; recorded per Q1-A.
