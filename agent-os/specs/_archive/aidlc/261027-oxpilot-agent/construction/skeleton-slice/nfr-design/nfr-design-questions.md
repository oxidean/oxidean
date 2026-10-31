# NFR Design — Questions (skeleton-slice)

Decisions for this stage. All resolved under the standing self-drive instruction; ratify at PR review.

## Q1 — Resilience pattern for the provider call
The skeleton has exactly one external dependency class: the provider call inside a turn task. Which resilience pattern?
[Answer]: A
- A. Fail-fast, no retry — a provider error terminates the turn with a terminal error frame + `status=error` assistant row. No retry, no circuit breaker (single stub provider; retry semantics belong to U4's real adapters).
- B. Bounded retry with backoff in U1.
- C. Circuit breaker around the provider.

## Q2 — Concurrency/bulkhead pattern for turns
How is turn concurrency contained?
[Answer]: B
- A. Global semaphore bounding concurrent turns.
- B. Per-session atomic claim (one active turn per session) as the bulkhead; reader fanout bounded by broadcast capacity 256 and OS fd limits. No global turn cap in U1 (single-replica, smoke-level scale); record as debt for later units.
- C. Unbounded turns.

## Q3 — Observability: tracing shape
Which tracing design for the turn lifecycle?
[Answer]: A
- A. Reuse existing `tracing` spans: one span per turn carrying `session_id`, `turn_id`, `message_id`, `repo_id`, provider reason; SSE reader spans for snapshot/resync/lag events. No new exporter, no metrics registry (NFR5 decision stands).
- B. Add a metrics registry + counters in U1.
- C. Unstructured `log` calls only.

## Q4 — Security architecture: gate + authz ordering
The C2 contract pins auth → gate → ownership for all surfaces. What implements it?
[Answer]: A
- A. Per-surface, not middleware-stacked: each `oxpilot.*` RPC resolves enablement inside the procedure via a shared `EnablementGuard` helper (reason precedence `flag_off > misconfigured > no_provider`); each SSE handler runs session auth → the same helper → session-ownership check (anti-enumerating `not_found`) in that order. `oxpilot.status` is gate-exempt by design and calls the helper to *report* state, not to deny.
- B. An Axum middleware layer for all oxpilot surfaces.
- C. Per-handler ad-hoc checks without a shared helper.

## Q5 — Persistence/performance design for transcripts
How are transcript reads shaped for the GET stream snapshot?
[Answer]: A
- A. Single `SELECT ... WHERE session_id ORDER BY seq` per snapshot (messages are small, per-session bounded by UX); index `(session_id, seq)`; no caching tier, no pagination in U1. Delta ring buffer (capacity 256) serves mid-turn replay without DB reads.
- B. Add a transcript cache layer.
- C. Cursor-paginate transcript reads in U1.

## Q6 — Failure-domain isolation (blast radius)
What does a turn task failure/wedge affect?
[Answer]: B
- A. Turns run on the request task — failure kills the connection.
- B. Each turn runs on its own spawned tokio task (detach-to-complete). A panicking/stuck turn degrades exactly one session's active turn; orphan sweep on startup reaps `streaming` rows to `error`. Broadcast lag degrades one reader to a snapshot resync. No shared mutable turn state beyond the per-session ActiveTurn entry.
- C. A global turn executor shared by all sessions.

## Consolidated Summary Confirmation

Decisions consolidated:
- Q1 A: fail-fast provider call; error terminates the turn (terminal error frame + status=error row). No retry/breaker in U1.
- Q2 B: per-session atomic ActiveTurn claim as the bulkhead; broadcast 256 + OS fds bound readers; global turn cap deferred.
- Q3 A: existing `tracing` spans only (turn span + SSE reader spans); no new metrics/exporter.
- Q4 A: shared `EnablementGuard` helper per surface, ordering auth -> gate -> ownership; `oxpilot.status` gate-exempt.
- Q5 A: indexed `(session_id, seq)` SELECT per snapshot; ring buffer for mid-turn replay; no cache/pagination in U1.
- Q6 B: one spawned task per turn (detach-to-complete); blast radius is one session's turn; orphan sweep + lag resync.

[Answer]: Looks correct
