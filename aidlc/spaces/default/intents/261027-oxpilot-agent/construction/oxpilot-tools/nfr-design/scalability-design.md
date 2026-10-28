# Scalability Design — oxpilot-tools (U5)

Consumes: `nfr-requirements/scalability-requirements.md`
(NFR-S1–S7), `entities.md` (`OxpilotTools`, `ArtifactScope`,
`ToolCtx`), `rules.md` (BR1/BR3/BR7/BR9), shipped `runtime.rs`
(`OxpilotRuntimeSlot.tools` carry), `connector/gateway.rs`
(`lock_for(target_id)` — the per-target `tokio::Mutex` map U5
never touches).

## Concurrency model — one stateless executor (NFR-S1–S4)

- ONE `OxpilotTools` instance — an `Arc<dyn ToolExecutor>` pinned
  on `OxpilotRuntimeSlot.tools` (runtime.rs) — serves every
  session/provider. The SAME Arc rides provider/settings
  rebuilds: `turn_parts()` carries `tools` beside `turns`, it is
  not rebuilt with them. Concurrent `invoke`s share only
  immutable `&'static` registry data plus the `AppState` handle.
- The executor holds NO mutable state — no per-session maps, no
  caches, no counters, no in-flight flags. All per-call scoping
  lives in `ToolCtx` + validated `args` + the resolved session
  row — locals that die with the future. `&self` methods only;
  `Sync` by construction, not by locking (NFR-S1/S2).
- U5 adds NO lock anywhere: exec/fs serialization lives entirely
  inside the gateway's per-target `tokio::Mutex` map
  (`lock_for(target_id)`, gateway.rs) — dispatch/respond/
  lifecycle/dequeue serialize there on the U6 side; the tool
  layer stays lock-free so a slow or dead target can never wedge
  the shared executor or serialize unrelated sessions (NFR-S3).
- The synthesized `RpcCtx` is built inside `dispatch_proc` PER
  CALL and dropped with it (NFR-S4, Q4-A) — cheap field
  assembly, not a handshake; a cached ctx would both leak
  identity across sessions and pin a stale session behind a
  rebuilt provider.

## Growth edges (NFR-S5–S7)

| edge | bound |
|---|---|
| per-call fan-out | reads = 1 dispatch; exec/fs = ≤1 `list_targets` + 1 `dispatch_action`; `pr_comment` = 1 scope query + 1 dispatch; `diff_apply` ≤ ~53 sequential — NO call fans out unboundedly regardless of args (NFR-S5) |
| derived state | `ArtifactScope` is rebuilt per call from the session's applied proposals — O(proposals in this session), dropped with the call; nothing accumulates process-wide (NFR-S6) |
| catalog growth | a code-review event, not a runtime axis — a new tool is one static row + one named handler + one schema fn; the registry stays flat (no per-tenant catalogs) and the closure pin test re-asserts the exact set on every addition (NFR-S7) |

## What this unit deliberately does NOT scale

- No per-tenant or per-session executor pools — the single
  instance inherits the API process's own scaling story;
  concurrent sessions contend only on the shared db pool and the
  gateway's per-target locks (U6's domain).
- No worker queues, no backpressure — tool calls are
  request-scoped futures bounded by the 60s race and the round
  budget, both U3-owned knobs.
- Session count is a U2/U3 axis: `invoke` holds one resolved
  session row per live call and performs no cross-session
  iteration — concurrent-session contention is entirely a
  service-layer (U2) and lock-layer (U6) concern.
