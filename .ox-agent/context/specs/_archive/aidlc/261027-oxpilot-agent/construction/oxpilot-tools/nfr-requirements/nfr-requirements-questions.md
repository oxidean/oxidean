# NFR Requirements Questions — oxpilot-tools

Unit: `oxpilot-tools` (U5, kind `service`) — `OxpilotTools`: the closed
model-callable tool catalog — repo/issue/pull read tools, six read-only
Actions tools, `propose_diff`/`proposal_get` + the user-only
`oxpilot.diff_apply` materialization path, ArtifactScope-bound
`pr_comment`, and `exec_shell`/`fs_*`/`targets_list`/`action_status`
dispatching into `ConnectorGateway` — per-tool authorization,
write-class audit, and the deny-by-default registry every model reach
into the forge flows through.

## Sources

- [unit] `inception/units-generation/unit-of-work.md` §U5.
- [req] requirements.md — NFR1 (session-only auth is U1's seam; U5's
  half is ToolCtx-sole-identity + consent-before-target + no stored
  secret leaving an agent-reachable surface), NFR2 (per-user isolation;
  redaction on every persisted/echoed model string — the tool surface
  is where model-authored content lands in rows and public artifacts),
  NFR3 (the shipped tool-window and payload bounds), NFR4 (fail-closed
  taxonomy; async dispatch degrades gracefully), NFR5 (write-class
  audit events attributable to the user), NFR6 (reuse existing
  tracing — no new observability stack), NFR9 (registration/surface-
  closure pin tests; per-crate procedures/dispatch pinning).
- [design] construction/oxpilot-tools/functional-design/{entities,
  rules, functional-spec}.md — BR1–BR9 the NFRs bind to; READY on
  adversarial review iteration 2.
- [contract] contract-summary.md — C1 `oxpilot.diff_*` procs, C6
  `ToolExecutor{catalog, invoke}` + `ToolCtx`/`ToolOutput`/`ToolError`,
  C7 `dispatch_action`/`action_status` + `Denied{reason, grant_id?}`.
- [u3-fd] construction/agent-runtime/functional-design — the turn loop
  that races `invoke` under TOOL_CALL_TIMEOUT=60s, bounds
  MAX_TOOL_ROUNDS=8, and clamps ARGS_SUMMARY_MAX/OUTPUT_SUMMARY_MAX=
  2KiB + TOOL_OUTPUT_MAX=32KiB.
- [u6-fd] construction/connector-gateway/functional-design — dispatch
  check order session→target→caps→grant→pending-dedupe→insert and the
  `data.denied` marker contract.
- [shipped] `oxidean-api-core/src/oxpilot.rs` (SUMMARY_MAX=512B,
  RESULT_MAX=2KiB, ACTION_ARGS_MAX=2KiB, `denied()`,
  CONNECTOR_CAPABILITIES, `NullTools` `no_tools`),
  `oxidean-api-oxpilot/src/runtime.rs` (emit_frame `pub(crate)`,
  terminal CAS drop), `connector/gateway.rs` (per-target lock),
  `oxidean-db` (`find_user_by_id`), `oxidean-api-core/src/audit.rs`
  (`record`/`record_with`), `oxidean-core/src/redact.rs`.

## Q1. Which NFR domains does oxpilot-tools carry real obligations in?

