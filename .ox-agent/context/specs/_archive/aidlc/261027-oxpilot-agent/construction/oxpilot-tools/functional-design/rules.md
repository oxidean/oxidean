# Business Rules — oxpilot-tools (U5)

## Rule set

```yaml
rules:
  # ---- BR1 — catalog closure & registration ----
  - id: BR1.1
    kind: constraint
    text: >
      The catalog is a compiled-in `&'static [CatalogEntry]` — closed,
      fixed-order, identical for every session/provider. `catalog()`
      returns the `ToolSpec` halves verbatim; nothing loads tool
      definitions from data, config, or provider output. Adding or
      removing a tool is a reviewed code change against the BR3.2/BR6.2
      allowlists — never a runtime filter over a larger proc set.
    violation_behaviour: a name absent from the registry cannot
      dispatch by construction (BR1.3 covers the reject shape).
    applies_to: OxpilotTools.
    source: FR7.1–FR7.5, NFR9 (registration coverage).

  - id: BR1.2
    kind: constraint
    text: >
      Tool names are flat `domain_verb` ids matching
      `^[a-zA-Z0-9_-]{1,64}$` — provider-legal on every adapter (dots
      are outside OpenAI/Anthropic tool-name charsets, so proc names
      are never the tool ids). `exec_shell`, `fs_read`, `fs_write`
      intentionally equal the C7 `kind` vocabulary verbatim so the
      handler passes `entry.spec.name` straight into
      `dispatch_action`'s kind parameter.
    applies_to: CatalogEntry.spec.name.

  - id: BR1.3
    kind: constraint
    text: >
      `invoke` with an unregistered `call.name` ⇒ `ToolError{code:
      unknown_tool}` — no schema validation, no handler, no dispatch,
      no audit row, no card. The card U3 persists for the call shows
      the error, so the transcript still records the attempt.
    violation_behaviour: fail closed; the model sees a normal tool
      error result it can react to.
    applies_to: invoke.

  - id: BR1.4
    kind: validation
    text: >
      `call.args_json` is validated against the entry's
      `parameters_json` JSON Schema BEFORE the handler runs — types,
      required fields, enums, string bounds, closed `additionalProperties`.
      Violations ⇒ `ToolError{invalid_args}` naming the offending field;
      the handler never sees unvalidated input.
    applies_to: invoke.
    source: NFR9.

  # ---- BR2 — invocation binding & authorization ----
  - id: BR2.1
    kind: authorization
    text: >
      Every handler re-resolves `svc.get_session(ctx.user_id,
      ctx.session_id)` before doing work — the session row is the live
      ownership + repo-binding + liveness check. Absent or foreign
      session ⇒ `ToolError{not_found}` (anti-enumeration: foreign and
      dead ids are indistinguishable). `ctx.repo_id` is advisory; the
      resolved row is authoritative.
    applies_to: all handlers.
    source: FR2.3, NFR1.

  - id: BR2.2
    kind: authorization
    text: >
      Repo-scoped tools inject `{owner, name}` into the backing proc's
      input from the RESOLVED session's repository binding — never from
      args. A `ref`/`branch` arg may only narrow WITHIN that repo (and
      defaults to the session branch when omitted); no arg can point a
      tool at another repo.
    applies_to: repo.*, issue.*, pull.*, repo.actions.* handlers.
    source: FR7.1, FR7.3.

  - id: BR2.3
    kind: constraint
    text: >
      `ToolCtx` is the sole identity authority. Any `user_id`,
      `session_id`, `repo_id`, or `target_id` (non-exec/fs tools) field
      inside `args_json` is rejected at schema validation
      (`additionalProperties: false`) — identity never crosses the
      args boundary. The exec/fs `target_id` arg is a TARGET selector
      only (BR7.3 still verifies it resolves to a user-owned target
      through the gateway's own checks).
    applies_to: all handlers.
    source: NFR1.

  - id: BR2.4
    kind: constraint
    text: >
      The read|write class lives on `CatalogEntry` (internal), not the
      wire `ToolSpec`. Write-class = {propose_diff, pr_comment,
      exec_shell, fs_read, fs_write} plus the user-proc `diff_apply`;
      every other entry is read-class. The class is fixed data —
      args can never reclassify a call.
    applies_to: CatalogEntry.
    source: C6 class rule.

  - id: BR2.5
    kind: authorization
    text: >
      Write-class handlers run their class-specific gate AFTER session
      resolution: propose_diff ⇒ session-owned pending-row insert only;
      pr_comment ⇒ ArtifactScope membership (BR8.1); exec/fs ⇒ the C7
      gateway's own session/target/caps/grant chain (U5 adds nothing,
      bypasses nothing); diff_apply ⇒ user-proc ownership + pending
      check (BR5.2). A failed gate ⇒ `ToolError{forbidden}` or the
      mapped denial — never a partial effect.
    applies_to: write-class handlers.
    source: FR3.2, FR5.2, FR7.4, FR7.5.

  # ---- BR3 — repository / issue / pull reads ----
  - id: BR3.1
    kind: constraint
    text: >
      Read tools reach their backing proc through
      `state.rpc_dispatch` on a synthesized-session `RpcCtx`
      (`build_rpc_ctx_with_session` with the ctx-bound user +
      `ClientMeta::default()` — no fabricated IP/UA). Never direct
      `oxidean-db` calls, never `GitBackend`/`vcs` handles: the proc's
      own authorization (repo read ACL, feature flags, archived
      checks, issue/pull gating) applies verbatim.
    applies_to: all read-class handlers.
    source: components.md "route through rpc_dispatch", FR7.1.

  - id: BR3.2
    kind: constraint
    text: >
      The backing-proc allowlist is closed and literal:
      `repo.get` (session-binding precheck), `repo.tree`, `repo.blob`,
      `repo.refs`, `repo.commits`, `repo.commit`,
      `issue.list`, `issue.get`, `pull.list`, `pull.get`.
      No other repo/issue/pull proc — including `pull.files`,
      `pull.commits`, `issue.comments.list`, `repo.search`,
      `repo.compare`, `repo.blame`, and EVERY `repo.*`/`pull.*`/
      `issue.*` writer — is reachable from the catalog.
    violation_behaviour: absence, not rejection — an unlisted proc
      name cannot appear in a CatalogEntry.
    applies_to: registry.
    source: FR7.1, FR7.3, FR7.5.

  - id: BR3.3
    kind: validation
    text: >
      Read-arg bounds: `path` normalized (repo-relative, no `..`, no
      leading `/`, ≤ 4 KiB); `ref`/`sha` string ≤ 256 chars defaulting
      to the session branch; pagination (`per_page`/`limit`/`skip`)
      capped at 100 rows; `repo.commits` limit ≤ 100. The schema makes
      these unrepresentable, not fixable-by-handler.
    applies_to: repo read handlers.
    source: NFR9.

  - id: BR3.4
    kind: constraint
    text: >
      Proc `AppError`s map into `ToolError` by code VERBATIM
      (`not_found`→`not_found`, `forbidden`→`forbidden`) — no error
      laundering that would leak which resource failed or garble proc
      semantics. Note the mechanism boundary: `ToolError.code` never
      reaches the model — `invoke_call` feeds back only the redacted
      `message` (runtime.rs:585–588), so `code` is card/internal/
      test-visible only and anything the model must act on rides the
      `message` string (BR7.3/BR7.4). Internal/unknown codes map to
      `ToolError{internal}` with a generic message.
    applies_to: all rpc_dispatch handlers.

  # ---- BR4 — ProposedDiff ----
  - id: BR4.1
    kind: constraint
    text: >
      `propose_diff{title, files[], diff_body}` inserts exactly one
      `oxpilot_proposals` row `status = pending` via
      `svc.save_proposal` + the `ops_json` column — the model can
      never set `applied` or `pr_ref`, and there is no in-place
      update path (a revised proposal is a new row).
    applies_to: propose_diff.
    source: FR3.1, AC2.1.1.

  - id: BR4.2
    kind: constraint
    text: >
      Proposing performs ZERO git mutation — no branch, commit, push,
      or PR at propose time. The stored row is review-only until the
      user runs `oxpilot.diff_apply`.
    applies_to: propose_diff.
    source: FR3.3, AC2.2.3.

  - id: BR4.3
    kind: validation
    text: >
      Path-set coherence: the set of `files[].path` (∪ `new_path` for
      renames) MUST equal the set of paths in `diff_body`'s
      `diff --git` headers — what the user reviews is what apply
      writes. Mismatch ⇒ `ToolError{invalid_args}` naming the
      disagreeing paths; the row is not inserted.
    applies_to: propose_diff.
    source: FR3.1.

  - id: BR4.4
    kind: validation
    text: >
      Proposal bounds: `title` ≤ 200 chars; `diff_body` ≤ DIFF_BODY_MAX
      (256 KiB); `files` ≤ MAX_PROPOSAL_FILES (50 — capped at the
      shipped `UPLOAD_MAX_FILES` bound so the BR5.3 single batched
      upload stays reachable); summed decoded
      `content_b64` ≤ DIFF_BODY_MAX; `base` is always the session's
      bound branch in v1 (not an arg). Overflow ⇒ `invalid_args`.
    applies_to: propose_diff.
    source: NFR9.

  - id: BR4.5
    kind: constraint
    text: >
      On insert success the handler emits
      `card{card_kind:"diff", ref_id: proposal_id}` on the live turn
      and returns `ToolOutput` "proposal <id> created — pending review".
      The emit path is `state.oxpilot.turns()` →
      `.get(ctx.session_id)` → `emit_frame(&ActiveTurn)` — the
      `message_queue` queued-card precedent (session.rs:331–341);
      `emit_frame` is `pub(crate)` and `invoke(call, ctx)` hands the
      tool no turn handle, so `OxpilotTools` must live inside
      `oxidean-api-oxpilot` (or the helper be widened). Emit-side race:
      if the turn already terminalled, `turns().get` finds nothing /
      `emit_frame` drops silently — the card is best-effort, a
      subsequent turn on the same session would get it; the durable
      tool-call row is the guaranteed record. The card is how U8
      renders the diff view (AC2.1.1 card) — emitted after commit,
      never before.
    applies_to: propose_diff.
    source: AC2.1.1, US2.3 (U8 renders).

  - id: BR4.6
    kind: constraint
    text: >
      `proposal_get{proposal_id}` (read-class) returns the session-
      owned row `{id, title, base, status, pr_ref, files[]}` —
      `files[]` derived by splitting `diff_body` on `diff --git`
      headers. `svc.proposal(user_id, proposal_id)` is USER-scoped —
      `owned_proposal` proves only that the row's session belongs to
      the user — so every session-scoped proposal read then pins an
      explicit post-fetch `row.session_id == ctx.session_id` check:
      foreign/absent/other-session id ⇒ `ToolError{not_found}`
      (anti-enumeration: same-user-other-session and dead ids are
      indistinguishable).
    applies_to: proposal_get.
    source: AC2.1.2 (patch view — model half; `oxpilot.diff_get`
      covers the UI half).

  # ---- BR5 — materialization (diff_apply) ----
  - id: BR5.1
    kind: constraint
    text: >
      `oxpilot.diff_apply{proposal_id}` is a USER proc — SessionOnly +
      `require_enabled` + `require_verified` + owned session, exactly
      like the other `oxpilot.*` writes. NO catalog entry maps to it:
      the explicit-confirm gate is that only the user's own proc call
      can reach this path (AC2.2.1/AC2.2.3 structural, not a prompt
      promise).
    applies_to: diff_apply.
    source: C1, FR3.2, AC2.2.1–AC2.2.3.

  - id: BR5.2
    kind: authorization
    text: >
      Apply resolves `proposal_id` → session → user via
      `svc.proposal(user_id, proposal_id)`/`svc.mark_applied`, which
      are USER-scoped: a `proposal_id` from ANOTHER session of the
      same user is reachable here — deliberate for the user proc (the
      repo binding and apply target come from the row's OWN session,
      so a user's proposal created in one session may be applied from
      another; the BR4.6 same-session pin applies only to the
      model-facing read path). Cross-user/absent ⇒ `not_found`;
      `status != pending` ⇒ `diff_not_pending`. The
      proc-bound user is the ONLY principal — no body field can
      substitute another identity.
    applies_to: diff_apply.
    source: C1 error vocabulary, AC2.2.1.

  - id: BR5.3
    kind: constraint
    text: >
      The apply sequence runs on the CALLER'S OWN `ctx.rpc_dispatch`
      (the user's real session ctx, not a synthesized one) in order:
      `repo.branchCreate{owner, name, branch: oxpilot/proposal-<id8>,
      start: base}` → file ops from `ops_json` (`repo.file.upload`
      batching create/update into ONE commit — a single upload call
      carries ≤ `UPLOAD_MAX_FILES` = 50 ops, the bound MAX_PROPOSAL_FILES
      is pinned to in BR4.4; `repo.file.delete` and
      `repo.file.rename` per op) → `pull.create{base_ref: base,
      head_ref: <branch>, title, body}`. The ops→proc field mapping
      is: op `content_b64` → `repo.file.upload`'s
      `files[].content_base64`; op `path` → `repo.file.rename`'s
      `from_path`, op `new_path` → its `to_path` (and `path` is the
      delete/upload path verbatim); all file ops ride the
      `oxpilot/proposal-<id8>` branch. Path rules are the shipped
      `normalize_edit_path` enforcement inside each `repo.file.*` proc
      — the apply path adds nothing of its own. Repo ACL, branch
      protection, and archived checks apply verbatim through the
      normal procs.
    applies_to: diff_apply.
    source: AC2.2.1 "via existing repo/PR RPC".

  - id: BR5.4
    kind: policy
    text: >
      The PR body is `{proposal title + summary}` plus the fixed
      provenance line `Produced by an /oxpilot agent session:
      <session_id> (proposal <proposal_id>).` — AC2.2.4's shipped
      product requirement, deliberately carved out from the repo's
      no-AI-attribution convention (which still forbids
      `Co-authored-by`/`Generated with` trailers even here —
      provenance is a session fact, not an author claim).
    applies_to: diff_apply.
    source: AC2.2.4.

  - id: BR5.5
    kind: constraint
    text: >
      On `pull.create` success: `svc.mark_applied(user_id,
      proposal_id, pr_ref = "owner/name#<number>")` then audit
      `oxpilot.diff_applied{session_id, proposal_id, branch, pr_ref}`
      and return `{proposal_id, pr_ref}`. The service itself already
      enforces ownership and `status != "pending" ⇒ diff_not_pending`
      inside `mark_applied`/`proposal` — the proc layer re-checks
      (BR5.2) rather than owning the rule. A proposal whose apply
      already succeeded stays `applied` — a second apply hits
      `diff_not_pending`, so double-materialization is impossible.
    applies_to: diff_apply.
    source: AC2.2.1, AC2.2.4.

  - id: BR5.6
    kind: constraint
    text: >
      The sequence NEVER pushes to an existing branch, force-updates
      a ref, merges, or mutates the default/protected branch: the
      head is always a fresh `oxpilot/` namespaced branch;
      `branchCreate` on an already-existing name fails rather than
      reusing it (BR5.7), and the default branch is never a valid
      `head_ref` by construction.
    applies_to: diff_apply.
    source: FR3.3, AC2.2.2/AC2.2.3.

  - id: BR5.7
    kind: constraint
    text: >
      Mid-sequence failure stops immediately and returns
      `apply_failed:<step>` naming the proc that failed (branchCreate /
      file op / pull.create) — NO automatic rollback (the partial
      branch is user-deletable via `repo.branchDelete`) and NO auto-
      retry. The proposal stays `pending` so the user can retry after
      cleanup; a leftover branch surfaces as
      `apply_failed:branch_exists` on retry.
    applies_to: diff_apply.
    source: FR3.2.

  # ---- BR6 — Actions reads (CI inspection) ----
  - id: BR6.1
    kind: constraint
    text: >
      The Actions surface is exactly six read-class tools —
      `actions_list_runs`, `actions_get_run`, `actions_get_job`,
      `actions_get_job_log`, `actions_list_workflows`,
      `actions_job_annotations` — all repo-scoped through the BR3.1
      dispatch path with the session repo injected.
    applies_to: registry.
    source: FR4.1, AC3.1.1.

  - id: BR6.2
    kind: constraint
    text: >
      Backing procs, verbatim and closed: `repo.actions.listRuns`,
      `repo.actions.getRun`, `repo.actions.getJob`,
      `repo.actions.getJobLog`, `repo.actions.listWorkflows`,
      `repo.actions.jobAnnotations`. Artifact reads
      (`listArtifacts`/`getArtifact`) are NOT in v1's catalog —
      payloads are unbounded/binary-friendly; the AC pins
      runs/jobs/logs only.
    applies_to: registry.
    source: FR4.1, AC3.1.1–AC3.1.3.

  - id: BR6.3
    kind: constraint
    text: >
      NO control or mutation Actions proc can ever be model-callable —
      `approveRun`, `approveJob`, `cancelRun`, `rerunRun`,
      `dispatchWorkflow`, `dispatchRepository`, `deleteRun`,
      `deleteRunLogs`, `deleteArtifact`, `deleteCache`,
      `dispatchRelease`, plus all secrets/variables/environments/
      runners settings writers. The mechanism is ABSENCE from the
      registry, not a runtime blocklist — AC4.3.x's CI-fix path can
      only ever PROPOSE a change.
    violation_behaviour: nonexistent — an unregistered name fails at
      BR1.3 before any dispatch.
    applies_to: registry.
    source: FR4.3, AC3.1.3.

  - id: BR6.4
    kind: validation
    text: >
      `actions_get_job_log{run_id, job_id, offset?}` never passes
      `wait_ms` — long-polling inside a 60 s tool window starves the
      turn loop; incremental reads ride `offset`/`next_offset`
      pagination, output tail-bounded to TOOL_OUTPUT_MAX with a
      `truncated` marker.
    applies_to: actions_get_job_log.
    source: AC3.1.2, NFR9.

  # ---- BR7 — computer-use dispatch ----
  - id: BR7.1
    kind: constraint
    text: >
      `exec_shell`, `fs_read`, `fs_write` dispatch ONLY through
      `gateway.dispatch_action(&ClientMeta::default(), user_id,
      session_id, target_id, kind, args)` — every gateway trait verb
      takes the `client: &ClientMeta` first param (oxpilot.rs:736+;
      server-side callers pass a default-constructed meta per the
      trait doc). `dispatch_action` returns the full
      `OxpilotActionRow` — the tool output reads `id`/`state` off it
      (`action_id` IS the row's `id` field). The tool never opens a
      socket, never targets a connector, never bypasses the grant
      chain. `kind` = `entry.spec.name` verbatim (the registry cannot
      mint a fourth kind).
    applies_to: exec/fs handlers.
    source: C7, FR5.3, AC4.3.1.

  - id: BR7.2
    kind: validation
    text: >
      Per-kind arg schema, closed properties: `exec_shell{command:
      string 1..1024, target_id?}`; `fs_read{path: 1..4096, target_id?}`;
      `fs_write{path: 1..4096, content_b64: required, target_id?}` —
      serialized args must fit inside ACTION_ARGS_MAX (2 KiB) since the
      gateway stores the (redacted) arg string. Schema rejection ⇒
      `invalid_args` BEFORE any gateway call.
    applies_to: exec/fs handlers.
    source: AC4.3.1, U6 pins.

  - id: BR7.3
    kind: authorization
    text: >
      Target resolution order: `args.target_id` present ⇒ use it (the
      gateway's own owner/connected/caps checks are the verification);
      absent ⇒ `gateway.list_targets(&ClientMeta::default(), user_id)`
      filtered to `lifecycle == "connected"` — `TargetView` splits the
      vocabularies (`status ∈ online|offline|revoked` vs `lifecycle ∈
      pending_consent|connected|disconnected|revoked`), so the filter
      pins the LIFECYCLE field, matching the gateway's own row check;
      a `connected` target with stale `last_seen_at` still auto-
      selects and dispatches into the gateway's `offline` denial —
      the liveness check stays the gateway's. Exactly one connected ⇒
      auto-select; zero ⇒ `ToolError{no_target}`;
      more than one ⇒ `ToolError{ambiguous_target}` whose MESSAGE
      instructs the model to pass `target_id` (discoverable via
      `targets_list`) — actionable guidance must ride
      `ToolError.message`/`ToolOutput.content` since `code` never
      reaches the model (BR3.4).
      The model can never name another user's target into existence —
      the gateway re-verifies ownership.
    applies_to: exec/fs handlers.
    source: AC4.3.2, Q6-A.

  - id: BR7.4
    kind: constraint
    text: >
      Denial mapping on `AppError{code: oxpilot.action_denied,
      data.denied}`: `consent_required` ⇒ emit
      `card{card_kind:"consent", ref_id: grant_id}` through the same
      `state.oxpilot.turns()` → `.get(ctx.session_id)` → `emit_frame`
      path BR4.5 pins (same terminalled-turn drop race) AND return
      `ToolOutput` "consent requested — grant <grant_id> pending; the
      action queues on approval — do NOT re-issue the call" (success-
      shaped per U6-WF2 — a pending grant is progress, not failure).
      `respond_grant` on allow MATERIALIZES the Action atomically from
      the grant's stored args, audits `oxpilot.action_dispatched`, and
      `notify_waiters` — an identical re-dispatch while the grant is
      still `pending` dedupes to the same grant_id, but after approval
      the model MUST NOT re-issue: under `action_once` scope the
      consumed grant no longer matches the pending-dedupe and a SECOND
      pending grant mints (second consent card, second execution);
      under `action_session` scope the live-grant arm inserts a SECOND
      identical action. The model holds `grant_id`, not `action_id`,
      and no grant→action lookup exists (`action_status` takes
      `action_id` only) — tool output must not promise status
      follow-up on the consent path. `not_connected` | `offline` |
      `capability` ⇒ `ToolError{action_denied}` carrying the reason in
      `message` (AC4.3.4's visible error — `code` never reaches the
      model, BR3.4); missing/malformed `denied` marker or unknown
      reason ⇒ `ToolError{internal}` — fail closed, never
      message-sniff.
    applies_to: exec/fs handlers.
    source: AC4.3.3/AC4.3.4, U6-BR4.4.

  - id: BR7.5
    kind: constraint
    text: >
      Dispatch success ⇒ `ToolOutput` carrying `{action_id, state:
      queued}` and a one-line summary — the connector result arrives
      asynchronously; the model polls `action_status{action_id}` on a
      later round/turn. U5 never blocks inside `invoke` waiting for
      the target.
    applies_to: exec/fs handlers + action_status tool.
    source: AC4.3.2/AC4.3.5, U6-WF2.

  - id: BR7.6
    kind: constraint
    text: >
      `targets_list{}` and `action_status{action_id}` are read-class
      tools over `gateway.list_targets(&ClientMeta::default(),
      user_id)` / `gateway.action_status(&ClientMeta::default(),
      user_id, action_id)` — the same `client`-first call shape as
      BR7.1; both already user-scoped inside the gateway;
      `action_status` on a foreign or
      absent action ⇒ `ToolError{not_found}` (the gateway's own
      anti-enumeration).
    applies_to: targets_list, action_status.
    source: AC4.3.5, FR7.4.

  - id: BR7.7
    kind: constraint
    text: >
      U5 never logs, persists, or summarizes RAW exec/fs args itself —
      the raw string goes to `dispatch_action` only; every retained
      surface (card `args_summary`, `ActionView.args_summary`) is the
      gateway's redacted/bounded copy or U3's `oxidean_core::redact`
      pass.
    applies_to: exec/fs handlers.
    source: FR5.5, NFR2.

  # ---- BR8 — agent-artifact comments (FR7.5) ----
  - id: BR8.1
    kind: authorization
    text: >
      `pr_comment{number, body}` resolves the session's applied
      proposals and requires `owner/name#number` ∈ ArtifactScope —
      the PR must be one this session materialized via diff_apply.
      Outside the set ⇒ `ToolError{forbidden}` (a hard error, not a
      silent no-op — the model needs to know it can't comment there).
      Issues, other sessions' PRs, and human PRs are all unreachable.
    applies_to: pr_comment.
    source: FR7.5, AC5.5.2.

  - id: BR8.2
    kind: constraint
    text: >
      The comment posts via `pull.comments.create{owner, name,
      number, body}` through rpc_dispatch — already shipped in
      oxidean-api-pulls, and called WITHOUT `path`/`side`/`line` so
      the comment lands as a general PR conversation comment (the
      anchor fields are optional; omitting them is the conversation
      path, not a separate proc). The forge shares the issue NUMBER
      COUNTER (`pull.create` calls `allocate_next_issue_number`) but
      not the tables — PRs live in `pull_requests`, their comments in
      `pull_comments`, and `issue.comments.create` resolves its target
      through the issues table, so it would answer `issue_not_found`
      on every real PR number. Line-anchored review comments (same
      proc, anchors supplied) are not exposed in v1.
    applies_to: pr_comment.
    source: FR7.5, Q9-A.

  - id: BR8.3
    kind: validation
    text: >
      `body` non-empty ≤ COMMENT_MAX (4 KiB) and passed through
      `oxidean_core::redact` (the free function — there is no
      `svc.redact`) by the HANDLER before dispatch: `pull.comments.
      create`/`issue.comments.create` only `validate_body`, they do
      not redact. A model-authored comment must not carry secrets
      into a public artifact (NFR2 floor applied at the write
      boundary).
    applies_to: pr_comment.
    source: NFR2, FR7.5.

  # ---- BR9 — audit, redaction, output bounds ----
  - id: BR9.1
    kind: policy
    text: >
      Write-class mutations emit audit rows AFTER the effect commits
      via `audit::record_with(&state.db, &ClientMeta::default(),
      actor, event, target, detail)` — the ctx-free entry point (the
      tool holds `ToolCtx`, not `RpcCtx`); `actor` is
      `Option<(user_id, username)>` resolved by a `find_user_by_id`
      lookup on `ctx.user_id` (fall back to `None` rather than
      skipping the event when the lookup fails). Events: propose_diff
      ⇒ `oxpilot.proposal_created{session_id,
      proposal_id, title, file_count}`; diff_apply ⇒
      `oxpilot.diff_applied{session_id, proposal_id, branch, pr_ref}`;
      pr_comment ⇒ `oxpilot.artifact_comment{session_id, pr_ref,
      comment_id}`. exec/fs dispatch coverage is U6's
      `oxpilot.action_dispatched` — U5 emits no duplicate row for
      the same effect.
    applies_to: write-class handlers + diff_apply.
    source: C6 class rule, FR5.5/AC5.3.x, NFR5.

  - id: BR9.2
    kind: policy
    text: >
      Read-class tools emit NO audit rows — the durable tool-call card
      row U3 persists (with bounded args/output summaries) is already
      the read trail; a per-read audit row is noise the C6 rule
      explicitly exempts.
    applies_to: read-class handlers.
    source: C6 class rule.

  - id: BR9.3
    kind: constraint
    text: >
      Audit `detail` carries ids, counts, and refs only — never diff
      bodies, file contents, command strings, tool args, or result
      payloads beyond summary length; every user-derived field passes
      `oxidean_core::redact` first. Failure of `audit::record`/
      `record_with` is swallowed (post-commit, best-effort) — a
      broken audit sink never fails a
      committed tool effect.
    applies_to: all audit emissions.
    source: FR5.5, NFR5.

  - id: BR9.4
    kind: constraint
    text: >
      Every handler bounds `ToolOutput.content` ≤ TOOL_OUTPUT_MAX
      (32 KiB) truncating with an explicit `truncated: true` marker,
      and `output_summary` ≤ OUTPUT_SUMMARY_MAX (2 KiB) — the runtime
      enforces the same ceilings, but a well-formed tool stays under
      them rather than relying on the clamp.
    applies_to: all handlers.
    source: U3 shipped pins, NFR9.

  - id: BR9.5
    kind: constraint
    text: >
      Model/user-derived strings reaching any persisted or echoed
      surface (args_summary, output_summary, proposal title/diff_body,
      comment body, audit detail) pass `oxidean_core::redact` — the
      free function, not a service verb. Split of responsibilities:
      `save_proposal` already redacts `title`/`diff_body` internally
      at the service boundary (services.rs:637–639), the gateway
      redacts its stored `args_summary`, and the TOOL redacts
      `output_summary`, the `pr_comment` body (BR8.3), and audit
      detail pre-dispatch. Secrets, tokens, provider keys, and
      connector verifier material never
      land in rows or model-visible output.
    applies_to: all handlers.
    source: NFR2, FR5.5.
```

## Summary

| Range | Kind | Covers |
|---|---|---|
| BR1.1–1.4 | constraint/validation | closed static registry, provider-legal names, `unknown_tool`, schema gate |
| BR2.1–2.5 | authorization/constraint | session re-resolution, session-bound repo injection, `ToolCtx` sole identity, internal read|write class, per-class write gates |
| BR3.1–3.4 | constraint/validation | `rpc_dispatch`-only reads, literal proc allowlist, arg bounds, verbatim error-code mapping |
| BR4.1–4.6 | constraint/validation | pending proposal insert, zero git mutation, diff↔ops coherence, bounds, diff card emit, session-scoped proposal_get |
| BR5.1–5.7 | constraint/authorization/policy | user-only `diff_apply`, ownership + pending gate, ordered branch→ops→PR via caller's dispatch, provenance line, `pr_ref` + audit, no push/merge/default-branch mutation, fail-stop with `apply_failed:<step>` |
| BR6.1–6.4 | constraint/validation | six-tool closed Actions read surface, literal proc list, control-proc absence, bounded log reads without `wait_ms` |
| BR7.1–7.7 | constraint/validation/authorization | C7-only dispatch, per-kind schemas + 2 KiB args bound, target resolution + ambiguity, `denied` marker → consent-card vs `action_denied`, async `action_status` polling, read-class targets/status tools, no raw-arg retention |
| BR8.1–8.3 | authorization/constraint/validation | ArtifactScope-bound `pr_comment`, `pull.comments.create` (unanchored) path, body bounds + `oxidean_core::redact` |
| BR9.1–9.5 | policy/constraint | post-commit write-class audit events, read-class exemption, bounded redacted detail, output ceilings, `oxidean_core::redact` on every persisted surface |
