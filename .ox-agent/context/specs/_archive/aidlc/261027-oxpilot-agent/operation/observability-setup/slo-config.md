# SLO Config — oxpilot

## Sources

Consumes per-unit `nfr-design/performance-design.md` (**performance-design**), `nfr-design/security-design.md` (**security-design**), `nfr-design/reliability-design.md` (**reliability-design**), `infrastructure-design/monitoring-design.md` (**monitoring-design**), and `infrastructure-design/infrastructure-specification.md` (**infrastructure-specification**).
## Defined SLOs (from NFR3.x, session-core)

| SLO | Target | Measurement owner |
|---|---|---|
| First SSE frame after `chat_post`/subscribe | < 250 ms p99 | `performance-validation` stage (post-deploy) |
| `session_list` proc | < 150 ms p99 | `performance-validation` stage (post-deploy) |

## Resource bounds (design limits, verified by tests)

- Prompt transcript: newest-first, ≤ 128 KiB assembled window.
- Tool-call summaries: ≤ 2 KiB each.
- Per-tool timeout 60 s; max 8 tool rounds; max turn duration 10 min.

## Error budget note

The feature is flag-gated; SLO enforcement begins only in flag-on environments. Flag-off traffic produces cheap denials and is excluded from SLO math.