- A. Security (the closed registry + schema gate + ToolCtx-sole-
  identity + write-class gates are the injection/confused-deputy
  boundary for every model reach into the forge; consent mapping at
  the C7 seam; redaction on every retained surface), Privacy (model-
  authored strings land in rows and a PUBLIC artifact — `pr_comment`
  — so `redact()` + bounded detail is a privacy control, and
  session/user scoping + `not_found` anti-enumeration is the
  per-user-isolation half), Reliability (the fail-closed
  ToolError taxonomy, consent-as-progress success shape, apply
  fail-stop, terminalled-turn emit drop), Performance (every invoke
  lives inside the 60 s tool window; bounded outputs, pagination,
  internal proc fan-out), Observability (exactly three post-commit
  write-class audit events with bounded redacted detail; card rows as
  the durable read trail), Testability (registry-closure and
  allowlist pin tests — NFR9's surface-closure leg for this unit),
  Usability (actionable model-facing guidance must ride
  `ToolError.message` since `code` never reaches the model).
- B. Security only — misses NFR5's write-class audit rows, NFR4's
  degrade contract (consent_required is success-shaped progress, and
  a connector-side failure degrades through the gateway, never a U5
  hang), and NFR9's closed-surface pin tests which name this unit's
  registry.
- C. Include Accessibility — U5 emits card kinds + ids, never markup;
  U8 owns the render surface WCAG binds to.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q2. Tool-surface closure — deny-by-default registry vs model discoverability?

- A. The catalog is a compiled-in `&'static [CatalogEntry]` — exactly
  the 23 flat `domain_verb` names, each bound to a literal backing
  proc: the BR3.2 ten-proc repo/issue/pull allowlist, the BR6.2 six
  Actions reads, `pull.comments.create` on the comment path, and C7
  dispatch kinds verbatim. An unregistered `call.name` ⇒
  `ToolError{unknown_tool}` before schema, handler, dispatch, or
  audit — the registry IS the allowlist (control/write procs are
  absent, not filtered; `diff_apply` has no catalog entry at all).
  Discoverability rides `catalog()` `ToolSpec` descriptions,
  `targets_list`, and actionable error messages — never a wider proc
  surface.
- B. Runtime filter over the full proc registry (skip control names):
  a filter bug or a new proc shipping before the filter learns it
  silently widens the model's reach — absence-by-construction has no
  bypass path.
- C. Provider-side tool selection / prompt-level scoping: the model is
  the adversary this surface defends against; the boundary must be
  code, not prompt text.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q3. Output bounds + redaction posture — what must every handler guarantee before returning?

- A. Handlers self-bound under the shipped ceilings rather than rely
  on U3's clamp (runtime.rs:582–586): `ToolOutput.content` ≤
  TOOL_OUTPUT_MAX (32 KiB) with an explicit `truncated` marker,
  `output_summary` ≤ OUTPUT_SUMMARY_MAX (2 KiB); persisted card
  `args_summary` ≤ ARGS_SUMMARY_MAX (2 KiB); `ActionView` passes
  through verbatim keeping the gateway's `args_summary` ≤
  SUMMARY_MAX (512 B) + `result_summary` ≤ RESULT_MAX (2 KiB) — U5
  performs no un-redaction. Every model-derived string reaching a
  persisted or echoed surface (output_summary, `pr_comment` body,
  audit detail) passes `oxidean_core::redact` — the free function —
  with the pinned split: `save_proposal` already redacts
  title/diff_body internally, the gateway redacts its stored
  args_summary, the tool redacts what it emits. Raw exec/fs args are
  never logged/persisted/summarized by U5 — the raw string goes to
  `dispatch_action` only.
- B. Emit full outputs and trust the runtime clamp + service redactor:
  a 256 KiB log tail would arrive truncated mid-JSON with no marker
  the model can page on, and any field that skips the service
  boundary (audit detail, comment body) would carry unredacted model
  output — the clamp is the second line, not the design.
- C. Per-tool bespoke caps sized to each proc: 23 different budgets is
  un-reviewable; two shipped ceilings + arg-side bounds is the
  enforceable contract.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q4. Fail-closed taxonomy + consent/denial posture — how do refusals surface?

- A. Every failure is a typed `ToolError` the durable card records:
  `unknown_tool` / `invalid_args` (naming the field) / `not_found`
  (anti-enumeration for foreign-or-dead sessions, proposals, actions)
  / `forbidden` (ArtifactScope, write gates) / `no_target` /
  `ambiguous_target` / `action_denied` (carrying the denied reason) /
  `diff_not_pending` / `apply_failed:<step>`; a missing executor is
  the shipped `NullTools` `no_tools`; a malformed `denied` marker ⇒
  `internal` — never message-sniff. `code` is internal/test-visible
  only, so actionable guidance rides `message` (ambiguous_target says
  pass `target_id`; action_denied names the reason). On the C7
  marker: `consent_required` ⇒ emit the consent card + a
  success-shaped `ToolOutput` ("queued on approval — do NOT re-issue")
  because a pending grant is progress and a post-approval re-issue
  mints a SECOND grant/second action; the other three reasons ⇒
  `action_denied`.
- B. Map all four denial reasons to hard errors: consent_required as
  failure tells the model its progress failed and invites the exact
  re-issue that mints a second execution after approval — the
  success-shape IS the dedupe guard.
- C. Poll/re-dispatch inside `invoke` until the grant resolves:
  blocks inside the 60 s window on a human decision, and still can't
  guarantee approval — the async card + later-turn status is the
  contract.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q5. Internal dispatch cost — how many proc calls may one invoke make?

- A. Read-class invokes make exactly ONE `rpc_dispatch` call (plus one
  session re-resolve); `pr_comment` adds one session-scoped proposals
  query for ArtifactScope; exec/fs adds `list_targets` only on the
  no-arg fallback; `actions_get_job_log` never passes `wait_ms` —
  incremental reads ride `offset`/`next_offset` across calls. The
  only multi-proc path is the USER proc `diff_apply`, bounded by
  construction: 1 branchCreate + ≤1 batched `repo.file.upload` (≤50
  ops — MAX_PROPOSAL_FILES pinned to the shipped UPLOAD_MAX_FILES) +
  ≤50 per-op delete/rename + 1 pull.create, sequential on the
  caller's own dispatch. Per-turn tool-call count is bounded by
  MAX_TOOL_ROUNDS=8 on the U3 side; U5 adds no unbounded loop inside
  a call — auto-pagination across rounds is the model's job.
- B. Let handlers auto-paginate until satisfied (fetch-all-then-cap):
  an unbounded proc loop inside a 60 s window starves the turn and
  can still blow the output cap — bounded single calls with
  model-driven paging is the honest shape.
- C. Cache proc results across calls to save dispatches: stale reads
  for a correctness-critical surface plus cross-call state on a
  shared executor — the executor stays stateless.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q6. Audit + observability posture — what emits `audit_events`, what rides cards?

- A. Exactly three post-commit events via
  `audit::record_with(&state.db, &ClientMeta::default(), actor,
  event, target, detail)` — `oxpilot.proposal_created{session_id,
  proposal_id, title, file_count}` on propose commit,
  `oxpilot.diff_applied{session_id, proposal_id, branch, pr_ref}` on
  apply success, `oxpilot.artifact_comment{session_id, pr_ref,
  comment_id}` on comment commit — actor resolved via
  `find_user_by_id` with `None` fallback, detail bounded to
  ids/counts/refs and redacted, failure swallowed. Read-class emits
  ZERO rows — the durable card row is the read trail (C6 rule
  verbatim); exec/fs gets no U5 row since U6's
  `oxpilot.action_dispatched` covers the same effect. Logging rides
  existing `tracing` — `warn!` on swallowed audit/dropped emit;
  never per-call `info!` spam; ids only, never args/bodies.
- B. Audit every tool call including reads: floods `audit_events`
  with non-events the C6 class rule explicitly exempts — the card
  row already records every read durably.
- C. Audit nothing — cards suffice: the write-class audit row is
  NFR5's attribution requirement (proposal/apply/comment attributable
  to the user); cards are session-scoped records, not the operator's
  audit surface.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q7. Testability posture — what proves the surface stays closed without a live provider?

