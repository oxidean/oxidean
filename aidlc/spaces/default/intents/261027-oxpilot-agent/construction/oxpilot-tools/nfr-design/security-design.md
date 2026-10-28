# Security Design — oxpilot-tools (U5)

Consumes: `nfr-requirements/security-requirements.md` (NFR1.1–1.20,
NFR2.1–2.4), `functional-design/rules.md` (BR1–BR9), `entities.md`
(`OxpilotTools`/`CatalogEntry`/`ToolCtx`/`ArtifactScope`/
`DeniedOutcome`), `nfr-design-questions.md` (Q1–Q7, all Option A),
shipped `oxidean-api-core/src/{oxpilot,app,audit,rpc}.rs`,
`oxidean-api-oxpilot/src/{runtime,session,services}.rs`,
`oxidean-api-oxpilot/src/connector/gateway.rs`,
`oxidean-core/src/redact.rs`.

The tool surface is where model output becomes a forge effect — every
control below is server-side enforcement inside `invoke`, never prompt
policy. The model is an untrusted caller: its args are injection
attempts, its identity claims are worthless, and its inputs/outputs
are second-order injection surfaces in both directions.

## Registry design (NFR1.1–1.4, D-01)

```rust
// registry.rs — the closed catalog (23 rows, fixed order; a pin test
// asserts the set verbatim — see reliability-design/pins)
pub(crate) struct CatalogEntry {
    pub name: &'static str,          // model-visible tool name
    pub description: &'static str,   // model-facing docstring
    pub schema: fn() -> Value,       // parameters_json builder — json!
                                     //   is not const-constructible
    pub class: ToolClass,            // Read | Write — INTERNAL ONLY
    pub kind: EntryKind,             // the FIXED dispatch binding
    pub handler: HandlerFn,          // fn(&OxpilotTools, &InvokeCtx,
                                     //   Value) -> BoxFuture<Result<
                                     //   ToolOutput, ToolError>>
                                     // DEVIATION from the upstream-
                                     //   pinned fn(&OxpilotTools,
                                     //   ToolCtx, Value) -> Result<
                                     //   ToolOutput, ToolError>
                                     //   (entities.md:54): BoxFuture
                                     //   is required for fn pointers
                                     //   to async fns; InvokeCtx{ctx,
                                     //   session, entry} is an
                                     //   NFR-design-introduced type
}
pub(crate) enum EntryKind {
    Proc(&'static str),  // literal allowlisted proc — BR3.2/BR6.2/BR8.2
    Connector,           // C7 kind := spec.name verbatim — BR1.2/BR7.1
    Proposal,            // svc-bound (propose_diff/proposal_get)
    GatewayRead,         // gateway.list_targets / action_status
}
static CATALOG: &[CatalogEntry] = &[ /* exactly the 23 pinned names */ ];
```

Design points:

- `class` rides beside the spec — never ON the wire (D-01): the
  provider sees exactly `{name, description, parameters_json}` and
  a tool it can call carries no caller-mutable capability bit.
- `kind` makes the name→dispatch binding inspectable DATA, not code:
  the pin test asserts each `Proc` entry binds one of the BR3.2 ten /
  BR6.2 six / `pull.comments.create` literals and each `Connector`
  entry binds `spec.name` verbatim — without executing a handler.
  The security mechanism for the ~30 control/mutation Actions procs
  the registry exposes is ABSENCE from the table, never runtime
  blocklisting (BR6.3/NFR1.4).
