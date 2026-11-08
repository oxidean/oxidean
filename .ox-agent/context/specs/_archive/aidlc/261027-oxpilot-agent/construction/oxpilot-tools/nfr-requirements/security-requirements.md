# Security Requirements — oxpilot-tools (U5)

Consumes: `construction/oxpilot-tools/functional-design/rules.md`
(BR1–BR9), `functional-design/entities.md`,
`functional-design/functional-spec.md`, `nfr-requirements-questions.md`,
`inception/requirements-analysis/requirements.md` (NFR1, NFR2),
`inception/contract-design/contract-summary.md` (C1, C6, C7),
`codekb/oxidean-app/technology-stack.md`.

The tool surface is the only path by which model output becomes a forge
effect — every pin below is server-side enforcement inside `invoke`,
never prompt policy. The model is treated as an untrusted caller whose
args are injection attempts and whose identity claims are worthless;
`ToolCtx` + the resolved session row are the only authorities.

## Tool-surface closure

| id | requirement | basis |
|----|-------------|-------|
| NFR1.1 | The catalog is a compiled-in `&'static [CatalogEntry]` — exactly the 23 flat `domain_verb` names, fixed-order, identical for every session/provider. Nothing loads tool definitions from data, config, or provider output; adding/removing a tool is a reviewed code change against the allowlist rules, never a runtime filter over a larger proc set | BR1.1, BR1.2, Q2-A |
| NFR1.2 | An unregistered `call.name` ⇒ `ToolError{unknown_tool}` BEFORE schema validation, handler, dispatch, audit, or card emit — fail closed; U3's durable card row still records the attempt. A missing executor resolves to shipped `NullTools` whose every call answers `no_tools` — the no-executor failure mode is closed by construction, not by null-check | BR1.3, WF1, `oxpilot.rs:127–141` |
| NFR1.3 | `call.args_json` is validated against the entry's `parameters_json` BEFORE the handler runs — types, required fields, enums, string bounds, nested object/array structure (`properties`/`items`/`minItems`/`maxItems`), numeric bounds (`minimum`/`maximum`), `pattern`, closed `additionalProperties` — violations ⇒ `ToolError{invalid_args}` naming the offending field; op-conditional requireds (rename⇒`new_path`, create/update⇒`content_b64`) are checked handler-side as `invalid_args`; the handler NEVER sees unvalidated input | BR1.4, Q8-A |
| NFR1.4 | The backing-proc allowlist is closed and literal: the BR3.2 ten (`repo.get`, `repo.tree`, `repo.blob`, `repo.refs`, `repo.commits`, `repo.commit`, `issue.list`, `issue.get`, `pull.list`, `pull.get`) + the BR6.2 six Actions reads + `pull.comments.create` (comment path) + the C7 kind trio. EVERY other proc — `pull.files`, `pull.commits`, `issue.comments.list`, `repo.search`, `repo.compare`, `repo.blame`, all repo/issue/pull writers, artifact reads (`listArtifacts`/`getArtifact` — unbounded payloads), and every Actions control/mutation proc (`approve*`, `cancel*`, `rerun*`, `dispatch*`, `delete*`, secrets/variables/environments/runners writers) — is ABSENT from the registry, not blocklisted; absence is a property no arg can defeat | BR3.2, BR6.2, BR6.3, BR8.2, AC3.1.3 |
| NFR1.5 | `exec_shell`/`fs_read`/`fs_write` tool names equal the C7 `kind` vocabulary verbatim and the handler passes `entry.spec.name` straight into `dispatch_action`'s `kind` — the registry cannot mint a fourth kind, and no other tool can smuggle a connector action through a renamed entry | BR1.2, BR7.1, CONNECTOR_CAPABILITIES |

## Identity & scope binding

