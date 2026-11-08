# Entities — agent-runtime (U3)

## Entity model

```yaml
entities:
  - name: ActiveTurn
    kind: in-memory (runtime registry)
    provenance: U1 (api-core `oxpilot.rs`); U3 extends its USE, not its shape.
    description: >
      One live turn per session — the claim map entry that owns the
      turn's broadcast fanout, buffered frame replay, cancellation
      token, and pending-send identity fields for resend matching.
    attributes:
      - { name: session_id, type: text, required: true }
      - { name: turn_id, type: text, required: true, constraints: ["uuid; release keyed on it so a stale task can't release a successor"] }
      - { name: message_id, type: text, required: true, constraints: ["the single assistant row this turn owns — one turn writes one assistant row"] }
      - { name: started_seq, type: atomic_i64, required: true, constraints: ["0 until the user row mints; resend_matches keys on it"] }
      - { name: pending_client_msg_id, type: text|null }
      - { name: pending_content, type: text, required: true, constraints: ["redacted copy for resend matching in the claim→insert window"] }
      - { name: fanout, type: broadcast::Sender<Value>, required: true, constraints: ["capacity TURN_FANOUT_CAPACITY; Lagged readers replay the buffer"] }
      - { name: emitted_deltas, type: Mutex<Vec<Value>>, required: true, constraints: ["full frame buffer incl. terminal — attach replay + queued-card delivery + abort frame", "unbounded BY DESIGN (late-attach replay is its purpose); accepted risk — bounded implicitly by TURN_MAX_DURATION × provider throughput (~MB-scale for a worst-case turn), recorded under NFR4 follow-up"] }
      - { name: cancel, type: CancellationToken, required: true, constraints: ["ADR-010 handle — abort hook + per-call timeouts observe it"] }
      - { name: terminal_sent, type: AtomicBool, required: true, constraints: ["first-wins terminal emission — every terminal frame passes `emit_terminal` which CASes this under the emitted_deltas lock; losers no-op (BR5.4)"] }
    constraints:
      - One live turn per session by construction (`ActiveTurns.claim`).
      - Turn state is the single writer of its frames — the only
        exception is `message_queue` pushing a `card{queued}` (BR3.4),
        which is a buffer+fanout push, never a stream item.

  - name: TurnChain
    kind: in-memory (loop state inside the spawned turn task)
    provenance: U3 — new.
    description: >
      The post-terminal drain-and-chain state: after a turn's assistant
      row reaches terminal status and BEFORE the terminal frame emits,
      `drain_queue` delivers the pending batch; a non-empty batch means
      the runner releases and immediately re-claims for the next turn.
    attributes:
      - { name: drained, type: Vec<OxpilotMessageRow>, required: true, constraints: ["transcript rows in queue_seq order — the next turn's seed"] }
      - { name: generation, type: integer, required: true, constraints: ["monotonic chain depth; bounded by MAX_CHAIN_DEPTH"] }
    constraints:
      - A POST landing in the release→reclaim gap wins the claim; its
        row lands after the drained batch in `seq` order — coherent.
      - The chain never holds the claim across turns — each turn is an
        ordinary claim; fairness for interleaved sends is preserved.

  - name: ChatMsg (extended)
    kind: wire (provider request member)
    provenance: U1 C4; U3 EXTENDS — flagged contract amendment.
    description: >
      One inbound message for the provider. The tool round-trip needs
      what {role,content} can't express: the assistant's own call list
      and the per-call result block.
    attributes:
      - { name: role, type: text, required: true, allowed_values: ["user", "assistant", "system", "tool"], constraints: ["'tool' is the result role — new in U3"] }
      - { name: content, type: text, required: true }
      - { name: tool_call_id, type: text|null, required: false, constraints: ["set when role='tool' — the call this result answers; NEVER empty on the wire — the loop mints `call_{message_id}_{call_index}` when the provider omits an id (Ollama may), because the durable card keys UNIQUE(session_id, call_id)"] }
      - { name: tool_name, type: text|null, required: false, constraints: ["set when role='tool' — Ollama's tool-result message is name-keyed; also the fallback for adapters that can't resolve name from tool_calls_json"] }
      - { name: tool_calls_json, type: text|null, required: false, constraints: ["set on the assistant's round message when it emitted calls — the serialized call array the adapter maps to provider-native tool_use/tool_calls"] }
    constraints:
      - Both new fields are Option — every pre-U3 constructor
        (StubProvider, tests) passes None; adapters serialize them only
        when present.
      - C4 amendment per the contract deviation rule: additive fields on
        a U1-owned type, consumer-proposed (U3), committed in this
        change — all adapters (openai/anthropic/ollama/stub) update
        together.

  - name: ToolSpec (provider view)
    kind: wire
    provenance: U1 C4 — unchanged.
    description: >
      One catalog entry the model may invoke — {name, description,
      parameters_json}. `ChatRequest.tools` is populated from
      `ToolExecutor.catalog()`.
    attributes:
      - { name: name, type: text, required: true }
      - { name: description, type: text, required: true }
      - { name: parameters_json, type: json, required: true }

  - name: ToolCall (invoke input)
    kind: wire (C6)
    provenance: U3 defines (C6 owner).
    description: >
      One model-requested call handed to the executor — identity-bound
      to the session via ToolCtx (ADR-008).
    attributes:
      - { name: call_id, type: text, required: true, constraints: ["the EFFECTIVE call id — provider's when non-empty else minted `call_{message_id}_{call_index}` — correlates result → card row (UNIQUE(session_id, call_id))"] }
      - { name: name, type: text, required: true }
      - { name: args_json, type: text, required: true, constraints: ["raw provider JSON; the executor validates per-tool", "card's `args_summary` + the `tool` frame arg are ≤ ARGS_SUMMARY_MAX (2 KiB), truncated with marker"] }

  - name: ToolCtx
    kind: wire (C6)
    provenance: U3 defines (C6 owner); ADR-008 pin.
    description: >
      The identity envelope every call carries — the executor resolves
      authz from it; callers cannot forge user/session scope.
    attributes:
      - { name: session_id, type: text, required: true }
      - { name: user_id, type: text, required: true }
      - { name: repo_id, type: text, required: true }
      - { name: proposal_id, type: text|null, required: false, constraints: ["present when the call continues an in-flight proposal — the runtime always passes None in v1; the executor resolves proposal linkage itself from session context (U5 owns that resolution)"] }

  - name: ToolOutput
    kind: wire (C6)
    provenance: U3 defines (C6 owner).
    description: >
      A successful call's result — `content` is the text the provider
      sees (`role:"tool"` body); `output_summary` is the bounded form
      the card row persists.
    attributes:
      - { name: content, type: text, required: true, constraints: ["secrets-redacted before it leaves the executor — results can echo file content", "≤ TOOL_OUTPUT_MAX (32 KiB) into the provider request — truncate with marker"] }
      - { name: output_summary, type: text, required: true, constraints: ["≤ OUTPUT_SUMMARY_MAX (2 KiB) — the durable card field + frame arg"] }

  - name: ToolError
    kind: wire (C6)
    provenance: U3 defines (C6 owner).
    description: >
      A failed call — its message still feeds back as a `role:"tool"`
      result (the model must see every call answered); the card writes
      `error` + a bounded message summary.
    attributes:
      - { name: code, type: text, required: true, constraints: ["stable code — timeout|aborted|denied|tool_failed|<executor-defined>"] }
      - { name: message, type: text, required: true, constraints: ["secrets-redacted; ≤ OUTPUT_SUMMARY_MAX into provider content + card summary"] }

  - name: ToolClass (registry-side, not on ToolSpec)
    kind: in-memory (U5 registry)
    provenance: C6 annotation "read|write class" — resolved here.
    description: >
      Tool class (read|write) is NOT a `ToolSpec` field — it lives on
      the U5 registry's side and drives write-audit + grant checks
      (write-class ⇒ audit row, connector-bound ⇒ consent grant). The
      provider-facing catalog stays the three-field ToolSpec; U5 maps
      name→class internally.

  - name: ToolCard (durable form)
    kind: persisted (OxpilotToolCall row — U2 owns the table)
    provenance: U2 entity; U3 is its writer at runtime.
    description: >
      The SSE `tool` frame's durable twin — rendered by U8 from the row;
      keyed (message_id, call_id) with call_index ordering.
    attributes: []  # field set pinned in U2 entities.md
    constraints:
      - Every emitted `tool` frame has a persisted row BEFORE the frame
        (durable-first — a mid-flight crash leaves the card, not a
        phantom frame).
```

## Observations

- The turn is the unit of stream identity: one `ActiveTurn`, one
  assistant `message_id`, one `seq` space for append frames.
- Everything the loop needs crosses the `SessionService` port — the
  runner never calls `state.db` directly (U2 review pin carried here).
- `ChatMsg.tool_call_id`/`tool_calls_json` is the ONE contract delta this
  unit introduces; it is flagged in contract-summary.md as a C4
  amendment, additive under the ownership rules.
