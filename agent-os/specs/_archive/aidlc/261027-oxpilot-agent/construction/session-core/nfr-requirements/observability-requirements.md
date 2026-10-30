# Observability Requirements — session-core (U2)

Consumes: `construction/session-core/functional-design/rules.md`,
`inception/requirements-analysis/requirements.md` (NFR5, NFR6),
`codekb/oxidean-app/technology-stack.md`.

NFR6 forbids a new observability stack — this unit rides the existing
tracing/logging/audit surfaces.

## Spans & logs

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | Service calls carry tracing span attributes with resource IDS ONLY — `session_id`, `queue_seq`, `call_index`, drain counts. `user_id` is NOT a span attr (the shipped turn span omits it; the lone `user_id` log precedent is the auth bootstrap, not the oxpilot path). NEVER `content`, `diff_body`, `base`, `title`, `goal`, `api_key`, or any body-bearing field | NFR6, BR3.1 |
| NFR6.2 | Terminal failures log the typed error code (`budget_exceeded`, `not_found`, `turn_active`, `oxpilot.disabled`) — never a raw payload; log fields match span-attr rules | NFR6, C1 |
| NFR6.3 | Queue drain emits a debug-level line per drain: `{session_id, drained_count}` — makes exactly-once verifiable in staging logs without content | NFR6, BR4.2 |

## Audit

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | `session_delete` writes an audit row {actor=user_id, action=`oxpilot.session_delete`, fields=[`session_id`], ts} — field NAMES only, per NFR5; no content, no goal text, no title | NFR5, AC1.5.2 |
| NFR5.2 | Budget refusal does NOT audit — `budget_exceeded` is a hot-path typed error, not a security event; `usage_get` already exposes the counter for ops | NFR5, BR6.2 |
| NFR5.3 | Hook failures on delete surface in the audit row as `action=oxpilot.session_delete` + `result=hook_failed` (field-name detail, never the hook's payload) | NFR5, C8 |

## Counters as telemetry

| id | requirement | basis |
|----|-------------|-------|
| NFR6.4 | `oxpilot_usage_counters` doubles as the spend telemetry — rows keyed `(scope, scope_key, window)` expose requests+tokens per scope per UTC day; no separate metrics pipeline needed for v1 | BR6.1, NFR6 |
| NFR6.5 | `oxpilot.usage_get` returns the C1-pinned shape `{period, tokens_used, tokens_cap, requests_used, requests_cap}` — `period` is the UTC day window, `*_cap` fields are null when unbounded (BR6.3), and `requests_cap`/`tokens_cap` both mirror the one configured V (BR6.2). The counter ROW's `(scope, scope_key, window)` key is storage-internal, never the payload | BR6.2, BR6.3, C1 |

## Provider/runtime diagnosis (consumed, not owned)

| id | requirement | basis |
|----|-------------|-------|
| NFR6.6 | This unit adds NO new health/status proc — `oxpilot.status` (U1) already reports enablement; U2's contribution is counters (NFR6.4) and typed errors (NFR6.2) | NFR6, BR1.3 |
