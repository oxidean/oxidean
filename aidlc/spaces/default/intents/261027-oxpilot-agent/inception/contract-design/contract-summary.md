# Contract Summary — `/oxpilot`

Boundaries pinned so units build in parallel. Authority order: this file
fixes **families, semantics, and error behaviour**; the generated
api-client / OpenAPI doc are outputs of `make rpc-gen`, not inputs.
Consumes `unit-of-work.md`, `unit-of-work-dependency.md`, `components.md`,
`requirements.md` ([dag], [req]).

## Contracts table

| # | Provider Unit | Consumer | Mechanism | Owner |
|---|---|---|---|---|
| C1 | skeleton-slice (U1) | External: web UI + `ox` CLI | typed RPC shared-schema | U1 |
| C2 | skeleton-slice (U1) | External: web UI | SSE over HTTP (yaml frame spec) | U1 |
| C3 | connector-gateway (U6) | External: user-run connector | OpenAPI HTTP | U6 |
| C4 | skeleton-slice (U1) | agent-runtime (U3), providers (U4) | Rust trait `LlmProvider` shared-schema | U1 |
| C5 | session-core (U2) | U3, U6, U8 | Rust API shared-schema | U2 |
| C6 | agent-runtime (U3) | oxpilot-tools (U5) | Rust trait `ToolExecutor` shared-schema | U3 |
| C7 | connector-gateway (U6) | oxpilot-tools (U5) | Rust API shared-schema | U6 |
| C8 | session-core (U2) | U3, U6 | `on_session_delete` hook shared-schema | U2 |

## C1 — `oxpilot.*` / `admin.oxpilot.*` typed RPC

SessionOnly for all `oxpilot.*` (PAT refusal pinned by closed-set test);
`admin.oxpilot.*` is admin-session scoped. Errors are typed RPC error
codes, not HTTP bodies.

```yaml
shared-schema:
  families:
    oxpilot.status:        # enablement + provider summary for the flag-off gate
      returns: { enabled: bool, provider: {id, model} | null, reason: string | null }
      errors: []
      gate_exempt: true    # callable while disabled — it IS how the UI learns the reason
    oxpilot.session_*:     # session_list, session_create, session_get, session_delete
      payload: { id, user_id, repo_id, branch, title, status, created_at, last_activity_at }
      errors: [forbidden, not_found, oxpilot.disabled, oxpilot.internal]
      # `rpc.bad_input` (malformed fields) and `db.not_configured` remain
      # generic proc-layer codes outside this per-family list.
    oxpilot.message_queue: # queue a follow-up while a turn streams
      payload: { session_id, content } -> { queued_id, position }
      errors: [forbidden, budget_exceeded, session_active_turn_absent]
    oxpilot.turn_cancel:   # abort the live turn (terminal frame follows)
      payload: { session_id } -> { aborted: bool }
      errors: [forbidden, no_active_turn]
    oxpilot.diff_*:        # diff_list, diff_get, diff_apply (confirm → branch+PR)
      payload: ProposedDiff { id, session_id, base, files[{path, patch}], status }
      errors: [forbidden, not_found, diff_not_pending]
    oxpilot.target_*:      # target_list, pairing_create, target_disconnect, target_revoke
      payload: Target { id, label, status: online|offline|revoked, granted_caps[] }
      errors: [forbidden, not_found, pairing_expired, oxpilot.disabled]
      # amendment (U6 functional-design, flagged per deviation rule —
      # additive-only, consumer-proposed):
      # - Target gains `lifecycle: pending_consent|connected|
      #   disconnected|revoked` (additive field; `status` stays the
      #   pinned 3-value liveness enum — pending_consent/disconnected
      #   map to `offline`, revoked to `revoked`) and `pairing_id`
      #   (the mint it came from — pairs the pairing→target
      #   correlation the connect dialog needs).
      # - pairing_create returns {pairing_id, pairing_code, expires_at}
      #   — pairing_id is the correlation handle (the plaintext code
      #   remains shown-once).
      # - ADDED proc `oxpilot.target_get{target_id} -> TargetView`
      #   (errors [forbidden, not_found, oxpilot.disabled]) — the
      #   interaction-spec's connect dialog polls it; additive to the
      #   pinned family.
    oxpilot.grant_*:       # grant_list, grant_respond (allow/deny, never "always")
      payload: Grant { id, kind: connect|action, capability, args_summary, state, expires_at }
      errors: [grant_not_pending, forbidden]
      # amendment (U6 functional-design, flagged per deviation rule):
      # - grant_respond gains optional `scope: once|session` — the
      #   once-vs-session choice is the user's at consent time
      #   (interaction-spec ConsentCard); pending action grants mint
      #   unflavored and stamp kind at respond. Absent scope defaults
      #   to `once`.
      # - grant_* errors gain `not_found` — the anti-enumeration
      #   convention resolves cross-user hits to not_found identical
      #   to absent rows (forbidden stays for the remaining pin).
    oxpilot.usage_get:     # per-user budget view
      returns: { period, tokens_used, tokens_cap, requests_used, requests_cap }
      errors: [forbidden]
    oxpilot.grant_get:     # fetch a single grant (consent card ref target)
      payload: { grant_id } -> Grant
      errors: [forbidden, not_found]
    oxpilot.queue_list:    # fetch queued follow-ups (queued card ref target)
      payload: { session_id } -> { queued: [QueuedMessage] }
      errors: [forbidden, not_found]
    admin.oxpilot.settings_get / settings_set:   # admin-session scoped
      payload: { enabled_override: tri-state, provider_id, model, base_url,
                 key_set: bool, key_last4: string|null, user_budget, instance_budget }
      errors: [forbidden, invalid_provider]
      notes: key material is write-only; reads return key_set + last4 only (admin.mcp.* precedent)
    admin.oxpilot.providers_list:
      returns: [ { id, models: [string], configured: bool } ]
      errors: [forbidden]
  notes:
    - Secret material (provider keys, pairing codes post-claim, verifier
      hashes) is never present in any payload.
    - Every proc EXCEPT `oxpilot.status` checks the enablement gate
      first: flag/provider absent → `oxpilot.disabled` before any other
      logic. `oxpilot.status` is gate-exempt — it is how the flag-off UI
      learns `reason`.
