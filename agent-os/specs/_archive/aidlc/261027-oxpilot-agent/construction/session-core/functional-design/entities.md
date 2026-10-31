# Entities — session-core (U2)

## Entity model

```yaml
entities:
  - name: OxpilotSession
    kind: persisted
    provenance: U1 (0085_oxpilot.sql) — U2 owns the lifecycle semantics, no schema change.
    description: >
      One /oxpilot conversation per row: user-owned, bound to a forge
      repository + branch. All reads and writes are caller-scoped by
      user_id; cross-user access resolves `not_found` (anti-enumeration).
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key"] }
      - { name: user_id, type: text, required: true, constraints: ["FK users.id ON DELETE CASCADE", "caller-scope column — never set from request input"] }
      - { name: repo_owner, type: text, required: true }
      - { name: repo_name, type: text, required: true }
      - { name: repo_id, type: text, required: true, constraints: ["FK repositories.id ON DELETE CASCADE"] }
      - { name: branch, type: text, required: true }
      - { name: goal, type: text|null, required: false, constraints: ["secrets-redacted on write"] }
      - { name: title, type: text, required: true, default: "Untitled session", constraints: ["derived from the redacted goal"] }
      - { name: status, type: text, required: true, default: "active", allowed_values: ["active", "archived", "deleted"], constraints: ["'deleted' rows are filtered out of every read path (U1 shipped: list/find skip status='deleted')"] }
      - { name: created_at, type: timestamp, required: true }
      - { name: last_activity_at, type: timestamp, required: true, constraints: ["touched on every message/tool/queue/proposal write"] }
    constraints:
      - list order is (user_id, last_activity_at DESC) via the U1 index.
      - hard deletion cascades to all owned child rows (FK ON DELETE CASCADE).

  - name: OxpilotMessage
    kind: persisted
    provenance: U1 (0085_oxpilot.sql) — U2 owns the read/write semantics, no schema change.
    description: >
      One transcript entry per row, ordered by per-session monotonic `seq`.
      `client_msg_id` is the sender-supplied idempotency key on user rows
      (UNIQUE(session_id, client_msg_id)); content is secrets-redacted
      before insert.
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key"] }
      - { name: session_id, type: text, required: true, constraints: ["FK oxpilot_sessions.id ON DELETE CASCADE"] }
      - { name: seq, type: integer, required: true, constraints: ["UNIQUE(session_id, seq)", "monotonic per session"] }
      - { name: role, type: text, required: true, allowed_values: ["user", "assistant", "system", "tool"] }
      - { name: content, type: text, required: true, default: "''", constraints: ["secrets-redacted on write (AC5.3.3, NFR1)"] }
      - { name: client_msg_id, type: text|null, required: false, constraints: ["UNIQUE(session_id, client_msg_id) — the dedupe key, shared semantics with queued rows"] }
      - { name: status, type: text, required: true, default: "complete", allowed_values: ["streaming", "complete", "error"], constraints: ["shipped vocabulary — sweep marks orphans 'error' (oxidean-db oxpilot.rs:713); C2 terminal results done|error|aborted map to complete|error|error"] }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - `streaming` rows on boot are swept to `error` by the U1 orphan
        sweep — no zombie streaming state across restarts.

  - name: OxpilotToolCall
    kind: persisted
    description: >
      One recorded tool invocation per row — the durable form of the C2
      SSE `tool` frame and the in-transcript ToolCard (AC1.3.6). Keyed by
      the assistant `message_id` that produced the call plus a per-call
      `call_id`, so one message can hold repeated same-tool invocations.
      Carries only summaries; raw args/output never persist.
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key"] }
      - { name: session_id, type: text, required: true, constraints: ["FK oxpilot_sessions.id ON DELETE CASCADE"] }
      - { name: message_id, type: text, required: true, constraints: ["references oxpilot_messages.id of the owning assistant row — the C2 tool-frame key"] }
      - { name: call_id, type: text, required: true, constraints: ["per-call identifier — provider tool_call id when present, else minted at emission", "UNIQUE(session_id, call_id)"] }
      - { name: seq, type: integer, required: true, constraints: ["positional copy of the owning message's seq — never minted on this table; orders tool cards inside the transcript merge"] }
      - { name: call_index, type: integer, required: true, constraints: ["0-based emission order inside the owning message", "intra-message ordering tiebreak for the C2 snapshot merge (BR7.3)"] }
      - { name: tool, type: text, required: true, description: "tool name from the U5 catalog" }
      - { name: args_summary, type: text, required: true, default: "''", constraints: ["secrets-redacted on write", "human-readable short form, never raw args"] }
      - { name: status, type: text, required: true, allowed_values: ["running", "done", "error"], constraints: ["matches the C2 tool-frame vocabulary"] }
      - { name: output_summary, type: text, required: true, default: "''", constraints: ["secrets-redacted on write"] }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - UNIQUE(session_id, call_id) — repeated same-tool calls under one
        message are distinct rows.
      - A status transition update path exists (running → done|error);
        append-only rows otherwise.

  - name: OxpilotQueuedMessage
    kind: persisted
    description: >
      A user follow-up enqueued while a reply streams (US1.8), written by
      the `oxpilot.message_queue` proc after the chat POST answers
      409 turn_active. Queued rows have no transcript `seq` — the
      drain-into-transcript op mints one when delivering them.
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key", "surfaced as queued_id"] }
      - { name: session_id, type: text, required: true, constraints: ["FK oxpilot_sessions.id ON DELETE CASCADE"] }
      - { name: queue_seq, type: integer, required: true, constraints: ["UNIQUE(session_id, queue_seq)", "monotonic per session", "position in queue_list payloads derives from this"] }
      - { name: content, type: text, required: true, constraints: ["secrets-redacted on write"] }
      - { name: client_msg_id, type: text|null, required: false, constraints: ["UNIQUE(session_id, client_msg_id)", "union-deduped against oxpilot_messages.client_msg_id (BR2.2)", "carried into the appended message row on drain"] }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - drain is a single-transaction DELETE…RETURNING + message-INSERT —
        delivered exactly once or not at all (no committed-delete window).

  - name: OxpilotProposal
    kind: persisted
    description: >
      A reviewable diff the agent produced for the session's bound repo
      (US2.1) — the persistence half of C5's save_proposal/proposal/
      mark_applied. Materialization into branch+PR is the later U5/U8
      action that flips status and records the PR link.
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key"] }
      - { name: session_id, type: text, required: true, constraints: ["FK oxpilot_sessions.id ON DELETE CASCADE"] }
      - { name: title, type: text, required: true, constraints: ["secrets-redacted on write (BR3.1)"] }
      - { name: base, type: text, required: true, constraints: ["base ref/sha the diff applies against — C1 ProposedDiff.base"] }
      - { name: diff_body, type: text, required: true, constraints: ["unified-diff text", "secrets-redacted on write", "C1's files[{path,patch}] payload derives by splitting diff_body on 'diff --git' headers at read time — one column, canonical form"] }
      - { name: status, type: text, required: true, default: "pending", allowed_values: ["pending", "applied"], constraints: ["C1 diff_* payloads carry this as ProposedDiff.status"] }
      - { name: pr_ref, type: text|null, required: false, description: "owner/repo#n recorded by materialize (U5/U8)" }
      - { name: created_at, type: timestamp, required: true }
    constraints:
      - status transitions pending → applied only; `applied` is terminal.

  - name: OxpilotUsageCounter
    kind: persisted
    description: >
      Rolling per-window spend counters for US5.2 — one row per
      (scope, scope_key, window). Both axes roll together: `requests`
      increments per completed turn exchange; `tokens` increments when the
      provider reports `Done{usage}`.
    attributes:
      - { name: id, type: text, required: true, unique: true, constraints: ["primary key"] }
      - { name: scope, type: text, required: true, allowed_values: ["user", "instance"] }
      - { name: scope_key, type: text, required: true, description: "user_id, or the literal '__instance__' for instance scope" }
      - { name: window, type: text, required: true, description: "UTC ISO day YYYY-MM-DD — rollover is UTC-midnight, never local time" }
      - { name: requests, type: integer, required: true, default: 0, constraints: ["one per completed turn exchange (a user send + terminal)"] }
      - { name: tokens, type: integer, required: true, default: 0 }
      - { name: updated_at, type: timestamp, required: true }
    constraints:
      - UNIQUE(scope, scope_key, window) — upsert-increment target.
      - A missing row reads as zero — no bootstrap write needed.
      - Budget binding: the configured `user_budget`/`instance_budget`
        value V is the cap for BOTH axes — requests_cap=V and
        tokens_cap=V in `oxpilot.usage_get` payload terms; over on either
        axis refuses.

relationships:
  - OxpilotSession --owns--> OxpilotMessage: 1:N, cascade.
  - OxpilotMessage --owns--> OxpilotToolCall: 1:N via message_id (session-level FK cascade).
  - OxpilotSession --owns--> OxpilotQueuedMessage: 1:N, cascade.
  - OxpilotSession --owns--> OxpilotProposal: 1:N, cascade.
  - users --owns--> OxpilotSession: 1:N, cascade (U1 FK).
  - repositories --owns--> OxpilotSession: 1:N, cascade (U1 FK).
  - OxpilotUsageCounter --standalone: keyed by (scope, scope_key, window); no FK — survives user/session deletes as an audit-grade rollup.
```

## Summary

U2 owns the persistence substrate for every conversational artifact:
the two U1 tables (`OxpilotSession`, `OxpilotMessage`) plus four new
tables. `OxpilotToolCall` records tool invocations keyed by the owning
assistant `message_id` + per-call `call_id` (repeated same-tool calls
persist distinctly); `OxpilotQueuedMessage` is the mid-stream queue
filled by `oxpilot.message_queue` and emptied by a transactional
drain-into-transcript that carries `client_msg_id` for union dedupe;
`OxpilotProposal` is the reviewable diff artifact; `OxpilotUsageCounter`
rolls requests+tokens per user-per-UTC-day and instance-per-UTC-day for
the US5.2 budget refusal where one configured value caps both axes.
Every body-bearing field is secrets-redacted before it reaches the
store, and every access is caller-scoped through `sessions.user_id`
with anti-enumeration `not_found` resolution. The three session-child
tables (tool_call, queued, proposal) carry `ON DELETE CASCADE` to
`oxpilot_sessions` so the C8 hook-then-cascade delete is a single
statement; `OxpilotUsageCounter` deliberately does NOT cascade — it is
the standalone audit-grade rollup (BR5.3).
