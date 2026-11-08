# Scalability Requirements — oxpilot-tools (U5)

Consumes: functional-design `entities.md` (OxpilotTools, ArtifactScope),
`rules.md` BR1/BR3/BR7/BR9, `functional-spec.md` Pins, questions Q5-A,
shipped `connector/gateway.rs` (per-target `lock_for` map).

## Concurrency model

| id | requirement | basis |
|----|-------------|-------|
| NFR-S1 | The executor is stateless across calls: ONE `OxpilotTools` instance (an `Arc<dyn ToolExecutor>` pinned on `OxpilotRuntimeSlot.tools`, carried across provider/settings rebuilds) serves EVERY session/provider — all per-call scoping lives in `ToolCtx` + validated `args`, never in executor state; concurrent invokes across sessions share only immutable registry data | entities OxpilotTools, `oxpilot.rs:1046` |
| NFR-S2 | The registry is `&'static` compiled-in: `catalog()` is pure and identical for every turn — O(entries) lookup, zero allocation pressure worth bounding, no per-model/per-session cache to warm or invalidate | BR1.1, entities, NFR3.13 |
| NFR-S3 | U5 adds NO lock of its own: dispatch serializes inside the gateway's existing per-target `tokio::Mutex` map (`lock_for(target_id)` — dispatch, respond, lifecycle, dequeue all serialize there); the tool layer stays lock-free so a slow target can never wedge the shared executor | BR7.1, `gateway.rs:169–200` |
| NFR-S4 | Synthesized `RpcCtx` is built per call and dropped with it — no long-lived ctx held on the executor (a cached ctx would leak identity across sessions AND pin a stale session); construction is cheap field assembly, not a handshake | BR3.1, FD-Q11 |

## Growth edges

| id | requirement | basis |
|----|-------------|-------|
| NFR-S5 | Per-call internal fan-out is bounded by construction: reads = 1 dispatch; exec/fs = 1 `list_targets` + 1 `dispatch_action`; `pr_comment` = 1 scope query + 1 dispatch; `diff_apply` ≤ ~53 sequential dispatches (bounded by MAX_PROPOSAL_FILES). No call can fan out unboundedly regardless of args | NFR3.2–NFR3.6 |
| NFR-S6 | Derived state stays O(session) and ephemeral: ArtifactScope is rebuilt per `pr_comment` call from that session's applied proposals — bounded by proposals-per-session, dropped with the call, nothing accumulates across invokes | BR8.1, entities ArtifactScope |
| NFR-S7 | Catalog growth is a code-review event, not a runtime scaling axis: a new entry is one static row + handler — the registry stays flat (no nesting, no per-tenant catalogs) and the pin test asserts the set verbatim | BR1.1, Q7-A |

Scalability is otherwise N/A for this unit — the executor inherits the
API process's own scaling story; recorded per Q1-A.
