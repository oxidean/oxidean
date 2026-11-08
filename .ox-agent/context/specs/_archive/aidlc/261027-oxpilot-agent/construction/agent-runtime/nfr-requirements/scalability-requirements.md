# Scalability Requirements — agent-runtime (U3)

Consumes: `construction/agent-runtime/functional-design/rules.md`
(BR3.x, BR4.x), `functional-design/entities.md`,
`inception/requirements-analysis/requirements.md` (NFR3, FR6.7).

v1 is single-process. These rows bound where the design holds as
replica count and turn volume grow, and pin the documented limits.

## Single-process limits

| id | requirement | basis |
|----|-------------|-------|
| NFR11.1 | `ActiveTurns` is process-local (DashMap-style) — two replicas could each claim the same session's turn; v1 documents single-replica for oxpilot chat, same caveat as U2's budget limiter (FR6.7) | FR6.7, entities ActiveTurns |
| NFR11.2 | `admission_lock` is per-session and lives inside the same map entry — multi-replica loses the atomicity story entirely, covered by the same single-replica caveat (no second mechanism invented for v1) | BR3.4, FR6.7 |
| NFR11.3 | `emitted_deltas`/`fanout` are in-memory — a replica crash drops live attachments; reconnect is snapshot-based per C2 so clients recover via snapshot+attach, not replay recovery | C2, entities |

## Growth shape

| id | requirement | basis |
|----|-------------|-------|
| NFR11.4 | Per-session serialization is the ONLY ordering the loop imposes — turns across different sessions run fully parallel; throughput scales with provider concurrency, not a shared runtime lock | BR3.4, BR4.1 |
| NFR11.5 | `PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB` keeps per-turn request size flat as sessions age — provider spend and assembly time are O(1) in session length, not O(transcript) | BR2.x |
| NFR11.6 | Subscriber count per turn is user-driven (one UI attach + reconnects); fanout is O(subscribers) per delta and attachment churn costs a buffer replay at attach | entities |
