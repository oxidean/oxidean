# Observability Design — agent-runtime (U3)

Consumes: `nfr-requirements/observability-requirements.md` (NFR5.x,
NFR6.x), `functional-design/rules.md` (BR7.1), `nfr-design-questions.md`
(Q7).

## Span + line table (Q7)

```rust
let span = tracing::info_span!("oxpilot.turn",
    session_id = %session_id, turn_id = %turn.turn_id,
    provider_id = provider.id(), model = ?model);
// .instrument() wraps run_turn; all lines inherit the fields.
```

| event | level | fields (ids only) |
|-------|-------|-------------------|
| turn start | `info!` | `{session_id, turn_id, provider_id, model}` (BR7.1) |
| tool call | `debug!` | `{session_id, tool, call_index, status}` |
| drain | `debug!` | `{session_id, drained_count, generation}` |
| abort | `warn!` | `{session_id}` |
| budget refusal | `debug!` | `{scope}` |
| terminal | `debug!` | `{session_id, result, code?}` |
| degraded arms | `warn!` | recheck-drain `Err`, claim-loss, panic-release, `card_persist` — `{session_id, generation}` + typed reason |

Never: prompt text, deltas, tool args/result bodies, `api_key`, goal,
title (NFR6.1, NFR5.3).

## Audit posture

- `turn_cancel` → NO audit row — routine user control; the `aborted`
  terminal row is the durable record (NFR5.1).
- Tool invocations → the `oxpilot_tool_calls` rows ARE the audit
  trail (status + summaries durable); write-tool side effects are
  U5's concern (NFR5.2, C6 split).
- Delete-driven abort → U2's `session_delete` audit row covers the
  event; the runtime adds no second row.

## Operator story

A wedged provider stream shows as turn-start `info` with no terminal
`debug` line + continued `ping` frames — distinguishable from a dead
task (panic ⇒ `warn` join-error line). Claim-loss churn shows as
`warn` claim-loss lines — the signal for lock-starvation bugs.
