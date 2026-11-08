# Observability Design — u1-skeleton-slice

Consumes: `nfr-requirements/observability-requirements.md`
(NFR5.1–5.3, NFR6.1–6.5), `functional-design/entities.md`.
Questions: `nfr-design-questions.md` Q3 (conductor-decided).

## Tracing design (Q3)

Existing `tracing` only — no new exporter, metrics registry, or
dashboard stack (NFR6.1).

| signal | shape | covers |
|--------|-------|--------|
| Turn span | one `tracing` span per claimed turn carrying `session_id`, `turn_id`, `message_id`, `repo_id`, provider id/reason — entered on the spawned task, closed at terminal | NFR6.2 |
| SSE reader span | per-attach span on GET/POST stream handlers; records snapshot emit, buffered-replay count, `Lagged`→resync, detach | NFR6.2 |
| Gate-denial event | `oxpilot`-targeted event with proc/route + `reason` — never env values | NFR6.2 |
| Claim/409 event | turn claim + `turn_active` denial as distinct events | NFR6.2 |
| Terminal event | `done`/`error` + code + durations (turn_ms, frames) | NFR6.2 |
| Orphan sweep | startup event carrying swept-row count | NFR6.2 |
| Session create | create event (metadata only); delete is U2 | NFR6.2 |

All log content is metadata only — ids, codes, counts; no message
content and no credential-shaped value ever enters a log line (NFR6.3).
Correlation is by `session_id`/`turn_id`/`message_id` span fields —
distributed trace propagation is not designed for U1's single process
surface (no traceparent handling; the SSE fetch carries cookies only).

## Audit design

| event | payload | behaviour | covers |
|-------|---------|-----------|--------|
| `oxpilot_session.create` | `{session_id, repo_owner, repo_name, branch}` | fire-and-forget `audit_events` row via existing `audit.rs`; audit failure never fails the operation | NFR5.1, NFR5.2 |
| Cross-user `not_found` denial | security-relevant audit (no content) | optional fire-and-forget, matching existing convention | NFR5.3 |

Session delete carries no audit in U1 — the delete proc is U2 (NFR5.1).

## SLI/SLO posture

No SLI registry or alert rules are designed — NFR6 forbids new
observability infra. The stub round-trip is self-verifying in a stack
test (POST → frames → terminal) as the functional probe (NFR6.4), and
`oxpilot.status` is the operator diagnostic surface answering "why is
it off" without log access (NFR6.5). The smoke-level latency ceilings
(NFR3.1–3.5) are asserted in the same stack test, not a monitoring
pipeline.
