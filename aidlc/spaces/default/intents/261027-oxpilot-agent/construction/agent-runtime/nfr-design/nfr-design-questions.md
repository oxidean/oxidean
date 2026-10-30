# NFR Design Questions — agent-runtime

Unit: `agent-runtime` (U3, kind `service`) — how each NFR-requirements
row becomes a concrete mechanism: the `ActiveTurns` map shape, the
admission lock, the terminal CAS, drain/chain choreography, prompt
windowing, provider pinning, and observability emission.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U3.
- [req] `construction/agent-runtime/nfr-requirements/*.md` —
  NFR1.x–NFR11.x the mechanisms satisfy.
- [design] `construction/agent-runtime/functional-design/{entities,
  rules, functional-spec}.md` — BR1–BR7 the mechanisms implement.
- [contract] contract-summary.md — C2 SSE, C4 provider, C5 service,
  C6 tools, C8 hooks.

## Q1. `ActiveTurns` map + admission-lock representation?

- A. Extend the shipped `ActiveTurns` (DashMap-style per-session map)
  with `admission_lock: Arc<tokio::sync::Mutex<()>>` on each entry —
  the map hands the lock Arc out; it drops with the entry. Holder
  spans: proc side (live-check + `enqueue` commit), task side
  (release + recheck drain). Guard-scoped; no manual unlock.
- B. A standalone `Mutex<()>` map keyed by session — two structures
  to keep consistent; orphaned locks outlive sessions.
- C. `std::sync::Mutex` — blocks across `.await` inside the drain;
  wrong primitive for an async hold span.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — ActiveTurns entries carry admission_lock: Arc<tokio::sync::Mutex<()>>, dropped with the entry; proc/task holders share it; guard-scoped. Conductor-decided (2026-10-27); ratify at PR review.
## Q2. Terminal first-wins mechanism?

- A. `emit_terminal(&turn, frame)` on `ActiveTurn` — CAS on
  `terminal_sent: AtomicBool` inside the `emitted_deltas` lock so the
  first writer commits both the flag and the buffer push atomically;
  losers no-op. Called by every terminal path (provider error, abort
  hook, timeout, rounds-exceeded, wrapper Err arm).
- B. A `tokio::sync::OnceCell` — same first-wins semantics but the
  payload type makes "inspect who won" awkward; CAS+bool is simpler.
- C. Channel close as terminal — can't carry the terminal payload;
  C2 pins a frame.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — emit_terminal CAS on terminal_sent inside the emitted_deltas lock; losers no-op. Conductor-decided (2026-10-27); ratify at PR review.
## Q3. Prompt-assembly structure?

- A. `assemble_prompt(svc, session, owner_id) -> Result<Vec<ChatMsg>,Err>`
  — system preamble + repo-context prefix (session's bound repo
  ref) + newest-first transcript rows until
  `PROMPT_TRANSCRIPT_MAX_BYTES = 128 KiB`, reversed to temporal order;
  pure function over service reads, separately testable.
- B. Inline in `run_turn` — untestable, couples prompt shape to the
  task lifecycle.
- C. Streaming window compaction — provider tokenizers differ; the
  byte window is the honest portable bound.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — assemble_prompt pure fn: system + repo prefix + newest-first 128 KiB byte window reversed to temporal. Conductor-decided (2026-10-27); ratify at PR review.
## Q4. Provider-pinning mechanics?

- A. `turn_parts()` on the registry slot returns
  `{provider: Option<Arc<dyn LlmProvider>>, model: Option<String>,
  executor: Arc<dyn ToolExecutor>}` under ONE lock read at turn
  start — mid-turn settings writes cannot swap the provider or
  executor; `model: None` ⇒ `no_model` terminal before streaming.
- B. Read provider at turn start, re-read model per round —
  inconsistent triple; a mid-turn settings write could mix provider
  A's stream with provider B's tool encoding.
- C. Pin at `chat_post` time — admission and turn task are
  different moments; the pin belongs to the task.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — turn_parts() one-lock triple {provider, model, executor}; model None ⇒ no_model terminal. Conductor-decided (2026-10-27); ratify at PR review.
## Q5. Tool-loop orchestration shape?

