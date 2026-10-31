# Entities — u1-skeleton-slice

Entity model for the walking-skeleton unit. The fenced `yaml` block is
the source of truth; the prose summary follows it. Persisted entities
ship with their full U2-ready column set now (question Q6) so no ALTER
migration is needed later; computed views are derived per request and
never stored.

Consumes: `inception/units-generation/unit-of-work.md` (U1 boundary),
`inception/requirements-analysis/requirements.md` (FR1.*, FR2.4, FR6.6, NFR1),
`inception/domain-design/components.md` (SessionService/OxpilotApi
entity ownership).

## Entity source-of-truth

```yaml
entities:
  - name: OxpilotSession
    description: >
      One `/oxpilot` conversation bound to a signed-in user, a forge
      repository, and a branch. The agent's working context.
    persisted: oxpilot_sessions (migration trio: sqlite/postgres/mysql via dialect_oxpilot.rs)
    attributes:
      - { name: id, type: text, required: true, unique: true, notes: primary key; string id matching users.id/repositories.id convention (TEXT on sqlite+postgres, CHAR(36) on mysql) }
      - { name: user_id, type: text, required: true, references: users.id, notes: owner scope; every read/write is filtered by it }
      - { name: repo_owner, type: text, required: true, notes: forge owner segment; resolved to repo_id at create }
      - { name: repo_name, type: text, required: true }
      - { name: repo_id, type: text, required: true, references: repositories.id, notes: always populated — create resolves the repo or fails (BR3.1) }
      - { name: branch, type: text, required: true, notes: default repo default-branch when dialog omits }
      - { name: goal, type: text, required: false, notes: user's stated objective; feeds prompt assembly in U3 }
      - { name: title, type: text, required: true, notes: defaults to first 80 chars of goal or "Untitled session" }
      - { name: status, type: text, required: true, allowed: [active, deleted], default: active }
      - { name: created_at, type: timestamp-utc, required: true }
      - { name: last_activity_at, type: timestamp-utc, required: true, notes: bumped on message append; index ordering key }
    constraints:
      - unique: none beyond pk (a user may hold many sessions per repo)
      - index: (user_id, last_activity_at DESC) — the index page ordering
    relationships:
      - { to: users, cardinality: N:1, direction: session→user, on_delete: cascade }
      - { to: OxpilotMessage, cardinality: 1:N, direction: session→messages }

  - name: OxpilotMessage
    description: >
      One transcript line — user prompt, assistant reply, or (later
      units) tool-call record. Persisted on write so reopening a session
      restores it (AC1.3.3 share).
    persisted: oxpilot_messages
    attributes:
      - { name: id, type: uuid-text, required: true, unique: true, notes: primary key; the SSE exactly-once key }
      - { name: session_id, type: text, required: true, references: oxpilot_sessions.id }
      - { name: seq, type: integer, required: true, notes: per-session monotonic — the transcript ORDER key; distinct from the per-message delta counter inside an SSE append frame. That counter's WIRE name stays `seq` per C2 (`append:{message_id, seq, role, delta}`); `delta_seq` is only our design-internal name for the same field (R-13) }
      - { name: role, type: text, required: true, allowed: [user, assistant, system, tool] }
      - { name: content, type: text, required: true, notes: secrets-redacted on write (NFR1) }
      - { name: client_msg_id, type: text, required: false, notes: sender-supplied idempotency key on user rows; unique per session when present (BR3.6) }
      - { name: status, type: text, required: true, allowed: [complete, streaming, error, aborted], default: complete }
      - { name: created_at, type: timestamp-utc, required: true }
    constraints:
      - unique: (session_id, seq) — duplicate seq ⇒ write conflict, never a silent gap
      - unique: (session_id, client_msg_id) where client_msg_id IS NOT NULL — defense-in-depth behind the ActiveTurn claim; a re-sent body rejoins the existing turn's stream or, after it finished, answers with the persisted transcript (BR3.6)
      - index: (session_id, seq)
    relationships:
      - { to: OxpilotSession, cardinality: N:1, direction: message→session, on_delete: cascade }

  - name: ActiveTurn
    description: >
      In-memory, per-process turn marker — the owner of BR4.2's
      "one live turn per session" invariant and the source of
      `snapshot.active_turn`. Lives in a concurrent map on `RpcCtx`
      (same per-process scope as the existing FailedAuth/Lookup
      limiters); never persisted.
    attributes:
      - { name: session_id, type: text, required: true, unique: true, notes: map key — atomic insert-if-absent is the 409 check }
      - { name: turn_id, type: text, required: true, notes: fresh uuid minted at claim — distinct from message_id; surfaces in snapshot.active_turn }
      - { name: message_id, type: text, required: true, references: oxpilot_messages.id, notes: the streaming assistant row }
      - { name: started_seq, type: integer, required: true, notes: seq of the user row that opened the turn }
      - { name: fanout, type: broadcast-channel, required: true, notes: per-turn frame publisher (tokio::broadcast per the stream_job_log mpsc fan-out precedent); POST publishes frames, GET subscribers attach — readers never own the turn }
      - { name: emitted_deltas, type: ring-buffer, required: true, notes: buffered append frames for the in-flight message; replays to mid-turn attachers so exactly-once holds (R-17) }
      - { name: cancel, type: cancellation-token, required: true, notes: fires only on explicit abort (U3), session-delete hook, or process shutdown — NOT on transport close (R-15) }
    constraints:
      - lifecycle: insert-if-absent at POST-accept → the turn task owns the assistant row to a terminal status and the entry is removed when the task finishes; transport close only detaches readers (the detached task keeps streaming to the row so a reconnect sees the full reply); a process-death orphan is swept to status=error at startup
      - scope: per-process — consistent with existing in-proc limiters; multi-replica deployments would need a shared turn lock (documented, out of v1)

  - name: EnablementState
    description: >
      Computed view, never stored: whether `/oxpilot` answers, why not
      when off, and which provider is bound. Resolved fresh per call.
    attributes:
      - { name: enabled, type: bool, required: true, notes: flag AND provider-slot }
      - { name: flag_on, type: bool, required: true, notes: env default OXIDEAN_OXPILOT_ENABLED (default off) OR DB override row when present }
      - { name: provider, type: "{id, model} | null", required: true }
      - { name: reason, type: text, required: false, notes: "exactly one of flag_off | no_provider | misconfigured; precedence: flag_off first, then misconfigured (provider env set but unrecognized), else no_provider" }
    constraints:
      - derived: resolved per request — never cached across a settings write

  - name: ProviderHandle
    description: >
      The `LlmProvider` trait object slot on `RpcCtx` (C4). Resolution
      reads `OXIDEAN_OXPILOT_PROVIDER`: `stub` ⇒ install `StubProvider`;
      unset ⇒ None (reason `no_provider`); any other value ⇒ None
      (reason `misconfigured`) — U1 recognizes `stub` only; U4's
      registry teaches the slot openai|anthropic|ollama and rebuilds it.
    attributes:
      - { name: provider_id, type: text, required: true, allowed: [stub], notes: "openai|anthropic|ollama arrive with U4" }
      - { name: model, type: text, required: true, notes: "from OXIDEAN_OXPILOT_MODEL, default 'stub-1'; surfaced via ChatRequest.model, not the trait id()" }
    constraints:
      - rule: None ⇒ every gated surface answers `oxpilot.disabled` with reason from EnablementState
```

## Summary

Two persisted entities carry the skeleton: `OxpilotSession` (the
repo+branch-bound conversation, user-scoped) and `OxpilotMessage`
(the per-session ordered transcript with an idempotent `client_msg_id`
on user rows). `ActiveTurn` is the in-memory one-turn-per-session lock
that makes BR4.2 atomic and feeds `snapshot.active_turn`.
`EnablementState` and `ProviderHandle` are request-time views over
env/config/slot — they decide whether anything else may run. Tables
ship full-width for what U1/U2 need; `computer_use_target_id` is NOT
created here — U6 adds it alongside its own `oxpilot_targets` table so
the FK lands when its referent exists (one ALTER by the owning unit,
not a dangling reference).