- A. Pin tests at three seams on the existing in-memory harness: (1)
  a registry-closure test asserting the catalog is EXACTLY the 23
  pinned names, each mapped to its allowlisted proc/kind — the
  Actions mutation vocabulary asserted absent, and `diff_apply`
  asserted to have no catalog entry; (2) per-handler tests with a
  stubbed `rpc_dispatch`/`ConnectorGateway` — foreign session ⇒
  `not_found`, schema rejections ⇒ `invalid_args`, denied-marker
  mapping (consent card vs `action_denied`), read-class-no-audit vs
  write-class-audit, session-pin on `proposal_get`; (3) `diff_apply`
  negative tests — non-pending ⇒ `diff_not_pending`, mid-sequence
  failure ⇒ `apply_failed:<step>` + still-pending row. Plus the
  ops_json migration's dialect assertion in `dialect_oxpilot.rs`
  alongside U2's tables (NFR9's same-change rule).
- B. Snapshot `catalog()` output only: proves names exist, not the
  closure/binding/denial behavior — a renamed entry that still
  dispatches to a control proc would pass a name snapshot.
- C. Provider-level contract tests: needs a live model to exercise
  what a stubbed dispatch already proves deterministically — wrong
  layer for a security boundary.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Q8. Tech-stack choices — which seams carry this, and does JSON-Schema validation need a new dep?