```

## C2 — SSE chat routes

`POST /api/oxpilot/sessions/{id}/chat` (session-authed, body
`{content, client_msg_id}`) starts a turn and streams frames.
`GET /api/oxpilot/sessions/{id}/chat/stream` resumes with a snapshot.
No `Last-Event-ID`; exactly-once is by `message_id` + `seq`.

```yaml
sse-contract:
  frames:
    snapshot: { type: snapshot, messages: [Message+ToolCall rows], pending_queue: [QueuedMessage], active_turn: {turn_id, started_seq} | null }
    append:   { type: append, message_id, seq, role, delta }            # token deltas; seq is per-message monotonic
    tool:     { type: tool_call, message_id, tool: string, args_summary, status: running|done|error }
    card:     { type: card, card_kind: consent|diff|queued, ref_id }    # client fetches ref via C1 procs
    terminal: { type: terminal, message_id, result: done|error|aborted, code?: string }
    ping:     { type: ping }                                            # keep-alive ≥ every 30 s
  rules:
    - A stream ends only on a terminal frame (or transport close → client
      reconnects to the GET route and replaces state with the snapshot).
    - Frames after terminal are never emitted; provider mid-stream
      failure still produces terminal{result: error, code}.
    - Non-2xx pre-stream failures return typed JSON errors, not SSE.
      Full vocabulary: 401 unauthenticated · 403 forbidden |
      oxpilot.disabled · 404 session · 409 turn_active (a turn is
      already streaming — the client calls `oxpilot.message_queue`
      instead; auto-enqueue and silent accept are both forbidden) ·
      422 invalid_body · 429 budget_exceeded ·
      500 oxpilot.internal (infrastructure failure — the escape hatch
      every closed vocabulary still needs).
    - `card{card_kind: consent}` resolves via `oxpilot.grant_get`,
      `card{card_kind: queued}` via `oxpilot.queue_list`,
      `card{card_kind: diff}` via `oxpilot.diff_get`.
