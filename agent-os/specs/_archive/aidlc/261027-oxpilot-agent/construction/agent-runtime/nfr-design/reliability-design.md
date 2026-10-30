# Reliability Design — agent-runtime (U3)

Consumes: `nfr-requirements/reliability-requirements.md` (NFR4.x),
`functional-design/{rules,functional-spec}.md`,
`nfr-design-questions.md` (Q1, Q2, Q6).

## Claim lifecycle (Q6)

```rust
// spawn_turn_task — wrapper owns release unconditionally:
let handle = tokio::spawn(async move {          // outer task
    let inner = tokio::spawn(run_turn(turn.clone(), seed));
    match inner.await {
        Ok(_) => {}                              // normal terminal path
        Err(je) => {                             // panic/cancel
            warn!(session_id, "turn task join error");
            emit_terminal(&turn, internal_frame()); // CAS — no-op if sent
        }
    }
    drain_and_chain(turn).await;                 // release+recheck, locked
});
```

`run_turn` NEVER calls `release` — single-owner semantics; the
`Err(JoinError)` arm covers panic AND cancellation (NFR4.2).

## Terminal CAS (Q2)

```rust
pub(crate) async fn emit_terminal(turn: &ActiveTurn, frame: Value) -> bool {
    let mut buf = turn.emitted_deltas.lock().await;  // shipped tokio::Mutex
    if turn.terminal_sent.swap(true, Ordering::SeqCst) { return false; }
    buf.push(frame.clone());                      // CAS+push under one hold
    let _ = turn.fanout.send(frame);              // subscribers get it
    true
}
```

The flag CAS sits INSIDE the buffer lock so the winner's flag set and
frame push are atomic relative to a concurrent winner-claim (BR5.4);
no frame emits after terminal holds structurally — losers return
early.

## Drain choreography (NFR4.4–4.6)

```
drain_and_chain(turn):                      // runs under admission_lock
    let Some((_entry, lock)) = turns.admission_lock(session_id).await
        else { return };                        // entry already gone → exit
    let g = lock.lock().await;
    turns.release(session_id, turn.turn_id); // id-paired: can't evict a successor
    match svc.drain_queue(owner, session).await {
        Ok(rows) if !rows.is_empty() && turn.generation < MAX_CHAIN_DEPTH
            => match turns.claim(session_id, generation+1) {     // re-claim;
                 Ok(next) => spawn_turn_task(next, seeded by rows),
                 Err(_lost) => {} // claim-loss ⇒ exit quietly — the winner's
               },                // residual drain + prompt window cover the rows
        Ok(_) => {}                          // empty/depth-cap → exit
        Err(e) => warn!(session_id, err=%e), // persisted rows survive;
    }                                        // next turn's residual drains
```

`message_queue` holds the same lock for its live-check+enqueue, so a
row can never commit between release and recheck — the airtight seam
(fd review R-20, BR3.4).

## Failure-arm matrix

| event | terminal | claim |
|-------|----------|-------|
| provider open fail / `Error` item | `{error, code}` provider-verbatim | wrapper release |
| `model: None` at turn start | `{error, no_model}` | wrapper release |
| cancel (turn_cancel/delete) | `{aborted}` no code | wrapper release → recheck — PINNED: `drain_and_chain` runs on abort too (queued user rows are durable intent, not the cancelled turn's product); on the delete path the cascade wins the race and the Err/claim-loss arms exit quietly |
| wall cap | `{error, turn_timeout}` | wrapper release |
| rounds exhausted | `{error, tool_rounds_exceeded}` | wrapper release |
| card persist fail | NOT terminal — `card_persist` fed back as `ToolError` result; model sees the failure | — |
| panic/JoinError | `{error, internal}` via wrapper Err arm | wrapper release |
| process crash | none — `streaming` rows reaped by boot sweep; claim dies with process | — |

## Durable-first ordering (BR2.4)

Card row `running` persists BEFORE its `tool`/`card` frames emit;
assistant terminal status write precedes the terminal frame; drained
rows land in transcript before the ending terminal. Crash mid-order
leaves rows — never phantom frames (NFR4.11).
