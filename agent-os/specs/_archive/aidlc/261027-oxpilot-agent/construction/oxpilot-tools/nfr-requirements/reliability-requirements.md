# Reliability Requirements — oxpilot-tools (U5)

Consumes: functional-design `rules.md` BR1/BR3/BR4/BR5/BR7/BR9,
`entities.md` (DeniedOutcome, ActionView), `functional-spec.md`
(Negative paths, WF5/WF6), questions Q4-A,
`inception/requirements-analysis/requirements.md` NFR4 (provider/connector
degrades surface in-session, never silently — U5's half is the typed
fail-closed taxonomy the model and the card both read).

## Fail-closed taxonomy — every failure is typed and recorded

| id | requirement | basis |
|----|-------------|-------|
| NFR4.1 | Every failure path returns a TYPED `ToolError` — `unknown_tool` (unregistered name), `invalid_args` naming the field (schema), `not_found` (dead/foreign session, proposal, action — anti-enumeration uniform), `forbidden` (ArtifactScope/write gates), `no_target`/`ambiguous_target` (target resolution), `action_denied` carrying the gateway reason, `diff_not_pending`, `apply_failed:<step>` — never a panic, never a silent no-op, never a success-shaped lie | BR1.3, BR1.4, BR2.1, BR4.6, BR7.3–7.6, BR5.2, BR5.7, BR8.1 |
| NFR4.2 | A missing executor degrades to the SHIPPED `NullTools` — every call answers `no_tools` ("no tool executor installed") fed back to the model normally; the absent-tools failure mode is closed by construction, distinct from `unknown_tool` (which is U5's own registry-miss) | WF1, `oxpilot.rs:127–141` |
| NFR4.3 | Denial decoding is fail-closed on STRUCTURE: `oxpilot.action_denied` without the `data.denied` marker, or with an unrecognized reason ⇒ `ToolError{internal}` — never parsed leniently, never keyed on message text or error type | BR7.4, `denied()` |
| NFR4.4 | `ToolError.code` is internal/test-visible ONLY — `invoke_call` feeds the model just the redacted `message` (runtime.rs:585–588): every model-actionable instruction MUST ride `message`/`ToolOutput.content` (ambiguous_target says pass `target_id`; action_denied names the reason; consent output says do-not-re-issue). A fix expressed only in `code` is invisible to the model — a defect class the taxonomy can't tolerate | BR3.4, BR7.3, BR7.4 |
| NFR4.5 | Proc `AppError`s map into `ToolError` by code VERBATIM (`not_found`→`not_found`, `forbidden`→`forbidden`; internal/unknown ⇒ `internal` + generic message) — no laundering that leaks which resource failed or garbles proc semantics | BR3.4 |

## Degrade semantics

| id | requirement | basis |
|----|-------------|-------|
| NFR4.6 | `consent_required` is success-shaped PROGRESS, not an error: consent card emitted + `ToolOutput` "consent requested — grant <id> pending; the action queues on approval — do NOT re-issue". `respond_grant` on allow materializes the Action from the grant's stored args with NO model involvement — the model holding only `grant_id` can never poll a nonexistent grant→action status | BR7.4, U6-WF2 |
| NFR4.7 | Dispatch is asynchronous: success returns `{action_id, state: queued}` immediately; a connector-side disconnect/offline/capability failure degrades through the GATEWAY's own denial or later `action_status` states — never a U5 hang or a U5-invented state. `ActionView.state` is treated as an OPEN vocabulary — an unrecognized state renders verbatim rather than choking the tool | BR7.5, BR7.6, entities ActionView |
| NFR4.8 | Card emits are best-effort: the `turns().get` → `emit_frame` path drops silently when the turn already terminalled (the terminal CAS closes the replay buffer — runtime.rs:113+) — the durable tool-call row is the guaranteed record; a dropped card is cosmetic, never a lost effect | BR4.5, BR7.4 |
| NFR4.9 | Audit emission is post-commit best-effort: `audit::record_with` failure is swallowed — a broken audit sink NEVER fails a tool effect that already committed | BR9.1, BR9.3, `audit.rs:25` |

## Mutation integrity

| id | requirement | basis |
|----|-------------|-------|
| NFR4.10 | `propose_diff` has NO dedupe — a re-issued call mints a SECOND pending row (supersede-by-new-proposal is the model's only edit path; there is no in-place update). Stated plainly: idempotency is neither claimed nor approximated | BR4.1, entities ProposedDiff |
| NFR4.11 | `diff_apply` is fail-stop: the ordered sequence (branchCreate → file ops → pull.create) halts on the first failure and returns `apply_failed:<step>` naming the proc — NO automatic rollback (the partial branch is user-deletable via `repo.branchDelete`), NO auto-retry; the proposal stays `pending` so the user can retry after cleanup, and a leftover branch surfaces as `apply_failed:branch_exists` on retry | BR5.7, WF5 |
| NFR4.12 | Double-materialization is impossible: a successful apply marks the row `applied` via `svc.mark_applied` (ownership + `status != "pending"` ⇒ `diff_not_pending` re-enforced inside the service) — a second apply hits `diff_not_pending`, never a second branch/PR | BR5.2, BR5.5 |
| NFR4.13 | Every announced terminal is a real terminal: `consent_required` never claims the action ran; `action_denied` never hints a retry will succeed; `apply_failed` never claims cleanup happened — the model's next move is always grounded in what actually persisted | BR7.4, BR5.7, Negative paths |
