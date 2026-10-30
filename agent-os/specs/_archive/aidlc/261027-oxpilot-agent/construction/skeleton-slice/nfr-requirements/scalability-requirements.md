# Scalability Requirements — u1-skeleton-slice

Consumes: `construction/skeleton-slice/functional-design/entities.md`,
`functional-design/rules.md` (BR3.2, BR4.2, BR4.6),
`inception/requirements-analysis/requirements.md` (NFR3, NFR4),
`codekb/oxidean-app/technology-stack.md`.

## Load shape

| id | requirement | basis |
|----|-------------|-------|
| NFR3.10 | Sessions per user: unbounded count, list is index-ordered `(user_id, last_activity_at DESC)` — pagination not needed at v1 scale but the ordering index must exist | BR3.2 |
| NFR3.11 | Messages per session: index `(session_id, seq)` keeps snapshot + append-order reads O(rows), independent of other sessions | entities |
| NFR3.12 | Live turns: exactly one per session (map key), so per-process turn count ≤ per-process active-session count; no unbounded in-memory state per turn beyond the delta buffer | BR4.2/BR4.6, NFR3.7 |

## Scaling posture

| id | requirement | basis |
|----|-------------|-------|
| NFR3.13 | ActiveTurn is per-process by design — single-replica deployments are the v1 target; the constraint is documented in `docs/CONFIGURATION.md` beside the env block, not hidden | BR4.6 scope note, Q6 |
| NFR3.14 | All persisted state lives in `oxidean-db` — horizontal concerns (connection pool, dialect parity) follow the existing sqlx×3 stack; no new state store | tech-stack, entities |
| NFR3.15 | SSE reader count per session is bounded by OS/fd limits only (no configured connection cap exists — bare `axum::serve`; accepted at team scale); broadcast `send` stores one copy in the ring, clones happen recv-side per `Receiver::recv()` — recv cost is O(readers) per frame, acceptable under the 256-frame buffer + Lagged-resync policy (NFR4.4) | entities ActiveTurn.fanout, R-13/R-14 findings |
| NFR3.18 | POST body bounded: `content` ≤ 64 KiB ⇒ 422 `invalid_body` — together with one-turn-per-session and fd-limit reader bounds, the skeleton's resource envelope is closed (sessions-per-user unbounded but tiny rows; readers bounded by OS limits; bodies bounded) | R-07/R-10 findings |

## Data growth

| id | requirement | basis |
|----|-------------|-------|
| NFR3.16 | Transcript growth is user-controlled once delete exists — the cascade FK is already declared (`on_delete: cascade` in entities); the delete proc itself is U2, so U1's growth bound is the 64 KiB body cap × one-turn-per-session | entities on_delete, R-01 finding |
| NFR3.17 | No caching layer introduced — all reads hit the DB directly; cache invalidation would add a failure mode without a measured need | NFR6 (no new infra) |
