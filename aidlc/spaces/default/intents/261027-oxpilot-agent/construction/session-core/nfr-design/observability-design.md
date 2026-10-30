# Observability Design — session-core (U2)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR5.1–5.3, NFR6.1–6.6), `nfr-design-questions.md`.

Rides the existing tracing + audit stack — no new observability
machinery (NFR6).

## Span attribute policy

| surface | allowed attrs | forbidden |
|---------|---------------|-----------|
| service spans (`oxpilot.session.*`) | `session_id`, `queue_seq`, `call_index`, `drained_count`, `err.code` | `user_id` (shipped turn span omits it), `content`, `diff_body`, `base`, `title`, `goal`, `api_key` |
| proc spans | proc name, `err.code` | any body field |

## Log lines

| event | level | fields |
|-------|-------|--------|
| drain completed | debug | `{session_id, drained_count}` |
| hook abort on delete | warn | `{session_id, hook_index, err.code}` |
| budget refusal | debug | `{scope}` — counter values logged by `usage_get`, not per refusal |
| enqueue dedupe hit | debug | `{session_id, arm: transcript|queue}` |

## Audit rows

| event | action | fields (names only) | result values |
|-------|--------|--------------------|---------------|
| session deleted | `oxpilot.session_delete` | `session_id` | `detail` JSON carries the outcome — `{"result":"ok"}` / `{"result":"hook_failed","hook_index":i}` / `{"result":"not_found"}` (no dedicated result column on `insert_audit_event`; the index rides `AppError.data` from the service, NFR5.3/R-20) |

No audit on `budget_exceeded` (hot-path typed error, NFR5.2), none on
reads (NFR5's auditability targets the destructive mutation).

## Counters as telemetry

`oxpilot.usage_get` returns `{period, tokens_used, tokens_cap,
requests_used, requests_cap}` (C1-pinned; null caps = unbounded) —
the ops surface for spend is the proc, not a metrics export (NFR6.4,
NFR6.5).