```

## C3 — Connector HTTP protocol

Dedicated routes (verifier-class Bearer auth, gateway-verified hash —
never session or PAT). Outbound-only from the connector's view. The
block below is a **contract-shape sketch in OpenAPI notation** — the
binding semantics are the request/response fields and rules; U7's
code generation emits the real handlers plus a valid `openapi.yaml`
as a code artifact.

```yaml
openapi: 3.0.3
info: { title: Oxpilot Connector, version: "1" }
paths:
  /api/oxpilot/connector/pair:
    post:
      summary: Exchange a single-use pairing code for connector credentials
      requestBody: { pairing_code: string, device_label: string, requested_caps: [exec_shell, fs_read, fs_write] }
      responses:
        '200': { connector_id, target_id, verifier }                  # shown once, stored by the connector
        '4xx': { error: pairing_expired | pairing_used | forbidden }
  /api/oxpilot/connector/claim:
    post:
      summary: Long-poll for actions on this target
      requestBody: { target_id, wait_seconds: <=60, capabilities }
      responses:
        '200': { actions: [{ action_id, kind: exec_shell|fs_read|fs_write, args, grant_id }], cancel: [action_id], retry_after: seconds }
        '204': timeout — re-poll; the hint rides the `Retry-After` header
               (204 has no body). `wait_seconds > 60` is clamped to 60
               server-side, not rejected.
  /api/oxpilot/connector/report:
    post:
      summary: Idempotent action result report (keyed by action_id)
      requestBody: { action_id, status: done|failed, exit_code?, output: string }   # secrets redacted client-side too
      responses: { '200': { ack: true } }
  /api/oxpilot/connector/heartbeat:
    post:
      summary: Liveness + revocation/cancel propagation
      requestBody: { target_id }
      responses: { '200': { revoked: bool, cancel: [action_id] } }
```

Connector rules pinned in requirements (FR6.*): single-use pairing codes
shown once; verifier material stored hashed server-side; revoke/cancel
sets MUST be honoured before starting and during execution of an action.

## C4 — `LlmProvider` trait (U1 → U3, U4)

```yaml
shared-schema:
  trait: |
    trait LlmProvider: Send + Sync {
      fn id(&self) -> &'static str;                 # "openai" | "anthropic" | "ollama" | "stub"
      async fn stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError>;
    }
    struct ChatRequest { model: String, system: String, messages: Vec<ChatMsg>, tools: Vec<ToolSpec> }
    enum ChatStreamItem { Delta(String), ToolCall{ id, name, args_json }, Done{ usage }, Error{ code, msg } }
  slot: "ctx.oxpilot_provider() -> Option<Arc<dyn LlmProvider>>"   # None ⇒ oxpilot.disabled
  rules:
    - U1 ships `StubProvider`; U4 replaces the slot via RwLock rebuild.
    - ProviderError carries a stable `code` the runtime maps to terminal frames.
```

## C5 — `SessionService` API (U2 → U3, U6, U8)

```yaml
shared-schema:
  api: |
    create_session / get_session(scoped user) / list_sessions / delete_session
    append_message / append_tool_call / transcript(session_id) -> Vec<Message>
    enqueue / drain_queue(session_id) -> Vec<QueuedMessage>
    record_usage / check_budget(user_id, caps: BudgetCaps) -> Result<(), BudgetExceeded>
    save_proposal / proposal(id) / mark_applied(id, pr_ref)
    # amendment (U2 nfr-design, flagged per deviation rule):
    # - check_budget gains the resolved caps param — budget values
    #   live on U4's EffectiveProviderConfig, unreachable from inside
    #   the stateless service; callers read slot.budget_caps.
    #   BudgetExceeded maps to AppError{code:"budget_exceeded",
    #   data:{scope}} (the repo's error type is AppError, no
    #   BudgetExceeded type exists).
    # - mark_applied gains pr_ref — WF8.3 records the materialized PR.
    # - enqueue returns QueuedReceipt {queued_id, position} (the
    #   C1-pinned wire shape; position is the ordinal
    #   COUNT(queue_seq < mine)+1, not queue_seq).
    # - ADDED seam verbs the wire needs: find_message_by_client_msg_id
    #   (dedupe), set_message_content / set_message_status (assistant
    #   streaming-row lifecycle — the shipped path inserts a
    #   `streaming` row with a caller-minted id then rewrites
    #   content/status mid-turn), list_tool_calls (C2 snapshot merge),
    #   set_tool_call_status (BR7.3 transitions), usage_view
    #   (oxpilot.usage_get backing read that must NOT refuse when
    #   over budget — it IS the spend surface; its caps params fill
    #   the C1 payload's cap fields — single writer).
    # - DTO pins: AppendMessage carries caller-minted {id, status};
    #   ToolCallWrite carries caller-supplied {seq (owning message's
    #   positional copy), call_index (per-message ordinal)}.
    # - codegen reconciliation (U2 review): `drain_queue` returns
    #   `Vec<Message>` — the appended transcript rows in delivery order
    #   (the queue rows are consumed; the rows the caller needs ARE the
    #   transcript inserts). `set_message_content`/`set_message_status`
    #   are id-keyed `(message_id)` — the service verifies the row's
    #   `session_id` for ownership, so a session param would be
    #   redundant. `append_tool_call`/`mark_applied` return their rows.
    #   ADDED `list_messages(user_id, session_id)` — the UNFILTERED
    #   transcript read (`streaming` included) the SSE snapshot needs so
    #   a remount overlays the live tail onto the in-flight assistant
    #   row; `transcript` stays BR2.3-filtered for prompt consumers.
    # All are service-internal signature refinements, not wire-payload
    # changes — additive under the ownership rules.
  rules:
    - All reads/writes take user_id; cross-user ⇒ forbidden at this layer too.
    - Secrets redaction happens on write — nothing leaves this service unredacted.
