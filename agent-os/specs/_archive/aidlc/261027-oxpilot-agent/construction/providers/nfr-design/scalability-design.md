# Scalability Design — providers (U4)

Consumes: `nfr-requirements/scalability-requirements.md` (NFR3.9–3.15).

## Data shape (NFR3.9–3.11)

```sql
-- oxpilot_settings: singleton, id = 1, all-nullable sparse override
-- reads: SELECT * FROM oxpilot_settings WHERE id = 1  -- inside rebuilds only
```

No growth: one row ever; registry is a static table; budget fields are
two integers on the same row (counters are U2's tables).

## Concurrency (NFR3.12–3.14)

- Egress concurrency inherits U1's turn bound — no adapter-side
  throttle; the turn map is the concurrency contract.
- `RwLock` reads don't queue behind the rebuild `Mutex` — they hold
  separate primitives (`rebuild_lock` for the section, `slot` RwLock for
  the swap instant).
- Swap cost is O(1): new `OxpilotRuntimeSlot` built beside the lock,
  write-locked for one assignment; `Arc<ActiveTurns>` clone keeps live
  sessions untouched.

## Multi-replica (NFR3.15)

Each replica owns its `OxpilotRuntime` — a `settings_set` rebuilds the
replica serving it; other replicas converge on their next rebuild/boot.
Documented bound for v1 admin-rare writes: a replica may serve a stale
provider until its next rebuild — acceptable since admin writes are
rare and operator-driven (the operator can hit the health/deploy flow
to force convergence if needed).