- `handler`'s signature is a DELIBERATE DEVIATION from the READY
  upstream's pinned `fn(&OxpilotTools, ToolCtx, Value)
  -> Result<ToolOutput, ToolError>` (entities.md:54): the
  `BoxFuture` wrapper is required for fn pointers to async fns,
  and `InvokeCtx{ctx, session, entry}` is an NFR-design-
  introduced type carrying the resolved session + entry so
  handlers never re-lookup the row.
- `catalog()` maps the static rows into owned `ToolSpec`s — pure,
  fixed order, O(23); nothing a caller does can add a row.
- `oxpilot.diff_apply` has NO entry — the three diff procs are
  user-only `oxpilot.*` RPC verbs (C1); the model can never reach a
  materialization path by name-squatting a tool.

## Dispatch ladder (fail-closed ordering — NFR1.2/1.3/1.16/1.17)

```text
invoke(call, ctx):
 1. entry = CATALOG lookup call.name
      miss        -> ToolError{unknown_tool}      BEFORE schema/
                     handler/dispatch/audit/card — BR1.3
 2. args = serde_json::from_str(&call.args_json)
      unparseable -> ToolError{invalid_args:"args_json"}
 3. validate(&entry.schema(), &args)
      violation   -> ToolError{invalid_args} naming the field — BR1.4
 4. session = svc.get_session(ctx.user_id, ctx.session_id)
      dead/foreign-> ToolError{not_found}          — BR2.1
 5. (entry.handler)(self, &InvokeCtx{ctx, session, entry}, args)
      the write-class gate runs FIRST inside the handler — BR2.5
```

Nothing before step 5 can create an effect: schema rejection precedes
EVERY dispatch (NFR1.17 — the gateway never sees an unvalidated arg
string; a `command`/`path` arg is bounded before
`dispatch_action`/`list_targets` runs), and an early reject's only
record is the durable tool-call row U3 persists.

## Identity flow (NFR1.6–1.10)

```text
provider args ──call.args_json──> schema gate ──X (user_id/
    │                                            session_id/repo_id/
    │                                            target_id on a
    │                                            non-exec/fs tool
    │                                            => reject)
    ▼
ToolCtx{session_id, user_id, repo_id, proposal_id: None}
    ▲ minted by invoke_call from the OWNED session row
    │ (runtime.rs:456–461) — args can never rebind it
    │ (additionalProperties:false)
    ▼
svc.get_session(user_id, session_id) ──> OxpilotSessionRow
    (dead/foreign => not_found — anti-enumeration uniform)
    ▼
repo binding injection: {owner: row.repo_owner,
    name: row.repo_name} + ref default row.branch
    — args narrow WITHIN the bound repo only
    ▼
per-call synthesized RpcCtx (build_rpc_ctx_with_session +
  "oxpilot:<session_id>" id + ClientMeta::default())
    ──> rpc_dispatch ──> proc-level require_verified/
        ACL/flags/archived checks run verbatim
```

Design points:

- `ToolCtx` is the SOLE identity authority. The runtime mints it;
  the model never produces it; `proposal_id` lands as `None`
  (runtime.rs minted) so `proposal_id` is an explicit arg in v1 —
  the model-facing pin lives post-fetch in `proposal_get`, not on
  the ctx.
- `ctx.repo_id` is advisory — the resolved row is authoritative
  (NFR1.7); handlers take `repo_owner`/`repo_name`/`branch` from
  the session row, and repo-mutation inputs accept NO owner/name
  arg at all (`pull.comments.create` gets the pair injected).
- `proposal_get` enforces `row.session_id == ctx.session_id`
  POST-FETCH (NFR1.9, Q7-A) — `svc.proposal` is user-scoped; the
  pin is model-side only so `diff_apply`'s deliberate same-user
  reach (BR5.2) survives. A cross-session proposal reads as
  `not_found`, never `forbidden` — anti-enumeration uniform
  (NFR1.15/BR4.6).
- `target_id` is a TARGET SELECTOR only (NFR1.6): exec/fs accept
  it, the gateway's owner check verifies it — the tool resolves
  nothing about ownership itself; any OTHER tool rejects the field
  via `additionalProperties:false`.
- The synthesized session id is `oxpilot:`-namespaced — never a
  Uuid collision (NFR1.10); `ClientMeta::default()` — no
  fabricated IP/UA; `remember_me:false`, `expires_at:now+1h`
  (dead-weight flags inside the single invocation — no
  require_2fa proc is reachable).
- User-side `diff_list`/`diff_get`/`diff_apply` run the shipped
  proc gates `require_enabled → require_verified` on the CALLER's
  own session — the tool surface and the user surface share the
  same session service, never the tool's synthesized ctx.

## Write-gate ordering per class (NFR1.11–1.15, BR2.5)

```text
propose_diff:  session (ladder step 4) -> arg bounds + op-
               conditional requireds + diff<->ops path-set
               coherence (else invalid_args) -> svc.save_proposal
               {status:"pending"} -> card{diff} -> audit
