# Performance Design — u1-skeleton-slice

Consumes: `nfr-requirements/performance-requirements.md` (NFR3.1–3.9),
`functional-design/entities.md`, `functional-design/functional-spec.md`,
`inception/contract-design/contract-summary.md` (C2).
Questions: `nfr-design-questions.md` Q2, Q5 (conductor-decided).

The skeleton's performance envelope is small and closed: three RPC
procedures, two SSE routes, one provider call per turn. Every design
below is an existing-stack pattern; nothing is tuned against a harness
that does not exist — N=1 smoke ceilings per the measurement protocol.

## Index & read-path design

| pattern | design | covers |
|---------|--------|--------|
| Transcript read | Single `SELECT … WHERE session_id = ? ORDER BY seq` per snapshot; composite index `(session_id, seq)` on `oxpilot_messages`; bounded by per-session message count, never a scan of other tenants' rows | NFR3.2, NFR3.8 |
| Session list read | Single indexed read on `(user_id, last_activity_at DESC)` on `oxpilot_sessions`; no pagination at v1 scale (unbounded count, tiny rows) | NFR3.4 |
| Status/create | `session_create` = one repo-resolve read + one session insert. `oxpilot.status` = in-process env + provider-slot resolution plus one keyed probe read on `oxpilot_settings` per call when that table exists (absent row/table ⇒ env fallback); either way well under 150 ms | NFR3.5 |

## Stream-path design

| pattern | design | covers |
|---------|--------|--------|
| First-frame latency | The turn task is spawned at claim; the first `append`/snapshot flows through the broadcast channel + mpsc writer already proven by `stream_job_log`. In-process stub ⇒ measured bound is our pipeline: < 250 ms | NFR3.1 |
| Mid-turn attach | `emitted_deltas` is a per-message buffer retaining **every** delta of the in-flight message — bounded by the message's own length (delta count ≤ content bytes ≤ 64 KiB), never a fixed-256 cap, so a mid-turn attach replays from delta 1 with no watermark gap regardless of row-flush lag. Replay is O(deltas), never O(transcript); the 256 capacity applies only to the live broadcast channel, where a `Lagged` reader takes a snapshot resync | NFR3.7, NFR3.8, NFR4.4 |
| Keep-alive | `Sse::keep_alive` interval pinned at exactly 30 s — the C2 floor requires a ping **≥ every 30 s** of stream silence, so the interval must not exceed it; cheap timer per reader, no shared scheduler | NFR3.3 |
| Memory envelope | Per live turn: one ActiveTurn entry (broadcast channel 256-frame ring, cancellation token, ids) — O(in-flight message). Per reader: one `Receiver` + one SSE writer task — bounded by OS fd limits | NFR3.6, NFR3.7 |

## Concurrency design (bulkhead)

Per-session atomic insert-if-absent claim on the `ActiveTurn` map is the
turn bulkhead (Q2): per-process live turns ≤ live sessions; a second
claim degrades to `409 turn_active` without touching the running turn.
No global semaphore — connection-level bounds (fd limits) are the outer
envelope; the global-cap gap is recorded debt for later units (NFR3.13's
single-replica posture makes a per-process cap moot at v1 scale).

## Client-side responsiveness

Composer echo is local state on the minimal page (no per-keystroke
round-trip); query responses feed through the `oxpilot-queries.ts`
TanStack factory with default staleness — no polling loop for the
transcript (SSE push only), session list refreshes on mutation
invalidation (NFR3.9).

## Explicitly not designed (recorded gaps)

- No transcript pagination/cursor reads — deferred past v1 scale.
- No cache tier — all reads hit the DB (NFR3.17); a cache adds an
  invalidation failure mode with no measured need.
- No performance harness — N=1 smoke ceilings in a stack test per the
  measurement protocol; real SLO tooling is recorded debt.
