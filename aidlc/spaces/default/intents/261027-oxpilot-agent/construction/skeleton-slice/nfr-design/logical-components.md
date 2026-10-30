# Logical Components — u1-skeleton-slice

Consumes: `functional-design/` (all), `nfr-requirements/` (all),
`inception/contract-design/contract-summary.md`,
`inception/domain-design/components.md`.
Questions: `nfr-design-questions.md` Q2, Q4, Q6 (conductor-decided).

Component-level view of where the NFR patterns land — this bridges NFR
design to Infrastructure Design (next stage).

## Inventory

| component | layer | owns | failure domain / blast radius |
|-----------|-------|------|-------------------------------|
| `oxidean-api-oxpilot` crate | RPC | `PROCEDURES` {status, session_create, session_list}, dispatch arm, SessionOnly classification | proc failure = typed error to one caller; no shared state |
| `EnablementGuard` | shared helper (in oxpilot crate) | flag+provider resolution, precedence `flag_off > misconfigured > no_provider` | misread config = fail-closed deny on gated surfaces; `status` stays reachable |
| `ActiveTurn` map | `RpcCtx` runtime slot | per-session turn claim, fanout broadcast, delta ring (256), cancellation token | per-process only; a wedged entry degrades exactly one session; lost on restart (orphan sweep covers rows) |
| Turn task | spawned per accepted POST | provider call, delta emit, row finalize, cancel handling | one session's turn — `Result` errors terminate that turn only; a panic is caught by the drop-guard/supervised `JoinHandle`, which removes the entry and lands `status=error`, so the session never pins and the row never stays `streaming` (Q6) |
| SSE handlers ×2 | `/api/` routes | auth→gate→ownership, snapshot emit, replay, keep-alive, detach | reader failure kills one connection; Lagged → snapshot resync |
| `StubProvider` | `LlmProvider` slot on `RpcCtx` | deterministic deltas, failure-injection flag | in-process; no external blast radius |
| `redact()` | `oxidean-core` | pattern-table secret detector on write | false-negative = content stored raw (accepted residual risk — the CONFIGURATION.md disclosure copy states the detector covers only known secret shapes); false-positive = over-redacted transcript |
| `oxidean-db` tables ×2 | persistence | `oxpilot_sessions`, `oxpilot_messages` + indexes | shared DB; dialect parity via `dialect_oxpilot.rs` |
| Web route bundle | `apps/web` | `/oxpilot` page, `oxpilot-queries.ts`, SSE fetch client | client-side only; flag-off page is explanatory, never dead — baseline semantics (`main` landmark, ordered headings, labelled controls) + flag-off copy naming `OXIDEAN_OXPILOT_ENABLED`, the provider env vars, and `docs/CONFIGURATION.md` (NFR7.1, NFR8.1) |

## Isolation strategy

Per-session is the unit of isolation everywhere: the ActiveTurn claim
key, the transcript queries' `WHERE session_id`, the audit payload's
`session_id`. No cross-session mutable state exists; the only process-
wide mutable structure is the ActiveTurn map itself, whose entries are
independent. Shared resources: the DB pool (existing), `RpcCtx` map
lock (claim-critical section is insert-if-absent, held for nanoseconds),
and the broadcast channel per live turn (capacity 256).

## Shared-resource contention map

| resource | contenders | contention control |
|----------|-----------|--------------------|
| ActiveTurn map | claiming POST, releasing turn task, readers attaching | insert-if-absent claim; short lock or DashMap shard |
| broadcast ring | turn producer + N readers | bounded 256; Lagged → resync |
| DB pool | all procs + SSE snapshot reads | existing sqlx pool; one-shot indexed queries |