pr_comment:    session -> ArtifactScope rebuild
               (find_oxpilot_proposals_by_session -> applied
               pr_refs) -> owner/name#number member of set
               else forbidden -> redact(body) ->
               pull.comments.create (UNANCHORED) -> audit
exec/fs trio:  session -> target resolution (explicit arg, or
               the connected-single fallback) ->
               gateway.dispatch_action (the gateway's own
               owner/caps/grant chain — U5 adds nothing)
diff_apply:    (USER PROC — no catalog entry) require_enabled ->
               require_verified -> svc.proposal (user-scoped) ->
               pending check -> apply-name precheck
               ("oxpilot/proposal-<id8>" verified UNPROTECTED,
               else apply_failed) -> caller-ctx rpc_dispatch
               chain — branchCreate{owner, name, branch:
               "oxpilot/proposal-<id8>", start: row.branch} ->
               repo.file.*{owner, name, ..., message:
               <synthesized>, branch: "oxpilot/proposal-<id8>"}
               (upload files[] entries carry content_base64) ->
               pull.create{owner, name, title, body,
               base_ref: row.branch, head_ref:
               "oxpilot/proposal-<id8>"} -> svc.mark_applied ->
               audit
```

Ordering invariant: no gate runs before session resolution; no
effect lands before its gate. Points:

- `propose_diff` inserts `pending` ONLY — the model can never set
  `applied`/`pr_ref`, never update in place, never touch git
  (NFR1.12); supersede-by-new is the only "edit" path.
- `pr_comment`'s scope check precedes the dispatch; a miss is a
  hard `forbidden`, never a silent no-op or a fallback target
  (NFR1.15); the call is UNANCHORED `pull.comments.create`
  (BR8.1) — a general PR conversation comment, no line/path
  anchors to forge.
- `diff_apply`'s chain runs on the CALLER's `ctx.rpc_dispatch` so
  every step inherits per-proc authorization (NFR1.13); the head
  is always a FRESH `oxpilot/proposal-<id8>` branch — never push
  to an existing branch, never force-update, never touch the
  default/protected branch; `branchCreate` on an existing name
  fails closed (NFR1.14). The per-step wire payloads are pinned:
  `branchCreate{owner, name, branch:"oxpilot/proposal-<id8>",
  start:row.branch}` — `start` is REQUIRED (omitted it defaults
  to the repo DEFAULT branch, not the session's bound one);
  every `repo.file.*` op carries `{owner, name, ...,
  message:<synthesized>, branch:"oxpilot/proposal-<id8>"}` —
  `message` is non-Option-required on delete/rename/upload and
  `branch` rides the flattened `RepoFileCommitOptions` wire
  field; upload `files[]` entries rename the op's `content_b64`
  to `content_base64`; `pull.create{owner, name, title, body,
  base_ref:row.branch, head_ref:"oxpilot/proposal-<id8>"}`.
  The `RepoFileCommitOptions` protected-branch fallback
  (protected `branch` + absent `new_branch` ⇒ silent
  `web-edit/*` retarget + auto-opened PR) is EXCLUDED, not
  accepted: the apply verifies the `oxpilot/proposal-<id8>`
  name is unprotected at apply start and fails `apply_failed`
  otherwise — commits can never silently retarget onto a
  generated branch and `pull.create` can never open a
  wrong-base PR.
- exec/fs pass `entry.spec.name` VERBATIM as the C7 kind — the
  args carry no `kind` field the model could rewrite (NFR1.16/
  BR7.1); the serialized `args` string is checked
  `<= ACTION_ARGS_MAX` (2 KiB) post-validation, pre-dispatch —
  bound and send the SAME string: the handler re-serializes the
  validated args, measures THAT string, and passes exactly that
  string into `dispatch_action`'s `args` param (never the raw
  `call.args_json`, whose whitespace padding could slip the
  check and hit the gateway's `head_bounded` truncation
  mid-JSON).

## Schema validator design (NFR1.3/1.8, NFR3.12, TS-5/TS-9)

```rust
// validate.rs — recursive keyword walker over the closed vocabulary
fn validate(schema: &Value, v: &Value, path: &mut String, depth: u8)
    -> Result<(), FieldErr>
// dispatch per PRESENT keyword:
//   type | required | enum
//   minLength|maxLength (chars) | minimum|maximum
//   minItems|maxItems
//   properties|items -> recurse, depth <= MAX_DEPTH (16)
//   additionalProperties:false -> unexpected-key reject
//   pattern -> closed matcher table (hand-authored fns — NO regex
//              dep; the only authored patterns are path-shape
//              rules: no "..", no leading "/")
```

- `additionalProperties:false` IS the identity-rejection
  mechanism — `user_id`/`session_id`/`repo_id`-shaped args die
  here (NFR1.6); every authored schema sets it.
- `pattern` resolves through a lookup table of authored matcher
  fns, not a regex engine — the registry authors every pattern and
  the crate carries no regex dep (TS-9); an unrecognized pattern
  is a registry defect, never a lenient pass.
- Errors name the dotted field path (`files[2].new_path`,
  `command`) — feeding `invalid_args`'s "name the field" contract
  (BR1.4); op-conditional requireds stay handler-side (schema
  can't express them, TS-5).
- Bounded by construction: depth cap 16 vs authored depth ≤3;
  O(schema keywords × args nodes); no regex sweeps, no external
  validation service, no draft-engine overhead.

## Denial + error mapping (NFR1.19, NFR4.5)

```text
map_err(AppError{code, data}) — shared by every proc/gateway arm:
  code == "oxpilot.action_denied":
    data.denied.reason missing/unknown -> ToolError{internal}
    "consent_required" + grant_id      -> consent card +
                                          SUCCESS ToolOutput
    "not_connected"|"offline"|
    "capability"                       -> ToolError{action_denied}
                                          — reason verbatim in msg
  code in {not_found, forbidden,       -> verbatim ToolError{code}
           invalid_args}                  (the PINNED verbatim
                                          set — exactly three)
  ALL other codes                      -> ToolError{internal} +
    (internal/unknown, rpc.bad_input,     generic message — never
     auth.*, db.*, repo.*, oxpilot.*)     resource-leaking text; a
                                          U5-side input defect is
                                          internal, never a named
                                          arm
```

The denial marker is matched on the STRUCTURAL `data.denied`
object — never on message text, never on error type; a missing or
malformed marker on an `oxpilot.action_denied` code fails CLOSED
to `internal` (NFR1.19). `ToolError.code` is card/test-internal
only — `invoke_call` feeds the model the `message` string
(runtime.rs:580–588), so every model-actionable instruction rides
`message`/`content`.

## Consent output semantics (NFR1.20, NFR4.6)

The `consent_required` output text is a correctness pin, not prose:

```text
"consent requested — grant <grant_id> pending; the action queues
 on approval — do NOT re-issue this call"
```

- Success-shaped `ToolOutput` (not `ToolError`) — a pending grant
  IS progress; an error invites the exact re-issue that mints a
  second grant and a second action post-approval (the
  `action_once`/`action_session` double-materialization hazard
  U6's dedupe can't absorb).
- Carries `grant_id`; never promises a status follow-up — the
  model holds no `action_id` on this path and `action_status`
  looks up actions only, so the text must not invite polling.
- The consent card reuses the one `emit_card` helper
  (`card{consent, ref_id:grant_id}` — BR7.4 via the BR4.5 seam).

## Redaction call-sites (NFR2.1–2.4)

| surface | who redacts | when |
|---|---|---|
| `ToolOutput.output_summary` | the handler via `oxidean_core::redact` | before return (U3 re-redacts + bounds at the clamp — the tool stays under) |
| `pr_comment` body | the HANDLER, pre-dispatch | `pull.comments.create` only `validate_body`s; the body is public-artifact egress — redact before the write |
| audit `detail` | the audit helper | every user-derived field bounded + redacted before `record_with` |
| proposal `title`/`diff_body` | `save_proposal` internally (services.rs:637–638) | service boundary — the tool does NOT double-wrap |
| exec/fs stored `args`/`args_summary` | the gateway | U5 hands the raw string to `dispatch_action` and never retains it (NFR2.2 — no independent U5 copy/log/summary) |
| `ActionView` | pass-through VERBATIM | gateway already bounded + redacted; U5 performs no un-redaction |
| applied `pr_ref` → `pr_comment` scope | none needed | `owner/name#number` is a ref, not content (NFR2.3) |

`redact` is heuristic + idempotent (masks credential-shaped
tokens, bearer tokens, JWTs, PEM blocks) — the design never
claims it is an exhaustive secret net; it is the mitigation layer
each egress point applies, not a guarantee (NFR2.4).

## Audit detail construction (NFR5.x hooks)

`audit(&self, ctx, event, target, detail)`: resolve the actor via
`state.db.find_user_by_id(&ctx.user_id)` →
`Some((id, username))` on hit, else `None` (the event still
emits — Q6-A); `detail` carries ids/counts/refs ONLY —
`{session_id, proposal_id, title (≤TITLE_MAX, redacted),
file_count}` / `{session_id, proposal_id, branch, pr_ref}` /
`{session_id, pr_ref, comment_id}` — then
`audit::record_with(&self.state.db, &ClientMeta::default(), actor,
event, target, Some(detail.to_string()))` AFTER the effect
commits. No diff bodies, no file contents, no command strings, no
raw args, no result payloads (NFR2.1/NFR5.6).

## Threat model → mechanism map

| threat | mechanism |
|---|---|
| Hostile args mint a proc/kind/identity | closed `&'static` registry + `EntryKind` literal bindings + `additionalProperties:false` — absence, not filtering |
| Confused deputy (tool executes as the user) | ToolCtx sole identity + per-call session re-resolve + session-row repo injection + ArtifactScope |
| Cross-session proposal read | post-fetch `row.session_id == ctx.session_id` ⇒ `not_found` (model side only — BR4.6/Q7) |
| Cross-user target dispatch | `target_id` is a selector; gateway owner check verifies — U5 adds nothing to the chain |
| Post-approval re-issue double-executes | consent output pins do-not-re-issue inside the success text; grant dedupe absorbs only pending-window re-dispatch |
| Model-authored secret into rows/artifacts | `redact` at every egress: summary, comment body, audit detail; service/gateway own their own boundaries |
| Indirect injection via tool output | output-side instruction-safety is the MODEL's — every attempted action still lands inside the registry |
| `denied` marker spoofed/omitted | structural decode only — missing/malformed/unknown ⇒ `internal`, never message-sniff |
| Schema-bypass giant/nested args | closed-vocabulary validator + depth cap + per-field bounds before any dispatch |
| Markup injection into persisted/public text | `redact` masks credentials only — U8 renders diff content escaped/plain and the forge renderer owns comment markdown (explicit boundary hand-off, not a U5 residual) |
