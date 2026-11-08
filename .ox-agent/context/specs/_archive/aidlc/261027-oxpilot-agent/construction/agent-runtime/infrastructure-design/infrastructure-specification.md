# Infrastructure Specification — agent-runtime (U3)

Consumes: `nfr-design/*`, `nfr-requirements/*`,
`functional-design/{rules,entities}.md`,
`inception/domain-design/components.md` (component inventory),
`team.md` (Way of Working).

## Deployable surface

U3 adds **no deployable**. The runtime is in-process tokio work inside
`oxidean-api` (`oxidean-api-oxpilot` routes + helpers). What ships:

| surface | change | infra impact |
|---------|--------|--------------|
| `oxidean-api-oxpilot` | `chat_post`/`chat_stream` internals + `run_turn`/`drain_and_chain`/`emit_terminal`/`emit_frame` helpers | none — same binary, same image |
| `oxidean-api-core::oxpilot` | `ActiveTurn` gains `admission_lock`/`terminal_sent`/`generation`; `ActiveTurns` gains `admission_lock()`; slot gains `tools: Option<Arc<dyn ToolExecutor>>`; `ToolExecutor`/`ToolCtx`/`ToolOutput` traits | none — library types |
| `oxpilot.turn_cancel` proc | new RPC verb on the closed PROCEDURES set | rpc-gen drift check only |
| deps | none — tokio/tokio-util/tracing already in tree | none |

## Environment

No new variables. The runtime consumes U4's existing enablement
(`OXIDEAN_OXPILOT_ENABLED`, provider config envs) and U2's service —
nothing else.

## Restart invariants

- `ActiveTurns` + `admission_lock` are empty at boot by construction
  (process-local).
- Mid-turn `streaming` transcript rows are reaped by the existing
  boot sweep (`sweep_oxpilot_streaming_messages`) — restart clears
  state rather than corrupting it.
- Single-replica caveat documented (NFR11.x) — the in-memory turn map
  + admission lock are only coherent under one process.

## Resource shape

- One `tokio` task per live turn + the spawned inner `run_turn` —
  task count bounded by concurrent sessions, not threads.
- `emitted_deltas` growth bounded implicitly by
  `TURN_MAX_DURATION × throughput` (accepted risk, NFR3.3).
- `PING_INTERVAL = 15 s` per attached stream — cheap heartbeat.
