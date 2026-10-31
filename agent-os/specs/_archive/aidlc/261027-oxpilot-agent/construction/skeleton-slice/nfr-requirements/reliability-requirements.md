# Reliability Requirements — u1-skeleton-slice

Consumes: `construction/skeleton-slice/functional-design/functional-spec.md`
(WF3, WF4, state machines), `functional-design/rules.md`
(BR3.4, BR4.3–BR4.6), `entities.md` (ActiveTurn),
`inception/requirements-analysis/requirements.md` (NFR4),
`inception/contract-design/contract-summary.md` (C2).

## Failure behaviour

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | Provider/stream failure surfaces as `terminal{result:error,code}` in-stream plus assistant row `status=error` — never a silent stall and never a frame after terminal | NFR4, BR4.3 |
| NFR4.2 | A torn-down stream never pins the session: the turn task owns the row to terminal status (detach-to-complete), and transport close detaches readers only | BR4.6, WF3.6 |
| NFR4.3 | Process-death orphan: any `oxpilot_messages.status='streaming'` row with no owning task is swept to `error` at startup — the transcript never shows a phantom-live message after restart | BR4.6, Q6 |
| NFR4.4 | Resume is exactly-once: snapshot-replace + buffered replay + `(message_id, seq)` dedupe; a reconnect never duplicates or loses accepted content. Slow-reader policy: the broadcast channel is bounded (256 frames); a `Lagged` reader is resent a fresh `snapshot` (self-healing resync) — the buffer is data, not wake hints | C2, BR4.4, NFR3, R-05 finding |

## Degradation

| id | requirement | basis |
|----|-------------|-------|
| NFR4.5 | Feature-disabled is a clean degradation, not an outage: `oxpilot.status` answers `{enabled:false, reason}`, every gated surface returns the typed `oxpilot.disabled`, and the page renders the explanatory state | BR2.3, BR5.4, NFR8 |
| NFR4.6 | A racing second POST degrades to `409 turn_active` — explicit, typed, and leaves the live turn untouched | BR4.2, C2 |

## Data durability

| id | requirement | basis |
|----|-------------|-------|
| NFR4.7 | User messages persist before the turn claim's provider work begins — an accepted message is never lost to a provider failure | BR3.4, WF3.2 |
| NFR4.8 | Partial assistant output is durable: the row accumulates content and lands `complete`/`error` — partial content renders as such on resume, never silently truncated | BR3.4 |
| NFR4.9 | Session delete + its ordered hook chain (U3 abort → U6 revoke) are U2 scope — C8's owner column is session-core; U1 carries no delete proc and registers no hook seam (recording the boundary, not colliding with U2's enforcement point) | C8, unit-of-work §U2, R-02 finding |

## Testability hooks

| id | requirement | basis |
|----|-------------|-------|
| NFR4.10 | The stub provider exposes a test-only failure injection (env or constructor flag) so the error-terminal path is exercised deterministically — `terminal{error}` is tested, not just `terminal{done}` | Q6, NFR9 |
