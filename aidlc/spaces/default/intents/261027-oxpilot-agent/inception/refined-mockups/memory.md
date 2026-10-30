# Memory — Refined Mockups (Inception)

- Mode inline (product support folded in); design-lead hat.
- Questions: 4 (format, spec coverage, responsive, a11y) — all conductor-decided, summary confirmed.
- Artifacts: mockups.md (M1–M8 covering all Must surfaces incl. US1.8 queue, consent card states, delete/revoke confirms, flag-off), interaction-spec.md (11 component specs + event→RPC map), design-system-mapping.md (all real Octane primitives; no new base components), accessibility-checklist.md (WCAG 2.1 AA).
- Advisory review READY: R-01..R-05 applied — over-budget in-session notice (AC5.2.2) added; Notice/Spinner replaced with real patterns (animate-spin, inline notice); composer disabled state removed (M7 replaces page); SSE transport pinned to fetch-reader POST + GET reconnect; queued-message drain semantics pinned (pending→send-after-retry or inline cancel).
- Learning persisted: POST SSE cannot use EventSource — fetch-reader contract.
