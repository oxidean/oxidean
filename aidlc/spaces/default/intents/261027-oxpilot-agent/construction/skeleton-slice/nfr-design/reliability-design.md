# Reliability Design — u1-skeleton-slice

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.10), `functional-design/functional-spec.md` (WF3–WF5),
`functional-design/entities.md` (ActiveTurn).
Questions: `nfr-design-questions.md` Q1, Q6 (conductor-decided).

## Failure-mode design

| failure | designed behaviour | covers |
|---------|--------------------|--------|
| Provider/stream error | Turn task catches it, writes assistant row `status=error`, emits `terminal{result:error,code}` — never a silent stall, never a frame after terminal | NFR4.1 |
| Client disconnect mid-turn | Transport close detaches readers only; the spawned turn task owns the row to a terminal status (detach-to-complete) | NFR4.2 |
| Turn-task panic mid-run | Finalization is guarded, not end-of-task code: a drop-guard/supervised `JoinHandle` observes `JoinError` and both removes the ActiveTurn entry (unpinning the session — no stuck `409 turn_active`) and lands the row's terminal `status=error`; the in-process variant of the phantom-live invariant is covered, not just the restart one | NFR4.1, NFR4.3 |
| Process death mid-turn | Startup orphan sweep runs **before the process serves traffic** (synchronous boot step, before listener accept): `oxpilot_messages.status='streaming'` rows with no owning task → `error`; no window where a snapshot can read a phantom-live row | NFR4.3 |
| Slow reader | Broadcast `Lagged` ⇒ reader gets a fresh `snapshot` (self-healing resync); buffer is data (capacity 256), not wake hints | NFR4.4 |
| Flag off | Clean degradation: `oxpilot.status` answers `{enabled:false, reason}`; gated surfaces return `oxpilot.disabled`; page renders the explanatory state | NFR4.5 |
| Racing second POST | `409 turn_active` — typed, explicit, live turn untouched | NFR4.6 |

## Resilience pattern decisions (Q1)

**No retry, no circuit breaker in U1.** The only external-dependency
class is the provider call inside a turn task, and the stub is
in-process — a retry loop would test nothing real. Provider errors
terminate the turn (error frame + `status=error` row); retry policy and
breakers belong to U4's real adapters where remote calls exist. The
**bulkhead** is the per-session atomic claim: a wedge in one session's
turn degrades exactly that session (Q6 — one spawned task per turn, no
shared executor).

## Durability ordering

1. User message row persists **before** the provider call begins —
   an accepted message is never lost to a provider failure (NFR4.7).
2. Assistant row accumulates content and lands `complete`/`error` —
   partial output is durable and renders as partial on resume
   (NFR4.8).
3. Resume is exactly-once: snapshot-replace + buffered replay +
   `(message_id, seq)` dedupe (NFR4.4).

## Boundary recording (not design)

Session delete + ordered hook chain (U3 abort → U6 revoke) is U2 scope
— U1 ships no delete proc and registers no hook seam (NFR4.9).

## Health & test hooks

- `oxpilot.status` doubles as the operator health surface
  (`{enabled, provider, reason}`) — the "why is it off" answer without
  log access (NFR6.5).
- `StubProvider` carries a test-only failure-injection flag via
  **constructor option only** — deliberately not an env var, which would
  drag a new `OXIDEAN_OXPILOT_*` name under BR1.5's `.env.example` +
  `docs/CONFIGURATION.md` parity obligation for a test hook
  (NFR4.10).
- Failover/replication: none at v1 — single-replica posture (NFR3.13);
  the durable transcript is the recovery substrate.
