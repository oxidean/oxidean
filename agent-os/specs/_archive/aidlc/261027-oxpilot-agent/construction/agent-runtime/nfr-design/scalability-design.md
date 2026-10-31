# Scalability Design — agent-runtime (U3)

Consumes: `nfr-requirements/scalability-requirements.md` (NFR11.x),
`functional-design/*`, `nfr-design-questions.md` (Q1).

## Single-replica boundary

| resource | scope | multi-replica story |
|----------|-------|---------------------|
| `ActiveTurns` map | process-local | two replicas could double-claim — v1 documents single-replica oxpilot chat (same caveat as U2 budget limiter, FR6.7) |
| `admission_lock` | per-session, inside map entry | loses atomicity across replicas — same documented caveat |
| `emitted_deltas`/`fanout` | process-local | crash drops attachments; C2 reconnect is snapshot-based — recovery is the snapshot, not replay recovery |
| queued rows | DB-durable | survive everything; the durable floor |

## Parallelism shape

- Turns across DIFFERENT sessions run fully parallel — no shared
  runtime lock beyond per-entry `admission_lock` holds measured in
  single-digit-ms DB ops (NFR11.4).
- `PROMPT_TRANSCRIPT_MAX_BYTES` keeps per-turn assembly + provider
  spend flat in session length (NFR11.5).
- Fanout is O(subscribers) per frame; attach costs one buffer replay
  (NFR11.6).

## Growth-sensitive choices

- `generation` is a u32 monotonic chain counter — `MAX_CHAIN_DEPTH`
  bounds it; a session can never monopolize the executor slot beyond
  16 auto-turns without a fresh POST (BR4.3).
- No polling loops in the runtime — all waits are `select!` on
  provider stream / cancel / interval / timeout; idle turns cost
  zero CPU.