- A. Reuse everything shipped, zero new dependencies: executor lives
  inside `oxidean-api-oxpilot` (emit_frame is `pub(crate)`; invoke
  gets no turn handle); reads via `state.rpc_dispatch` on
  `build_rpc_ctx_with_session`-synthesized `RpcCtx` +
  `ClientMeta::default()` — never direct db/git; audit via
  `audit::record_with`; redaction via `oxidean_core::redact`;
  `serde_json` for args; `diff_*` procs registered on the existing
  `oxpilot.*` PROCEDURES set + `make rpc-gen`. Arg validation is a
  MINIMAL HAND-ROLLED JSON-Schema check over U5's own closed
  vocabulary (type/required/enum/properties/items/minLength/
  maxLength/minItems/maxItems/minimum/maximum/pattern/
  `additionalProperties: false`) — the workspace ships `schemars`
  1.2 for schema GENERATION only (oxidean-core, oxidean-api-rest)
  and has NO runtime-validation crate; the schemas are authored
  in-registry, so the validator validates a vocabulary U5 itself
  defines (op-conditional requireds live handler-side as
  `invalid_args` — schema cannot express them).
- B. Add the `jsonschema` crate for full spec validation: a new
  dependency (pulling a full draft-2020-12 engine) to validate a
  closed keyword vocabulary U5 emits itself — the dependency
  cost buys coverage of schema features the registry never uses.
- C. Trust the model and skip schema validation: model args are the
  injection vector — BR1.4 pins validation BEFORE the handler; an
  unvalidated `fs_write` arg reaching the gateway is exactly the
  failure this gate exists for.
- D. Not decided.
- X. Other (please specify)

[Answer]: A

## Consolidated Summary Confirmation

- Domains: Security, Privacy, Reliability, Performance,
  Observability, Testability, Usability (model-facing message leg) —
  seven with real pins; Accessibility/Scalability-deferral per Q1.
- Closure: compiled-in 23-entry registry; absence not filtering;
  `unknown_tool` before everything; `diff_apply` has no catalog
  entry (Q2).
- Bounds/redaction: 32 KiB content / 2 KiB summaries / 512 B action
  args_summary pass-through; `redact()` on every persisted/echoed
  model string; raw exec/fs args never retained (Q3).
- Fail-closed: typed ToolError taxonomy, code-internal/message-
  model split, consent success-shape with the re-issue hazard
  pinned (Q4).
- Cost: one dispatch per read; apply bounded ≤ ~53 procs; no
  wait_ms; no unbounded in-call loops (Q5).
- Audit/observability: three post-commit events, read-class exempt,
  bounded redacted detail, existing tracing only (Q6).
- Tests: registry-closure + handler + apply-negative pin tests;
  ops_json dialect assertion (Q7).
- Tech: all seams reused; hand-rolled minimal JSON-Schema validator;
  zero new deps (Q8).

[Answer]: Looks correct
