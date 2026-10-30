# Performance Requirements — agent-runtime (U3)

Consumes: `construction/agent-runtime/functional-design/rules.md`
(BR2.x, BR3.x, BR5.x), `functional-design/entities.md`,
`functional-design/functional-spec.md`,
`inception/requirements-analysis/requirements.md` (NFR3),
`codekb/oxidean-app/technology-stack.md`.

Provider latency is external; this unit bounds OUR loop overhead,
buffer growth, and request size on the chat hot path.

## Request-size bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | Prompt assembly is bounded: newest-first transcript rows until `PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB` newest-first BYTE window (no row count — fd BR2.3/WF2.3) — a mature session's `ChatRequest` size is constant, so NFR3's interactive latency doesn't degrade with session age | NFR3, BR2.x |
| NFR3.2 | Repository-context preamble is the session's bound repo ref/summary — a fixed-size prefix, never a whole-repo dump | BR2.x |

## Buffer & frame cost

| id | requirement | basis |
|----|-------------|-------|
| NFR3.3 | `emitted_deltas` is unbounded BY DESIGN (late-attach replay is its purpose) — the implicit bound is `TURN_MAX_DURATION × provider throughput` (~MB-scale worst case); recorded as accepted risk at fd entities (review R-15) | entities ActiveTurn |
| NFR3.4 | Per-delta fanout is O(subscribers) — stream attachers take their own channel; no per-frame global lock beyond the buffer's short `Mutex<Vec>` push | entities ActiveTurn |
| NFR3.5 | `args_summary`/`output_summary` and the `tool` frame arg are capped at `ARGS_SUMMARY_MAX = 2 KiB`/`OUTPUT_SUMMARY_MAX = 2 KiB` — frame size is bounded regardless of tool payload size | entities, BR3.3 |
| NFR3.6 | `ping` frames reuse the shipped `PING_INTERVAL = 15 s` cadence (routes.rs) — stream liveness costs one frame per interval per attached client, no busy loop (C2) | C2, BR5.x |

## Loop & lock cost

| id | requirement | basis |
|----|-------------|-------|
| NFR3.7 | Tool calls run under `tokio::time::timeout(TOOL_CALL_TIMEOUT = 60 s)` racing `cancel` — a wedged executor can't hold the turn past the per-call bound; round exhaustion at `MAX_TOOL_ROUNDS = 8` terminals `tool_rounds_exceeded` (fd BR3.3) | BR3.3 |
| NFR3.8 | The whole turn runs under `TURN_MAX_DURATION = 10 min` — a runaway provider stream or tool loop terminals `turn_timeout`; the bound makes NFR3.3's implicit buffer bound real | BR5.2 |
| NFR3.9 | `admission_lock` is a per-session `tokio::sync::Mutex<()>` — its hold span is release→recheck (one drain) or live-check→enqueue (one insert); per-session scope means zero cross-session contention and no global serialization (fd BR3.4, BR4.1) | BR3.4, BR4.1 |
| NFR3.10 | The `ActiveTurns` map read path (claim lookup, fanout attach, `turns().get`) stays outside the admission lock — snapshot streams and queued-card pushes never queue behind an in-flight drain | BR3.4 |
