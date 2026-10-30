# Observability Requirements — agent-runtime (U3)

Consumes: `construction/agent-runtime/functional-design/rules.md`,
`functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR5, NFR6),
`codekb/oxidean-app/technology-stack.md`.

NFR6 forbids a new observability stack — the turn loop rides the
existing tracing/logging surfaces and emits structured fields only.

## Tracing & logs

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | Turn lifecycle lines per the pinned BR7.1 table: `info` turn start `{session_id, turn_id, provider_id, model}`; `debug` tool call `{session_id, tool, call_index, status}`; `debug` drain `{session_id, drained_count, generation}`; `warn` abort `{session_id}`; `debug` budget refusal `{scope}`; terminal `{session_id, result, code?}` at `debug`. NEVER content, prompt text, tool args/results bodies, `api_key`, goal, or title | NFR6, NFR5, BR7.1 |
| NFR6.2 | Terminal emission logs at `debug` per BR7.1 carrying `{session_id, result, code?}` — terminal reason is a stable typed value, never a payload | NFR6, BR7.1 |
| NFR6.3 | `warn!` on the pinned degraded arms beyond BR7.1's table: recheck-drain `Err`, claim-loss exit, panic/JoinError release, `card_persist` feedback — each carries `{session_id, generation}` + the typed reason, no content | BR4.1, BR5.2, BR5.3 |
| NFR6.4 | `debug!` per drain `{session_id, drained_count, generation}` — BR7.1's pinned drain line; queue liveness verifiable in staging logs without content | NFR6, BR7.1 |

## Audit

| id | requirement | basis |
|----|-------------|-------|
| NFR5.1 | `oxpilot.turn_cancel` writes NO audit row — cancelling a turn is a routine user action like `message_queue`, not a security event; the `aborted` terminal in the transcript is the durable record | NFR5 |
| NFR5.2 | Tool-call invocations are NOT individually audited at the runtime layer — the durable `oxpilot_tool_calls` rows (with status + summaries) ARE the audit trail; write-tool side effects are the U5 executor's audit concern | C6, entities OxpilotToolCall |
| NFR5.3 | Provider credential material never appears in logs, spans, or audit — `provider id` literals only (NFR1.5) | NFR1, NFR5 |
