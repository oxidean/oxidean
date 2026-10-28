# Reliability Design — oxpilot-tools (U5)

Consumes: `nfr-requirements/reliability-requirements.md`
(NFR4.1–4.13), `functional-design/rules.md` (BR1/BR3/BR4/BR5/BR7/BR9),
`entities.md` (`DeniedOutcome`, `ActionView`, `ToolError`),
`nfr-design-questions.md` (Q4/Q5/Q7), shipped `runtime.rs` (terminal
CAS, `invoke_call` clamp, 60s race, `NullTools`), `services.rs`
(`mark_applied` guard), `audit.rs` (`record_with` swallow),
`connector/gateway.rs` (`denied` marker).

The unit's reliability contract is a TOTAL failure taxonomy: every
failure path through the dispatch ladder lands on exactly one typed
`ToolError` arm — never a panic, never a silent default, never a
partial outcome presented as success.

## The fail-closed taxonomy (NFR4.1)

| failure | `ToolError.code` | message carries |
|---|---|---|
| unregistered `call.name` | `unknown_tool` | `unknown tool <name>` |
| unparseable `args_json` / schema violation | `invalid_args` | the dotted field path + reason |
| dead/foreign session; foreign/absent proposal or action | `not_found` | uniform — anti-enumeration |
| ArtifactScope miss; write-gate refusal | `forbidden` | which gate refused |
| zero connected targets | `no_target` | pair-a-target guidance |
| >1 connected targets | `ambiguous_target` | `pass target_id — see targets_list` |
| `denied{not_connected|offline|capability}` | `action_denied` | the denied reason verbatim |
| apply on non-`pending` proposal | `diff_not_pending` | `proposal not pending` |
| apply step failure | `apply_failed:<step>` | the proc that failed |
| malformed `denied` marker; unknown reason; proc `internal`/unknown | `internal` | generic — never resource-leaking |
| missing executor | `no_tools` (NullTools — shipped, U3's arm) | `no tool executor installed` |

- `code` is internal/test-visible ONLY — `invoke_call` feeds the
  model the `message` string (runtime.rs:580–588); every
  model-actionable instruction rides `message`/`content`:
  `ambiguous_target` says pass `target_id`, `action_denied` names
  the reason, `no_target` says pair a target (NFR4.4).
- Proc `AppError`s map against a PINNED verbatim set (NFR4.5):
  exactly {`not_found`→`not_found`, `forbidden`→`forbidden`,
  `invalid_args`→`invalid_args`}; ALL other codes —
  `internal`/unknown, `rpc.bad_input`, `auth.*`, `db.*`,
  `repo.*`, `oxpilot.*` — ⇒ `internal` + generic message. No
  laundering, no re-encoding: a U5-side input defect (e.g.
  `rpc.bad_input` from a malformed synthesized request) is
  `internal`, never a named arm — so every proc failure still
  lands on exactly one typed arm (the totality claim holds).
- The ladder ordering makes the taxonomy disjoint: name-before-
  schema-before-session-before-handler means an early reject can
  never reach a code path that produces a different arm.

## Denial decode — fail closed on structure (NFR4.3)

```text
map_denied(AppError{code, data}):
  code != "oxpilot.action_denied"         -> the pinned verbatim-set
                                            mapping above ({not_found,
                                            forbidden, invalid_args}
                                            verbatim; all else internal)
  data.denied absent / not an object    -> ToolError{internal}
  reason == "consent_required" && grant_id present
                                        -> card{consent} + success out
  reason in {not_connected, offline, capability}
                                        -> ToolError{action_denied}
  anything else                         -> ToolError{internal}
```

A `consent_required` without a usable `grant_id` falls to
`internal` — a consent card with a null ref is malformed C2 (the
`message_queue` precedent emits nothing on a missing ref,
session.rs:341).

## Consent — progress, not failure (NFR4.6)

Output is `ToolOutput` (the success variant), never `ToolError`:
`"consent requested — grant <id> pending; the action queues on
approval — do NOT re-issue"`. `respond_grant` materializes the
Action server-side from the grant's stored args — the model can
never poll a grant→action status (none exists; `action_status`
takes `action_id` only), so the output promises no follow-up and
the text pins no-re-issue inside the success surface.

## Async degrade (NFR4.7)

- `dispatch_action` returns `{action_id, state:"queued"}` when the
  gateway's insert returns — the tool never waits on a connector
  inside `invoke`; connector results arrive async and ride
  `action_status` on a later round.
- `ActionView.state` is an OPEN vocabulary at the tool: the
  handler renders whatever the gateway returns (the shipped
  7-state set today); an unrecognized state passes through
  verbatim rather than choking — a U6 vocabulary extension never
  breaks U5.

## Emit + audit degrade (NFR4.8/4.9)

- `emit_card`: `turns().get` returns `None` after the turn's map
  entry releases, and `emit_frame` drops frames once
  `terminal_sent` CASes (runtime.rs:113–122) — BOTH paths drop
  silently. The durable tool-call row is the guaranteed record; a
  dropped `card{diff|consent}` is cosmetic. The `message_queue`
  `card{queued}` precedent (session.rs:326–341) is the shape this
  helper mirrors.
- `audit::record_with` swallows its own failure (audit.rs:41–56,
  warn-level log) — post-commit ordering means a broken audit
  sink never fails a committed effect.

## Mutation integrity (NFR4.10–4.12)

- `propose_diff` has NO dedupe — a re-issued call mints a second
  `pending` row; supersede-by-new is the only edit path.
  Idempotency is neither claimed nor approximated (NFR4.10).
- `diff_apply` is fail-stop: the ordered sequence
  (`repo.branchCreate` → `ops_json` ops → `pull.create`) halts on
  the FIRST failure and returns `apply_failed:<step>` naming the
  proc — NO automatic rollback (the partial branch is
  user-deletable via `repo.branchDelete`), NO auto-retry, the
  proposal stays `pending`, and a leftover branch surfaces as
  `apply_failed:branch_exists` on retry (NFR4.11).
- Double-materialization is impossible: the CONCURRENCY guard
  is the deterministic `oxpilot/proposal-<id8>` branch name —
  a second concurrent apply collides at `branchCreate` and its
  chain dies `apply_failed:branch_exists` before ever reaching
  `mark_applied` (NFR4.12). `svc.mark_applied`'s ownership +
  `status != "pending"` ⇒ `diff_not_pending` recheck
  (services.rs:665–666) covers SEQUENTIAL retries only — it is
  a non-atomic check-then-write (`mark_proposal_applied` issues
  an unconditional `UPDATE ... WHERE id=?` with no
  `AND status='pending'` predicate on any dialect,
  db/oxpilot.rs:1898–1929); the branch-name collision is what
  serializes concurrent applies. Optional tightening: add
  `AND status='pending'` to the UPDATE and map `rows_affected
  == 0` ⇒ `diff_not_pending`, making the db write itself
  atomic.

## Honest terminals (NFR4.13)

`consent_required` never claims the action ran; `action_denied`
never hints a retry succeeds; `apply_failed` never claims cleanup
happened; `not_found` never distinguishes dead-vs-foreign. Every
terminal string states exactly what the system did — nothing
more.

## Panic/timeout posture

- No handler path panics by design: every fallible step maps into
  `ToolError`. A truly unexpected panic inside the invoke future
  is a defect class, not a mapped outcome — U3's 60s race + the
  durable card row bound the blast radius, but the taxonomy
  deliberately has no arm that pretends to recover it.
- A handler exceeding `TOOL_CALL_TIMEOUT` (60s) is cancelled by
  U3's race — the per-class dispatch counts (performance-design)
  keep every well-formed call far inside it, so the race is a
  backstop, not an expected path (NFR3.1).

## Reliability pins the design must satisfy

- The shared CATALOG closure pin test asserts the exact 23-name
  set + each entry's `kind` binding verbatim — a drifted registry
  fails at compile/test time, not at a model call (BR1.1/NFR1.1).
- Early rejects produce the durable U3 tool-call row as their
  ONLY record — `invoke` never writes a side-channel.
- `NullTools` (shipped) already closes the missing-executor path
  with `no_tools` — U5 owns no fallback for it.