| id | requirement | basis |
|----|-------------|-------|
| NFR1.6 | `ToolCtx` is the SOLE identity authority: `session_id`/`user_id` come from the owned session row at mint (U3), and any `user_id`/`session_id`/`repo_id`-shaped field inside `args_json` is rejected at schema (`additionalProperties: false`) — identity never crosses the args boundary. The exec/fs `target_id` arg is a TARGET SELECTOR only; the gateway's own owner check verifies it (BR7.3) | BR2.3, NFR1 |
| NFR1.7 | Every handler re-resolves `svc.get_session(ctx.user_id, ctx.session_id)` before doing work — the resolved row is authoritative for ownership, repo binding, and liveness; `ctx.repo_id` is advisory. Absent or foreign session ⇒ `ToolError{not_found}` — foreign and dead ids are indistinguishable (anti-enumeration) | BR2.1, FR2.3 |
| NFR1.8 | Repo-scoped tools inject `{owner, name}` into the backing proc's input from the RESOLVED session's repository binding — never from args; a `ref`/`branch` arg may only narrow WITHIN that repo (defaulting to the session branch). No arg can point a tool at another repo | BR2.2, FR7.1 |
| NFR1.9 | Model-facing proposal reads pin `row.session_id == ctx.session_id` POST-FETCH — `svc.proposal`/`owned_proposal` is USER-scoped (proves the row's session belongs to the user, not that it IS this session), so `proposal_get` on a same-user-OTHER-session or foreign/absent id ⇒ `ToolError{not_found}`. The user proc `diff_apply` deliberately accepts same-user-other-session proposals (BR5.2) — the row's own session supplies repo binding and apply target; the relaxation is user-side only and never reaches the model | BR4.6, BR5.2, `services.rs:140–153` |
| NFR1.10 | Read tools reach backing procs through `state.rpc_dispatch` on a session-equivalent `RpcCtx` synthesized per call via `build_rpc_ctx_with_session` + `ClientMeta::default()` — the `oxpilot:`-namespaced synthetic id can never collide with a real session row; no fabricated IP/UA; `require_verified`-class proc guards run verbatim; repo read ACL/feature flags/archived checks apply with NO tool-layer pre-clearing. Never direct `oxidean-db`/`GitBackend`/`vcs` calls | BR3.1, FD-Q11 |

## Write-class gates & materialization

| id | requirement | basis |
|----|-------------|-------|
| NFR1.11 | The read\|write class is fixed data on `CatalogEntry` — never on the wire `ToolSpec` (D-01) and never derivable from args. Write-class = {`propose_diff`, `pr_comment`, `exec_shell`, `fs_read`, `fs_write`} + the user-proc `diff_apply`; every other entry is read-class. A write-class handler's class-specific gate runs AFTER session resolution; a failed gate ⇒ `ToolError{forbidden}` or the mapped denial — never a partial effect | BR2.4, BR2.5, entities |
| NFR1.12 | `propose_diff` inserts exactly one `pending` row — the model can never set `applied`/`pr_ref`, never update in place (a revision is a new row), and performs ZERO git mutation. Before insert, path-set coherence MUST hold: `set(files[].path ∪ new_path) == set(diff --git header paths)` else `invalid_args` naming the disagreeing paths — what the user reviews is what apply writes; an incoherent proposal never becomes a row | BR4.1, BR4.2, BR4.3 |
| NFR1.13 | `oxpilot.diff_apply` is a USER proc — SessionOnly + `require_enabled` + `require_verified` + owned session — and NO catalog entry maps to it; the explicit-confirm gate is structural (only the user's own proc call reaches this path), not a prompt promise. The proc-bound user is the ONLY principal — no body field substitutes another identity; `status != pending` ⇒ `diff_not_pending` | BR5.1, BR5.2, C1, AC2.2.1 |
| NFR1.14 | The apply sequence runs on the CALLER's own `ctx.rpc_dispatch` (real session ctx — repo ACL, branch protection, archived checks apply verbatim) and NEVER pushes to an existing branch, force-updates a ref, merges, or mutates the default/protected branch: the head is always a fresh `oxpilot/proposal-<id8>` branch; `branchCreate` on an existing name fails rather than reusing | BR5.3, BR5.6, AC2.2.2/AC2.2.3 |
| NFR1.15 | `pr_comment{number, body}` requires `owner/name#number` ∈ ArtifactScope — the `pr_ref`s of THIS session's `applied` proposals, resolved fresh per call. Outside the set ⇒ `ToolError{forbidden}` (hard error, not silent no-op): issues, other sessions' PRs (same user included), and human PRs are all unreachable; the comment posts via `pull.comments.create` UNANCHORED — never `issue.comments.create` (PRs share the issue number counter, not the issues tables) | BR8.1, BR8.2, FR7.5, AC5.5.2 |

## Computer-use boundary

| id | requirement | basis |
|----|-------------|-------|
| NFR1.16 | exec/fs dispatch ONLY via `gateway.dispatch_action(&ClientMeta::default(), user_id, session_id, target_id, kind, args)` — the tool never opens a socket, never targets a connector, never bypasses the session→target→caps→live-grant→pending-dedupe chain; U5 adds nothing to it and subtracts nothing from it | BR7.1, C7, U6-WF2 |
| NFR1.17 | Per-kind arg schemas are closed and bounded: `exec_shell{command: 1..1024, target_id?}`; `fs_read{path: 1..4096, target_id?}`; `fs_write{path: 1..4096, content_b64: required, target_id?}` — serialized args MUST fit ACTION_ARGS_MAX (2 KiB) since the gateway stores the (redacted) arg string; schema rejection lands BEFORE any gateway call. U5's jail duty ends at correct kind/args mapping — the fs/exec jail itself is U7-side | BR7.2, `oxpilot.rs:541` |
| NFR1.18 | Target resolution: `args.target_id` present ⇒ dispatch with it (the gateway's owner/connected/caps checks are the verification — the model can never name another user's target into existence); absent ⇒ `list_targets` filtered to `lifecycle == "connected"` — exactly one ⇒ auto-select; zero ⇒ `ToolError{no_target}`; more than one ⇒ `ToolError{ambiguous_target}` whose MESSAGE tells the model to pass `target_id` (discoverable via `targets_list`), since `code` never reaches the model | BR7.3, BR3.4 |
| NFR1.19 | Denial mapping rides the structured marker ONLY — `AppError{code: oxpilot.action_denied, data.denied.reason}`: `consent_required` ⇒ emit `card{card_kind:"consent", ref_id: grant_id}` + success-shaped `ToolOutput` pinning "queued on approval — do NOT re-issue"; `not_connected`/`offline`/`capability` ⇒ `ToolError{action_denied}` carrying the reason; missing/malformed marker or unknown reason ⇒ `ToolError{internal}` — fail closed, NEVER message-sniff | BR7.4, `denied()`/`denied_reason` |
| NFR1.20 | The consent dedupe hazard is pinned in the OUTPUT TEXT, not assumed: an identical re-dispatch while the grant is `pending` dedupes to the same `grant_id`, but after approval a re-issue mints a SECOND pending grant under `action_once` scope (second consent card, second execution) or a SECOND identical action under `action_session` scope. The tool output therefore carries the do-not-re-issue instruction and never promises grant→action status follow-up — none exists (`action_status` takes `action_id` only) | BR7.4, WF6.4 |

## Secret hygiene on retained surfaces

| id | requirement | basis |
|----|-------------|-------|
| NFR2.1 | Every model/user-derived string reaching a persisted or echoed surface passes `oxidean_core::redact` (free function — no `svc.redact`): the tool redacts `output_summary`, the `pr_comment` body, and audit detail pre-dispatch; `save_proposal` redacts `title`/`diff_body` internally at the service boundary; the gateway redacts its stored `args_summary`. Secrets, tokens, provider keys, and connector verifier material never land in rows or model-visible output | BR9.5, BR8.3, `services.rs:637–639` |
| NFR2.2 | U5 never logs, persists, or summarizes RAW exec/fs args — the raw string goes to `dispatch_action` only; every retained surface (card `args_summary`, `ActionView.args_summary`) is the gateway's redacted/bounded copy or U3's `redact` pass | BR7.7, FR5.5 |
| NFR2.3 | `pr_comment` body is non-empty ≤ COMMENT_MAX (4 KiB) and redacted by the HANDLER before dispatch — `pull.comments.create` only `validate_body`s, never redacts, and the comment lands on a PUBLIC artifact: the NFR2 floor applies at the write boundary, before egress, not after | BR8.3, NFR2 |
| NFR2.4 | Audit `detail` carries ids, counts, refs, and bounded redacted fields (`title` ≤ TITLE_MAX, redacted) only — NEVER diff bodies, file contents, command strings, tool args, or result payloads beyond summary length; every user-derived field passes `redact` first | BR9.3, NFR5 |

## Threat considerations

- Prompt injection via tool args is the primary adversary: the
  schema-closed registry + verbatim proc allowlist bound the worst a
  hostile arg set can reach — a bounded read, a pending proposal the
  user must still approve, or a gateway dispatch that dies on the
  gateway's own consent chain. No injected arg mints a tool, a proc, a
  kind, or an identity (NFR1.1–NFR1.6).
- Confused deputy: a tool handler runs with the session user's
  authority, so every scope source is pinned server-side — ToolCtx
  identity, session-row repo binding, ArtifactScope membership.
  `target_id` is the only arg-shaped scope selector and the gateway
  re-verifies ownership on it (NFR1.6/NFR1.18).
- Cross-session reach: the model-facing paths pin `session_id`
  post-fetch (NFR1.9); the deliberate same-user-other-session
  relaxation lives only in the user proc `diff_apply`, where the
  row's own session supplies the repo binding — the model surface
  never sees it.
- Consent re-issue is the one hazard a correct tool can still cause:
  the pending-grant dedupe absorbs an identical re-dispatch, but a
  post-approval re-issue double-executes — the output text is the
  only guardrail because `ToolError.code` never reaches the model
  (NFR1.20, BR3.4).
- The exec/fs jail stays U7-side. U5's duty is correct kind/args
  mapping within ACTION_ARGS_MAX — a pathological path the connector
  refuses is U7's contract to enforce, not U5's to pre-judge.
- Indirect prompt injection rides tool OUTPUT, not just args: issue
  titles/bodies, PR text, CI job logs, and file contents are
  adversary-authored strings the read tools relay verbatim into model
  context — the canonical second-order vector for exactly this
  surface. The containment is the same closure that bounds hostile
  args: whatever the injected content convinces the model to attempt,
  the attempt still lands inside the registry — worst case a bounded
  read, a pending proposal the user must approve, or a gateway
  denial. Output-side instruction-safety is the model's, not U5's;
  U5's duty is that no tool output carries an authority escalation
  the registry doesn't also enforce.
- Markup injection into persisted and public surfaces is the second
  content class: model-authored `title`/`diff_body`/PR body/
  `pr_comment` body pass `redact()`, which masks credential patterns
  only — markup sanitization is an unowned boundary between U5's
  redaction pins and U8's diff-card render + the forge's public
  comment renderer. Assigned explicitly: U8 renders diff content
  escaped/plain (its reviewable-diff card), the forge renderer owns
  comment markdown — U5 records it as a boundary hand-off, not a
  residual it silently absorbs.
