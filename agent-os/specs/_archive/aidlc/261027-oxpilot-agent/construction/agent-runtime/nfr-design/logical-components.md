# Logical Components — agent-runtime (U3)

Consumes: `functional-design/{entities,rules,functional-spec}.md`,
`nfr-requirements/*`, `nfr-design-questions.md` (Q1–Q8),
`codekb/oxidean-app/technology-stack.md`.

## Component catalogue

| component | crate | kind | owns |
|-----------|-------|------|------|
| `ActiveTurn` extension | `oxidean-api-core::oxpilot` | type | gains `admission_lock: Arc<tokio::sync::Mutex<()>>` + `terminal_sent: AtomicBool` + `generation: u32` (chain depth — held on the ENTRY so `drain_and_chain` passes `generation+1` into the successor claim; refines the fd's TurnChain modelling); existing fields (`turn_id`, `pending_client_msg_id`, `pending_content: String`, `started_seq`, `cancel`, `emitted_deltas`, `fanout`) retained |
| `ActiveTurns` | `oxidean-api-core::oxpilot` | map | `claim`, `release(session_id, turn_id)` (the shipped turn_id-paired release — a stale task can't evict a successor's claim), `get` + NEW `admission_lock(session_id) -> Option<(Arc<ActiveTurn>, Arc<Mutex<()>>)>` (pair-return so a stale guard re-verifies liveness under the lock — R-13c). Cancellation is `ActiveTurn.cancel` (field), reached via `get` — the map gains no `cancel` method. `spawn_turn_task` wrapper owns release on every exit shape via inner `JoinHandle` await |
| `AgentRuntime` helpers | `oxidean-api-oxpilot::routes` | module | `assemble_prompt`, `run_turn` → `run_round`, `drain_and_chain` module-private; `emit_terminal`/`emit_frame` are `pub(crate)` — `oxpilot.turn_cancel` (dispatch) and the C8 abort hook call `emit_terminal` through the same first-wins CAS |
| `trait ToolExecutor` + `ToolCtx`/`ToolCall`/`ToolOutput`/`ToolSpec`/`ToolCard` | `oxidean-api-core::oxpilot` | trait port | C6 PROVIDED by U3: `catalog() -> Vec<ToolSpec>`; `invoke(call: ToolCall, ctx: ToolCtx) -> Result<ToolOutput, ToolError>`; `ToolSpec { name, description, parameters_json }` (the pinned three-field provider view — tool class lives registry-side in U5, NOT on the spec); `ToolOutput { content, output_summary }`; `ToolCtx { session_id, user_id, repo_id, proposal_id: Option<..> }`; ships `NullTools` (empty catalog, invoke ⇒ `ToolError{code:"no_tools"}`) until U5 |
| `trait ToolExecutor` slot field | `oxidean-api-core::oxpilot` | slot field | `OxpilotRuntimeSlot` gains `tools: Option<Arc<dyn ToolExecutor>>` (fd-pinned name + nullability — `None` maps to the `NullTools` Arc at `turn_parts()` time so `TurnParts.executor` stays non-Option); `turn_parts()` returns `{provider, model, executor}` under ONE lock read (widened `provider_for_turn`) |
| `oxpilot.turn_cancel` proc | `oxidean-api-oxpilot::dispatch` | rpc boundary | C1-pinned `{session_id} -> {aborted: bool}`; SessionOnly + `require_enabled` + owned session; live turn ⇒ `emit_terminal{aborted}` + `cancel.cancel()` ⇒ `{aborted:true}`; CAS-loss ⇒ `{aborted:false}`; absent ⇒ `no_active_turn` |
| `chat_post`/`chat_stream` (existing) | `oxidean-api-oxpilot::routes` | sse/rpc | unchanged signatures; internals gain the pinned admission order (BR1.1) and residual drain (BR4.2) — the post-release recheck lives in `drain_and_chain` (BR4.1), NOT the route |
| `message_queue` tail | `oxidean-api-oxpilot::session` | proc | enqueue-commit moves INSIDE `admission_lock` hold (BR3.4); queued-card push stays post-commit |

## Dependency shape

```
apps/web ──SSE/JSON──▶ api-oxpilot::routes ──▶ AgentRuntime helpers
                           │                       │
                           │                       ├─▶ svc (dyn SessionService) — reads/drain/cards
                           │                       ├─▶ slot.turn_parts() — provider Arc + model + executor
                           │                       └─▶ turns() — claim/release/lock/cancel
                           ▼
                    api-core::oxpilot::{ActiveTurns, ToolExecutor,
                                        ChatRequest, ChatStreamItem}
                           ▲
                    impl'd: api-oxpilot/providers/* (U4)
                    consumed: api-oxpilot/tools/* (U5, behind trait)
```

One-way edges: `api-oxpilot → api-core → oxidean-core`,
`api-oxpilot → oxidean-db` (through the service trait only). U5's
executor impl lands behind `Arc<dyn ToolExecutor>` — no U3→U5 type
dependency.

## Ownership notes

- `ActiveTurns` owns claim lifecycle + the admission lock; the turn
  TASK never calls `release` — the wrapper owns it unconditionally
  (NFR4.2).
- `turn_parts()` is the ONLY provider/executor read at turn start;
  no other code path touches the registry mid-turn (NFR-security
  pinning).
- `message_queue` owns the enqueue commit; the runtime owns the
  recheck — the SHARED lock makes the pair atomic without either
  owning the other's section.