```

## C6 — `ToolExecutor` / `ToolCard` (U3 → U5)

```yaml
shared-schema:
  trait: |
    trait ToolExecutor: Send + Sync {
      fn catalog(&self) -> Vec<ToolSpec>;                       # names + json-schemas + read|write class
      async fn invoke(&self, call: ToolCall, ctx: ToolCtx) -> Result<ToolOutput, ToolError>;
    }
    struct ToolCtx { session_id, user_id, repo_id, proposal_id? }   # identity-bound (ADR-008)
  card: "ToolCard { tool, args_summary, status, output_summary } — what the SSE `tool` frame renders"
  rules:
    - write-class tools emit audit rows; read-class do not.
    - exec_shell/fs_* are write-class and route through C7, never direct.
```

## C7 — Action-queue dispatch (U6 → U5)

```yaml
shared-schema:
  api: |
    dispatch_action(session_id, target_id, kind: exec_shell|fs_read|fs_write, args) -> Result<Action, Denied>
    action_status(action_id) -> ActionView { action_id, kind, state, created_at, args_summary }
    # ActionView is the public trim — U5 never sees connector
    # credentials or raw Action internals (verifier refs, claim cursors).
    # amendment (U6 functional-design, flagged per deviation rule — the
    # pinned shape cannot implement the pinned owner checks):
    # - dispatch_action(user_id, session_id, target_id, kind, args) —
    #   user_id leads (U5's ToolCtx already carries it); BR4.1's
    #   get_session + target-owner checks need it and C5 has no
    #   unscoped session read.
    # - action_status(user_id, action_id) — owner-checked read.
    # - Denied shape pinned: { reason: not_connected|offline|
    #   capability|consent_required, grant_id? } — consent_required
    #   carries grant_id for the consent-card ref; the others carry
    #   none.
    # - ActionView gains `result_summary` (additive — the completed
    #   action's bounded redacted excerpt; absent until terminal).
  rules:
    - dispatch_action enforces the Grant check; absent/expired grant ⇒
      Denied and a pending action-grant row is created for the consent card.
    - Action.state ∈ queued|claimed|running|done|failed|cancel_requested|cancelled.
```

## C8 — `on_session_delete` hook (U2 → U3, U6)

```yaml
shared-schema:
  hook: |
    trait SessionDeleteHook: Send + Sync {
      async fn on_session_delete(&self, session_id, user_id) -> Result<()>;
    }
    # SessionService calls registered hooks BEFORE cascade, in
    # registration order (U3 abort first, U6 revoke second); a hook
    # failure aborts the delete. Hooks are idempotent — re-entry is
    # a no-op. The API proc performs no turn logic itself; this hook
    # is the single enforcement point for ADR-010's abort-before-delete.
  registrars:
    - U3: abort live turns (terminal frame + cancel token) — ADR-010.
    - U6: revoke/cancel all grants+actions for the session — audited transitions.
```

## Contract ownership rules

- The provider-side unit owns each spec (table's Owner column); changes
  to a spec are proposed by the consumer but committed by the owner.
- Additive changes are always safe: consumers ignore unknown fields in
  every payload above (RPC DTOs, SSE frames, OpenAPI bodies).
- Breaking changes = new proc name (`oxpilot.diff_apply_v2`), new route
  path (`/chat/v2/stream`), or new trait method with default — never a
  silent shape change. `versioning.toml` SemVer pins each crate.
- Code Generation treats this file as the normative seam list; any
  deviation is a review finding, not a local choice.

## Open questions

| Contract | Question | Blocks |
|---|---|---|
| C1 | Exact repo-read tool names vs. existing `vcs.*`/`actions.*` procs — resolved during U5 functional-design against the live dispatch registry | oxpilot-tools |
| C2 | Ping interval & max stream lifetime under oxidean-web's proxy limits — set during U1 build against real timeout config | skeleton-slice |
