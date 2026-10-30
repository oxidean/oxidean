# Entities — oxpilot-tools (U5)

## Entity model

```yaml
entities:
  - name: OxpilotTools
    kind: in-memory executor seam (concrete `ToolExecutor`
      implementation; `Arc<dyn ToolExecutor>` on
      `OxpilotRuntimeSlot.tools`)
    provenance: new this unit — the single struct the runtime pins
      into `turn_parts()` at turn start (runtime rebuilds carry the
      same Arc across provider/settings swaps, oxpilot.rs:1046).
    description: >
      The tool catalog plus its dispatch registry. Owns an `AppState`
      clone and reaches every oxpilot port through it:
      `state.oxpilot.sessions()` (SessionService),
      `state.oxpilot.gateway()` (ConnectorGateway),
      `state.oxpilot.turns()` (live-turn map for `emit_frame`) —
      `state.sessions` is the AUTH SessionService and is NEVER the
      right handle. `emit_frame` is `pub(crate)` and takes
      `&ActiveTurn`, and `invoke(call, ctx)` hands the tool no turn
      handle, so `OxpilotTools` lives inside `oxidean-api-oxpilot`
      and the emit path is `turns().get(ctx.session_id)` (BR4.5).
      Plus a static `[CatalogEntry]` table; `catalog()` returns the
      `ToolSpec` halves in registry order; `invoke(call, ctx)` looks up
      `call.name` (absent ⇒ `ToolError{unknown_tool}`), validates args
      against the entry's JSON Schema, re-resolves the session, then
      runs the entry's handler against the bound `ToolCtx`.
    attributes:
      - { name: state, type: "AppState", required: true, constraints: ["carries db, rpc_dispatch, git/vcs handles + the `state.oxpilot` port (sessions()/gateway()/turns()) — `state.sessions` is the AUTH SessionService, not the oxpilot seam; the executor never opens its own connections"] }
      - { name: registry, type: "&'static [CatalogEntry]", required: true, constraints: ["compiled-in, closed — adding a tool is a code change reviewed against the allowlist rules; nothing loads tool definitions from data"] }
    constraints:
      - One executor instance serves every session — all per-call
        scoping lives in `ToolCtx` + `args`, never in executor state.
      - "`catalog()` is pure: same registry, same output, every turn."

  - name: CatalogEntry
    kind: in-memory static row (registry element)
    provenance: new this unit — splits the shipped `ToolSpec`'s three
      wire fields from the two things the wire struct does not carry
      (class + handler).
    description: >
      One model-callable tool: the provider-facing spec, its
      read|write class, and the handler that executes it. The class
      field is the executor's internal authorization/audit partition —
      C6's sketch lists `class` on `ToolSpec` but the shipped struct
      carries only `{name, description, parameters_json}` (oxpilot.rs:58),
      so the class rides beside the spec inside the registry, never on
      the wire.
    attributes:
      - { name: spec, type: ToolSpec, required: true, constraints: ["name ∈ flat `domain_verb` ids matching `^[a-zA-Z0-9_-]{1,64}$` (provider-legal on OpenAI/Anthropic — dotted proc names are not); `parameters_json` is a JSON Schema the adapter serializes verbatim"] }
      - { name: class, type: "enum", required: true, allowed_values: ["read", "write"], constraints: ["write ⇒ audit row post-commit (BR9.1); read ⇒ none — the C6 rule verbatim"] }
      - { name: handler, type: "fn(&OxpilotTools, ToolCtx, Value) -> Result<ToolOutput, ToolError>", required: true }
    constraints:
      - The name→proc binding is fixed data in the entry, not derived
        from args — a model can never steer a tool to a different
        backing procedure.
      - "`exec_shell`/`fs_read`/`fs_write` names are the C7 `kind`
        vocabulary verbatim, so dispatch passes `entry.spec.name`
        straight into `dispatch_action`'s `kind`."

  - name: ToolCtx
    kind: consumed C6 struct (per-invocation identity binding)
    provenance: C6; minted by U3's `invoke_call` with the turn's
      session user — `proposal_id` is `None` on every shipped call
      (runtime.rs:460).
    description: >
      The scoped identity a tool call executes under. `session_id` +
      `user_id` come from the OWNED session row, not the model; every
      handler must re-resolve `svc.get_session(user_id, session_id)`
      to bind `repo_id`/`owner`/`name`/`branch` and reject dead
      sessions — args can never rebind user, session, or repository.
    attributes:
      - { name: session_id, type: string, required: true }
      - { name: user_id, type: string, required: true }
      - { name: repo_id, type: string, required: true, constraints: ["copied from the session row at mint; handlers re-resolve rather than trust it"] }
      - { name: proposal_id, type: "string|null", required: false, constraints: ["always None today (gap G-2) — tools that need a proposal take it as a validated arg"] }
    constraints:
      - Treated as the ONLY identity authority in a call — any
        `user_id`/`session_id`/`repo_id`-shaped field inside
        `args_json` is ignored or rejected (BR2.3).

  - name: ProposedDiff
    kind: persisted row (`oxpilot_proposals`, migration 0087) + C1
      wire payload
    provenance: existing table + U2's service verbs — the
      `SessionService` names are `save_proposal`, `proposal`,
      `mark_applied(user_id, proposal_id, pr_ref)` (oxpilot.rs:492–509);
      `insert_oxpilot_proposal`/`find_oxpilot_proposal`/
      `mark_oxpilot_proposal_applied` are the DB-layer verbs beneath
      them. U5 adds the write path, the `oxpilot.diff_*` procs, and
      (Q3-B) the additive `ops_json` chain — NEW migration
      `NNNN_oxpilot_proposals_ops.sql` in all three dialects (applied
      0087 is never edited — migrations are append-only) +
      `NewOxpilotProposal` + `OxpilotProposalRow` + `ProposalWrite` +
      the `save_proposal` impl.
    description: >
      The reviewable unit of agent-authored change: a pending
      canonical unified diff plus the file ops that materialize it.
      Nothing touches git until `diff_apply`; `files[]` on the wire
      derives by splitting `diff_body` on `diff --git` headers (the
      table's own convention).
    attributes:
      - { name: id, type: string, required: true, constraints: ["service-minted — `save_proposal` does `Uuid::new_v4()` internally and `ProposalWrite` has no id field; the handler learns the id from the returned row (`row.id`)"] }
      - { name: session_id, type: string, required: true, constraints: ["ownership boundary — `svc.proposal`/`svc.mark_applied` are USER-scoped (prove the row's session belongs to the user, not that it IS this session); model-facing reads additionally pin `row.session_id == ctx.session_id` post-fetch ⇒ `not_found` (BR4.6); the user proc `diff_apply` deliberately accepts same-user-other-session proposals (BR5.2)"] }
      - { name: title, type: string, required: true, constraints: ["bounded ≤ 200 chars; also seeds the PR title"] }
      - { name: base, type: string, required: true, constraints: ["the session's bound branch — not a model arg in v1 (BR4.4); NOT redacted — it is an identifier"] }
      - { name: diff_body, type: string, required: true, constraints: ["canonical unified diff text ≤ DIFF_BODY_MAX; redacted INSIDE `save_proposal` via `oxidean_core::redact` (free function — no `svc.redact` exists) like every user-visible body"] }
      - { name: ops_json, type: "string (JSON)", required: true, constraints: ["serialized [DiffFileOp] the apply path executes verbatim — additive column landing via a NEW migration in all three dialects (Q3-B); never shown raw on the wire — diff_get renders files[] from diff_body"] }
      - { name: status, type: "enum", required: true, allowed_values: ["pending", "applied"], constraints: ["only `pending` rows are applyable — `diff_apply` on anything else ⇒ `diff_not_pending` (enforced inside the service; the proc layer re-checks)"] }
      - { name: pr_ref, type: "string|null", required: false, constraints: ["set once by `svc.mark_applied`; format `owner/name#number` (Q10-A)"] }
      - { name: created_at, type: rfc3339, required: true }
    constraints:
      - Model writes set `status = pending` unconditionally — the model
        cannot mint an `applied` row or pre-fill `pr_ref`.
      - One session can hold many pending proposals — `diff_list`
        returns them newest-first; supersede-by-new-proposal is the
        model's only edit path (no in-place diff update tool).

  - name: DiffFileOp
    kind: persisted (element of ProposedDiff.ops_json) + `propose_diff`
      args element
    provenance: new this unit — the materialization instruction set
      that the diff text alone cannot express (deletes, exact post-
      image bytes).
    description: >
      One file mutation the apply path performs through `repo.file.*`
      procs. The propose handler validates the op set against
      `diff_body` headers (path-set coherence — BR4.3) so what the
      user reviews is what `diff_apply` writes.
    attributes:
      - { name: path, type: string, required: true, constraints: ["repo-relative, no `..`, no absolute, no `.git/` prefix — the shipped `normalize_edit_path` enforcement inside each `repo.file.*` proc"] }
      - { name: op, type: "enum", required: true, allowed_values: ["create", "update", "delete", "rename"], constraints: ["rename requires `new_path`; create/update require `content_b64`"] }
      - { name: new_path, type: "string|null", required: false }
      - { name: content_b64, type: "string|null", required: false, constraints: ["exact post-image bytes, base64 — the apply path never re-derives content from diff hunks"] }
    constraints:
      - "Bounded: ≤ MAX_PROPOSAL_FILES (50) ops and ≤ DIFF_BODY_MAX summed
        decoded content — enforced before insert (BR4.4)."
      - "ops→proc field mapping (BR5.3): `content_b64` →
        `repo.file.upload`'s `files[].content_base64`; `path` →
        `repo.file.rename`'s `from_path`, `new_path` → `to_path`;
        `path` verbatim on delete/upload; all ops ride the
        `oxpilot/proposal-<id8>` branch."

  - name: ArtifactScope
    kind: derived in-memory set (per `pr_comment` invocation)
    provenance: FR7.5/AC5.5.2 — "only on artifacts the agent itself
      created with user approval"; v1's creatable artifact set is
      exactly applied proposals' `pr_ref`s.
    description: >
      The allow-set `pr_comment` checks: `pr_ref` values of THIS
      session's `applied` proposals, resolved fresh per call. A number
      outside the set ⇒ `ToolError{forbidden}` — the model cannot
      comment on any issue/PR it did not materialize. Every entry
      names a pull request — the forge shares the issue NUMBER
      COUNTER (`allocate_next_issue_number`) but PRs live in
      `pull_requests`/`pull_comments`, so the comment posts through
      `pull.comments.create` unanchored (BR8.2), never the
      issues-table path.
    attributes:
      - { name: allowed_pr_refs, type: "set<string>", required: true, constraints: ["`owner/name#number` entries from the session's applied proposals only"] }
    constraints:
      - "Session-scoped: a proposal another session applied (even the
        same user/repo) is outside the set."
      - Recomputed per call — a just-applied proposal joins the set on
        the next invocation, no cache to invalidate.

  - name: ActionView
    kind: consumed C7 struct (status read)
    provenance: C7 `action_status(&ClientMeta::default(), user_id,
      action_id)` return. `dispatch_action` instead returns the full
      `OxpilotActionRow` (db/oxpilot.rs:2206–2222 — including stored
      redacted `args` and `grant_id`); the dispatch tool output reads
      `id`/`state` off that row — `action_id` IS the row's `id` field.
    description: >
      The read-back the `action_status` tool surfaces. Deliberately
      thin — no credential material, no raw args (only
      `args_summary`), no claim cursor.
    attributes:
      - { name: action_id, type: string, required: true }
      - { name: kind, type: "enum", required: true, allowed_values: ["exec_shell", "fs_read", "fs_write"] }
      - { name: state, type: "enum", required: true, allowed_values: ["queued", "claimed", "running", "done", "failed", "cancel_requested", "cancelled"], constraints: ["shipped 7-state vocabulary (oxpilot.rs:655–656) — treated as OPEN: an unrecognized state must render, never choke the tool"] }
      - { name: args_summary, type: string, required: true, constraints: ["gateway-redacted ≤ SUMMARY_MAX (512 B — oxpilot.rs:542; NOT the 2 KiB ARGS_SUMMARY_MAX, which bounds tool-call cards)"] }
      - { name: result_summary, type: "string|null", required: false, constraints: ["populated on terminal states only; bounded ≤ RESULT_MAX (2 KiB) + redacted server-side"] }
      - { name: created_at, type: rfc3339, required: true }
    constraints:
      - The tool output passes the view through verbatim — U5 adds no
        fields and performs no un-redaction.

  - name: DeniedOutcome
    kind: consumed C7 error shape (`AppError.data.denied`)
    provenance: >
      U6-BR4.4 — refusals ride `AppError{code: oxpilot.action_denied,
      data: {denied: {reason, grant_id?}}}`; U5 matches on the
      `data.denied` marker, never on message text or error type.
    description: >
      The structured refusal U6 returns before insert. U5 maps the
      four reasons into two tool outcomes — `consent_required` is a
      success-shaped `ToolOutput` (+ emitted consent card) because a
      pending grant is progress; the other three are `ToolError`s the
      model reads as hard refusals (AC4.3.4's in-session error).
    attributes:
      - { name: reason, type: "enum", required: true, allowed_values: ["not_connected", "offline", "capability", "consent_required"] }
      - { name: grant_id, type: "string|null", required: false, constraints: ["present only on consent_required — becomes the emitted card's ref_id"] }
    constraints:
      - An `oxpilot.action_denied` error WITHOUT the `data.denied`
        marker, or with an unrecognized reason, is treated as a hard
        `ToolError` — never parsed leniently (fail closed).

  - name: ToolAuditRecord
    kind: persisted `audit_events` row (via
      `audit::record_with(&state.db, &ClientMeta::default(), actor,
      event, target, detail)` — the ctx-free entry point; the tool
      holds `ToolCtx`, not `RpcCtx`)
    provenance: existing `audit` crate + `oxpilot.*` event-name
      convention; U5 owns the propose/apply/comment rows while U6
      owns the consent/dispatch/result lifecycle rows.
    description: >
      The write-class audit emission (C6: "write-class tools emit
      audit rows; read-class do not"). Post-commit, best-effort —
      `audit::record_with` swallows its own failure rather than failing
      the tool call after the effect already landed. `actor` is
      `Option<(user_id, username)>`: the tool holds only
      `ToolCtx.user_id`, so it resolves `Some((user_id, username))`
      via a `find_user_by_id` lookup and falls back to `None` rather
      than skipping the event (BR9.1).
    attributes:
      - { name: event, type: string, required: true, constraints: ["`oxpilot.proposal_created`, `oxpilot.diff_applied`, `oxpilot.artifact_comment` — exec/fs dispatch coverage is U6's `oxpilot.action_dispatched`; U5 does not double-audit it"] }
      - { name: actor, type: "Option<(user_id, username)>", required: true, constraints: ["`Some((ctx.user_id, username))` when `find_user_by_id` resolves; `None` fallback — never blocks the event"] }
      - { name: detail, type: "json", required: true, constraints: ["bounded, redacted via `oxidean_core::redact` — session_id, proposal_id/target kind, file count or artifact ref; NEVER diff bodies, file contents, command strings, or tool args beyond summary length"] }
    constraints:
      - Emitted AFTER the mutation commits — auditing a failed write
        fabricates an event that never happened.
      - Read-class tools emit nothing — reads are already visible via
        the durable tool-call card row U3 persists.

  - name: InternalRpcCall
    kind: in-memory dispatch hop (ctx.rpc_dispatch reuse)
    provenance: components.md — "Read tools route through
      rpc_dispatch (never direct DB/git)"; `build_rpc_ctx_with_session`
      (app.rs:537) is the shipped seam that mints a session-equivalent
      `RpcCtx` from an already-resolved session.
    description: >
      How a repo/issue/pull/actions read reaches its backing proc:
      the handler builds `RpcRequest{procedure, input}` with
      `owner`/`name` filled from the resolved session's repo binding,
      synthesizes an `RpcCtx` bound to `ToolCtx.user_id` +
      `ClientMeta::default()` (no fabricated IP/UA), and invokes
      `state.rpc_dispatch`. The proc's own authorization (repo
      membership/read ACL, feature flags, archived checks) applies
      verbatim — the tool layer adds nothing and bypasses nothing.
    attributes:
      - { name: procedure, type: string, required: true, constraints: ["one of the pinned allowlisted names in BR3.2/BR6.2 — never taken from args"] }
      - { name: input, type: "json", required: true, constraints: ["model-supplied fields validated by the entry schema; owner/name/ref defaults injected server-side from the session binding"] }
    constraints:
      - A synthesized-session ctx means `require_verified`-class proc
        guards still run — the tool does not pre-clear proc-level
        authorization, it relies on it.
      - Proc `AppError`s map into `ToolError` verbatim by code
        (not_found stays not_found) — no error-code laundering that
        would leak or garble proc semantics. `code` is internal/
        test-visible only — the model receives the redacted `message`
        via `invoke_call`, so actionable guidance rides
        message/content strings (BR3.4/BR7.3).
```

## Observations

- **`ToolSpec` ships three fields, not four.** C6's sketch lists a
  `class` member; the shipped wire struct is `{name, description,
  parameters_json}` (oxpilot.rs:58). U5 keeps the class in
  `CatalogEntry` (internal) rather than widening the C6 wire — the
  provider must never see the class, and a wire-visible class invites
  nothing but confusion. This is deviation D-01, already accepted in
  the C6 contract note ("spec shape carried verbatim").
- **`ToolCtx.proposal_id` is dead in v1 (G-2).** `invoke_call` mints
  `None` unconditionally (runtime.rs:460). Every tool that needs a
  proposal takes `proposal_id` as an explicit argument and validates
  ownership via `svc.proposal(user_id, proposal_id)` — which is
  USER-scoped (`owned_proposal` proves the row's session belongs to
  the user, not that it is THIS session, services.rs:140–153), so
  session-scoped reads add the post-fetch `row.session_id ==
  ctx.session_id` ⇒ `not_found` pin (BR4.6). Ambient binding
  is a U3 runtime change deferred until a consumer needs it (Q8-A).
- **`ops_json` is the one additive column.** The 0087 table's
  `diff_body` cannot express deletes or carry post-image bytes; the
  apply path needs verbatim ops. The column is internal-only — C1's
  `ProposedDiff` wire shape is unchanged (`files[]` still derives from
  the diff text). It lands as a NEW migration
  (`NNNN_oxpilot_proposals_ops.sql` ×3 dialects — applied migrations
  are append-only, 0087 is never edited) plus `NewOxpilotProposal`,
  `OxpilotProposalRow`, `ProposalWrite`, and the `save_proposal`
  impl.
- **The registry is the allowlist.** There is no "read subset of the
  Actions proc surface" filter — the six Actions entries and the
  repo/issue/pull entries are the ONLY names that can ever dispatch;
  control procs (`approveRun`, `rerunRun`, `cancelRun`, `dispatch*`,
  `delete*`, secrets/variables/environment writers) are absent, not
  blocked.
- **`pr_ref` is `owner/name#number`.** Server-side correlation, no URL
  dependency — U8 renders it linkable; `ArtifactScope` matches on the
  same string.
