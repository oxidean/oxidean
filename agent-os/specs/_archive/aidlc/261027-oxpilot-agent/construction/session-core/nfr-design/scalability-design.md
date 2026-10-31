# Scalability Design — session-core (U2)

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR11.1–11.6), `functional-design/entities.md`, `nfr-design-questions.md`.

v1 is single-process. This file pins where the design holds and where
the documented limits sit — no distributed machinery is built.

## Contention map

| resource | contention scope | mechanism |
|----------|-----------------|-----------|
| `seq` mint | one session's writers | SELECT MAX+1 → INSERT under `UNIQUE(session_id,seq)` |
| `queue_seq` mint | one session's enqueue callers | same + bounded retry (8, NFR10.7) |
| `ActiveTurns` claim | process-local map | U3 invariant — one live turn per session; single-replica caveat for cross-replica (NFR11.2) |
| `oxpilot_usage_counters` row | per (scope,key,window) | atomic upsert — DB-serialized increments |
| `check_budget`→act | process-local | non-atomic read/decide — documented over-admit window (NFR4.10) |

## Documented limits (the honest caveats)

1. **Single-replica turn ownership** — two replicas could each mint a
   live turn for one session; chat correctness assumes one replica.
   Same bound FR6.7 sets for the limiter (NFR11.2).
2. **Budget over-admit across replicas** — DB counters are shared, but
   check→act isn't atomic across processes: N replicas can over-admit
   up to N× the remaining budget in the window (NFR11.3).
3. **Unbounded table growth** — no retention job in v1;
   `session_delete` is the only reclamation. Indexes keep the hot
   reads O(session/user scope) so growth degrades gracefully
   (NFR11.5).
4. **Queue depth unbounded** — a pathological client can enqueue
   thousands; read paths are O(queue depth) and the budget gate bounds
   turn consumption, not queue length (NFR11.6).

## Counter shape at scale (NFR11.4)

`(scope, scope_key, window)` — scope `user|instance`; rows/day =
users + 1; UTC-day rollover means yesterday's rows are cold and never
queried again. No trim job: cardinality is bounded by users × days
retained by operator policy, not by this code.
