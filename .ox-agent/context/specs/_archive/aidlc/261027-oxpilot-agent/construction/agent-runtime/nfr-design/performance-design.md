# Performance Design — agent-runtime (U3)

Consumes: `nfr-requirements/performance-requirements.md` (NFR3.x),
`functional-design/{rules,entities}.md`, `nfr-design-questions.md`
(Q3, Q5).

## Prompt windowing (Q3)

```rust
fn assemble_prompt(svc, session, owner_id, parts) -> Result<Vec<ChatMsg>, AppError> {
    let mut msgs = vec![system_preamble(session), repo_context(session)];
    let mut budget = PROMPT_TRANSCRIPT_MAX_BYTES; // 128 KiB
    let mut window: Vec<ChatMsg> = svc.transcript(owner_id, session.id)?
        .iter().rev()                       // newest-first
        .take_while(|m| {
            let n = rendered_len(m); // post-into_chat_msg bytes —
            // includes the ≤200 B/call tool-summary lines BR2.3 appends,
            // so a request can't exceed the 128 KiB pin
            if n <= budget { budget -= n; true } else { false }
        })
        .map(into_chat_msg).collect();
    window.reverse();                        // temporal order
    msgs.extend(window); Ok(msgs)
}
```

Constant request size regardless of session age (NFR3.1); the window
is pure — unit-testable without a provider.

## Turn-loop shape (Q5)

```
run_turn(task-scoped):
  parts = slot.turn_parts()              // one lock read — pinned triple
  provider none ⇒ terminal{error, oxpilot.disabled} (BR1.3/WF2.1 — post-gate provider-absent is the disabled-class code); model none ⇒ no_model
  msgs = assemble_prompt(...)            // bounded window
  for round in 0..MAX_TOOL_ROUNDS (8):
      select!{ stream item | cancel | TURN_MAX_DURATION }
        Delta → append delta frame + accumulate round slice
        ToolCall → per call:
            call_id = item.id or minted call_{msg_id}_{call_index}
            persist card running → emit tool/card frames (durable-first)
            res = timeout(TOOL_CALL_TIMEOUT, executor.invoke(..) race cancel)
            card done|error → emit; tool ChatMsg (redacted, ≤ TOOL_OUTPUT_MAX 32 KiB) onto buffer
            push assistant round msg (per-round delta slice + tool_calls_json)
        Done → terminal{done}
        Error(c) → terminal{error, c}
      rounds exhausted with pending calls ⇒ terminal{tool_rounds_exceeded}
```

Every terminal arm routes through `emit_terminal` (CAS). The
`select!` races cancel + wall-cap on every await point — no code path
blocks past the bounds (NFR3.7/3.8).

## Bounded structures

| structure | bound | mechanism |
|-----------|-------|-----------|
| prompt window | `PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB` | newest-first take_while |
| card summaries | `ARGS_SUMMARY_MAX` / `OUTPUT_SUMMARY_MAX` = 2 KiB | truncate + marker |
| tool rounds | `MAX_TOOL_ROUNDS = 8` | loop bound |
| per-call | `TOOL_CALL_TIMEOUT = 60 s` | `tokio::time::timeout` |
| turn wall | `TURN_MAX_DURATION = 10 min` | select arm |
| chain depth | `MAX_CHAIN_DEPTH = 16` | generation counter |
| `emitted_deltas` | unbounded — accepted risk | implicit wall×throughput bound |
| `ping` | `PING_INTERVAL = 15 s` (shipped) | `tokio::time::interval` |

## Lock-contention shape

`admission_lock` sections are the only serialized work: proc
(check+enqueue) ~one indexed insert; task (release+drain) ~one
indexed tx. Both per-session — cross-session parallelism unaffected
(NFR3.9/3.10, NFR11.4). The streaming path holds NO lock — deltas
fan out under the buffer's short `Mutex<Vec>` push only.
