# CI/CD Pipeline Design — agent-runtime (U3)

Consumes: `nfr-design/*`, `team.md` (Way of Working, Testing Posture),
`contract-summary.md` (C1, C2, C6).

U3 changes **no CI jobs and no deployment topology** — the pipeline
contract is reused end-to-end. This records where U3 touches the
existing lanes and what must land in the same change to stay green.

## Build & test stages touched

| surface | what U3 adds | gate effect |
|---------|--------------|-------------|
| `job: rust-build` (nextest archive) | no new deps — tokio/tracing/oxidean-core only; `ToolExecutor` traits land in api-core | unchanged supply chain |
| `job: api-rust` shards | runtime unit tests (`assemble_prompt` windowing, emit_terminal CAS arms, drain_and_chain under lock, MAX_* bounds) + `turn_cancel` proc tests (C1 shape: `{aborted:bool}`, `no_active_turn`) + closed PROCEDURES pin extended to `turn_cancel` | existing sharding absorbs |
| `job: api-rust` SSE smokes | `chat_stream` attach-order + terminal-e2e smokes extended in the shipped test binary | same lane |
| `make rpc-gen` / `job: rpc-sync` | `oxpilot.turn_cancel` client types must be generated + committed — Rust is the source of truth | existing job fails on drift |
| `job: db-dialects` | none — U3 adds no tables | unchanged |

## Deployment strategy

Unchanged rolling path (PR → `pr-<n>` → ephemeral env → squash →
`sha-<sha>` → staging → manual promote). U3 rides the same `api`
image — `images.yml` untouched.

**Ordering**: the runtime assumes U2's tables + service exist
(migration `0087` is already in-tree and additive-only). A new binary
against a pre-U2 database fails service calls `oxpilot.internal`,
never boot — same contract every prior migration rides.

## Rollback procedures

1. `admin.oxpilot.settings_set{enabled_override:false}` / env flag —
   an ADMISSION gate only: new `chat_post`/`message_queue`/
   `turn_cancel` refuse `oxpilot.disabled`, so no NEW turn starts.
   Live turns are unaffected — `turn_parts()` pinned the provider at
   turn start and no mid-turn flag-flip kill exists (WF4 has no flag
   arm); they run to their natural terminal (or `TURN_MAX_DURATION`),
   then their release→recheck drains under `admission_lock`. To stop a
   LIVE turn the levers are `turn_cancel`/`session_delete` — which
   themselves refuse `oxpilot.disabled` under flag-off — so a wedged
   live turn's only recourse is item 2 (image repin restarts the
   process; the map is process-local and dies with it, stale
   `streaming` rows reap at boot).
2. Standard image repin — in-memory turn state is empty at boot; the
   boot sweep clears stale `streaming` rows on the older binary too.
