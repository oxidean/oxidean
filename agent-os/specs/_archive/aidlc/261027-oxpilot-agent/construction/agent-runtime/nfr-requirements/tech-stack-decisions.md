# Tech Stack Decisions — agent-runtime (U3)

Consumes: `codekb/oxidean-app/technology-stack.md`,
`construction/agent-runtime/functional-design/*`,
`inception/requirements-analysis/requirements.md` (NFR9).

Stack is inherited — this unit adds no new dependencies. Decisions
below are stack-conformance constraints.

## Runtime primitives

| id | requirement | basis |
|----|-------------|-------|
| NFR10.1 | The turn task is a `tokio::spawn`'d future holding `Arc<ActiveTurn>` — cancellation via `CancellationToken` (already in the workspace's tokio-util posture); no new runtime, no thread-per-turn | stack.md, entities |
| NFR10.2 | `admission_lock` is `tokio::sync::Mutex<()>` (async-aware — the hold span crosses `.await` on `drain_queue`), guard-scoped so panic drops it; NOT `std::sync::Mutex` (blocking across await would be wrong) | BR3.4, review R-25 |
| NFR10.3 | `terminal_sent` is an `AtomicBool` CAS inside `ActiveTurn` — the first-wins guarantee needs a primitive cheaper than a mutex held across emit | BR5.x, entities |
| NFR10.4 | `fanout`/`emitted_deltas` use the workspace's existing broadcast/mutex patterns — the stream attach takes `tokio::sync` channels consistent with U1's SSE plumbing | entities, stack.md |

## Crate boundaries

| id | requirement | basis |
|----|-------------|-------|
| NFR10.5 | `AgentRuntime` lives in `oxidean-api-oxpilot` (owns the SSE routes and the proc impls); `ActiveTurn`/`ActiveTurns`/`RpcCtx` slots stay in `oxidean-api-core` per U1's shipped layout — no crate move in this unit | entities, C4 |
| NFR10.6 | Provider types (`LlmProvider`, `ChatRequest`, `ChatStreamItem`, `ChatMsg`) are U4's api-core surface — the runtime consumes the trait object; no provider-sdk dependency crosses into api-oxpilot | C4 |
| NFR10.7 | Tool execution is the `ToolExecutor`/`ToolCard` seam (C6) PROVIDED by U3, consumed/implemented by U5 — U3 defines the trait shape in api-core as the pinned interface and ships a no-op/pass-through registry until U5 lands | C6 |

## Testing & coverage

| id | requirement | basis |
|----|-------------|-------|
| NFR10.8 | Unit tests use `cargo-nextest` under the crate's existing suite — mock `LlmProvider` + fake `ToolExecutor` + `tokio::time::pause` for timeout arms; wiremock stays provider-HTTP-layer (U4's harness), not needed inside the loop | NFR9 |
| NFR10.9 | The SSE route coverage extends U1's shipped smokes — `chat_post`/`chat_stream` get a fake-runtime path in the existing test binary, not a new harness | NFR9 |
| NFR10.10 | `oxpilot.turn_cancel` joins the closed-set `PROCEDURES` pin test — collect-only coverage per the proc registry pattern (same as U2's +5) | NFR9, C1 |
