# Scalability Design — u1-skeleton-slice

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR3.10–3.18), `functional-design/entities.md` (ActiveTurn),
`tech-stack-decisions.md`.
Questions: `nfr-design-questions.md` Q2 (conductor-decided).

## Deployment posture

Single-replica is the v1 target — `ActiveTurn` is a **per-process** map
by design (NFR3.13). The constraint is disclosed in
`docs/CONFIGURATION.md` beside the env block. All persisted state lives
in `oxidean-db` (sqlx × 3 dialects); no new state store (NFR3.14).

## Load-distribution design

| axis | bound | mechanism | covers |
|------|-------|-----------|--------|
| Sessions per user | unbounded count, tiny rows | `(user_id, last_activity_at DESC)` index keeps list reads index-ordered | NFR3.10 |
| Messages per session | per-session growth | `(session_id, seq)` index; reads stay O(own rows) | NFR3.11 |
| Live turns | ≤ 1 per session ⇒ per-process turns ≤ live sessions | ActiveTurn map key IS the bound | NFR3.12 |
| In-memory turn state | O(in-flight message) | bounded delta ring (capacity 256) + broadcast channel + cancellation token | NFR3.12, NFR3.7 |
| SSE readers per session | OS/fd-limited (no configured cap — bare `axum::serve`; accepted at team scale) | broadcast `send` stores one copy in the ring; per-reader clone cost at `recv()` — O(readers) per frame, acceptable under the Lagged-resync policy | NFR3.15 |
| POST body | ≤ 64 KiB ⇒ `422 invalid_body` | closed resource envelope: tiny unbounded rows + fd-bound readers + bounded bodies | NFR3.18 |

## Data-partitioning design

Per-session row-keyed partitioning is inherent: every query is
`WHERE session_id`/`WHERE user_id` scoped — no cross-partition reads
exist in U1's surface (NFR3.10, NFR3.11). Transcript growth becomes
user-controlled via the U2 delete proc; U1's growth bound is the body
cap × one-turn-per-session (NFR3.16). No caching layer — direct DB reads
(NFR3.17).

## Scale-out path (deferred)

Multi-replica would need a shared turn-claim substrate (e.g. advisory
lock or claim row) — explicitly rejected for v1 as new infrastructure.
The ActiveTurn seam is already an interface boundary: a claim-row
implementation can replace the in-memory map without touching proc
semantics (recorded for U2+).