- A. `run_round(turn, msgs, parts) -> RoundOutcome` — per round:
  provider stream → deltas fan out + accumulate → `ToolCall` items
  collect; per call: mint effective `call_id` (provider's or
  `call_{message_id}_{call_index}`), persist card `running` → emit
  `tool`/`card` frames → `executor.invoke` under
  `TOOL_CALL_TIMEOUT = 60 s` racing `cancel` → card `done`/`error` →
  append assistant round msg (per-round delta slice + tool_calls_json)
  + `ChatMsg{role:"tool"}` per call. `MAX_TOOL_ROUNDS = 8` exhaustion
  ⇒ `tool_rounds_exceeded` terminal.
- B. Recursive rounds — depth bound is identical but the iterative
  form keeps `msgs` mutation in one function.
- C. Executor-driven loop — C6 gives the EXECUTOR no loop authority;
  the runtime owns provider interaction.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — iterative run_round; effective call_id mint; persist card running → frames → invoke(60s race cancel) → done/error; tool ChatMsg per call; MAX_TOOL_ROUNDS=8. Conductor-decided (2026-10-27); ratify at PR review.
## Q6. Claim lifecycle mechanics?

- A. `ActiveTurn` construction mints `turn_id`, `cancel:
  CancellationToken`, `generation` (chain depth); claim inserts the
  entry; `release` removes map entry; the task wrapper
  (`spawn_turn_task`) owns release on every exit shape — inner
  `tokio::spawn` JoinHandle awaited, `Err(JoinError)` covered. The
  task NEVER releases inside `run_turn` — the wrapper owns it.
- B. `Drop` guard on `ActiveTurn` — map removal isn't Drop-friendly
  (entry lives in the map); an explicit wrapper release is clearer.
- C. RAII via channel close — can't distinguish exit shapes for the
  warn arm.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — wrapper-owned release via inner JoinHandle await (covers Ok/Err/panic); run_turn never releases. Conductor-decided (2026-10-27); ratify at PR review.
## Q7. Observability emission mechanics?

- A. `tracing::info_span!("oxpilot.turn", session_id, turn_id,
  provider_id, model)` per turn task; `.in_scope`/`instrument` the
  loop; log lines follow BR7.1's pinned table — `info` turn start,
  `debug` tool/drain/terminal, `warn` abort and the degraded arms
  (recheck-Err, claim-loss, panic-release, card_persist).
- B. A bespoke event bus — NFR6 forbids a new stack.
- C. Log macros inline without span — loses turn correlation across
  interleaved sessions.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — info_span oxpilot.turn per task; BR7.1 log table verbatim. Conductor-decided (2026-10-27); ratify at PR review.
## Q8. Stream attach/detach mechanics?

- A. `chat_stream` → snapshot frame (built from `list_messages` —
  includes `streaming` rows) → if live turn: subscribe `fanout`
  FIRST, then clone `emitted_deltas` into the SSE response (the
  shipped order — buffer-first loses a frame emitted in the
  clone→subscribe window; a duplicate is client-deduped, a missed
  frame is not), then live frames; `ping` every `PING_INTERVAL =
  15 s`; close on terminal or transport drop. Late attach after terminal resolves `None` →
  snapshot-only stream that ends clean.
- B. Long-poll the turn — C2 pins SSE; the shipped route is already
  SSE.
- C. Replay from `fanout` only — misses frames emitted before attach;
  the buffer is the replay source.
- D. Not decided.
- X. Other (please specify)

[Answer]: A — snapshot → fanout subscribe → buffer replay → live frames → 15s ping → terminal/close (shipped attach order); late attach resolves snapshot-only. Conductor-decided (2026-10-27); ratify at PR review.
## Consolidated Summary Confirmation

- Turn map: per-session entry + `admission_lock` Arc, guard-scoped
  (Q1).
- Terminal: `emit_terminal` CAS inside the buffer lock (Q2).
- Prompt: `assemble_prompt` pure fn, 128 KiB newest-first byte window
  (Q3).
- Pinning: `turn_parts()` triple under one lock read (Q4).
- Tool loop: iterative `run_round`, persist-before-emit,
  timeout-racing invoke (Q5).
- Claims: wrapper-owned release on all exit shapes (Q6).
- Observability: one `info_span` per turn + BR7.1 log table (Q7).
- Attach: snapshot → fanout subscribe → buffer replay → live frames
  → ping (Q8).

Does this all look correct before I write the NFR-design artifacts?

- Looks correct
- Request changes

[Answer]: Looks correct
