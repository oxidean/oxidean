# Observability Design — oxpilot-tools (U5)

Consumes: `nfr-requirements/observability-requirements.md`
(NFR5.1–5.7 audit; NFR6.1–6.5 telemetry), `rules.md` (BR9),
`entities.md` (`ToolAuditRecord`), `functional-spec.md`
(Observability), `nfr-design-questions.md` (Q5/Q6), shipped
`audit.rs` (`record_with` + `DeferredAudit` precedent),
`runtime.rs` (durable tool-call row; `ARGS_SUMMARY_MAX`/
`OUTPUT_SUMMARY_MAX`), `session.rs` (`card{queued}` precedent),
`oxidean-db` (`find_user_by_id`).

Observability here is deliberately thin: U5 owns THREE audit
events and a card-emit helper — reads leave no audit rows (the
durable U3 tool-call card is the read trail), and exec/fs
dispatch/grant/action lifecycle audits are U6's domain (the
gateway emits them). Everything below runs post-commit and
best-effort; observability never fails a committed effect.

## Audit emission design (NFR5.1–5.4, NFR5.7)

One shared helper on `OxpilotTools` — the single call site for
all three events:

```rust
async fn audit(&self, ctx: &ToolCtx, event: &'static str,
               target: Option<(&'static str, &str)>, detail: Value) {
    let actor = match self.state.db.find_user_by_id(&ctx.user_id).await {
        Ok(Some(u)) => Some((ctx.user_id.as_str(), u.username.as_str())),
        _ => None,                       // miss/error — event still
                                         //   emits (NFR5.4 fallback)
    };
    audit::record_with(&self.state.db, &ClientMeta::default(), actor,
                       event, target, Some(detail.to_string())).await;
}
```

- `record_with(&db, &client_meta, actor, event, target, detail)`
  is the shipped shape — U5 calls it verbatim with
  `ClientMeta::default()` (a server-side emit fabricates no
  IP/UA); post-commit ordering + `record_with`'s internal swallow
  mean the emit can never fail the mutation it describes
  (NFR4.9/NFR5.3).
- Actor: `Some((user_id, username))` on `find_user_by_id` hit —
  the join `admin.users.getActivity` renders (audit.rs doc);
  `None` on miss/error — the row lands actorless rather than not
  at all (NFR5.4). NO actor cache — the executor is stateless and
  a username map goes stale on rename (Q6-A).
- The DeferredAudit shape is NOT used: U5's three events emit
  AFTER a single committed effect each, so `record_with`
  directly; the gateway's deferred pattern exists to roll audits
  back with failed mutations, which isn't U5's flow.

## The three events + their detail builders (NFR5.1/5.2/5.6)

| event | emitted when | target | detail (ids/counts/refs ONLY) |
|---|---|---|---|
| `oxpilot.proposal_created` | `svc.save_proposal` commits the `pending` row | `("oxpilot_proposal", proposal_id)` | `{session_id, proposal_id, title (≤ TITLE_MAX, redacted), file_count}` |
| `oxpilot.diff_applied` | `svc.mark_applied` lands (inside `diff_apply`) | `("oxpilot_proposal", proposal_id)` | `{session_id, proposal_id, branch, pr_ref}` |
| `oxpilot.artifact_comment` | `pull.comments.create` returns a comment id | `("pull_request", pr_ref)` | `{session_id, pr_ref, comment_id}` |

- `target` types follow the shipped `oxpilot_<noun>` convention
  (`oxpilot_grant`/`oxpilot_action` in the gateway);
  `pull_request` is the existing repo-side type for PR targets.
- Detail is CONSTRUCTED data — ids, counts, refs, one bounded
  redacted string field — serialized to a bounded JSON string.
  NEVER diff bodies, file contents, command strings, raw args,
  or result payloads (NFR5.6/NFR2.1); the same rule governs the
  user-side `diff_apply` emit.

## Reads — the card IS the record (NFR5.5)

Read tools write NO audit rows. Their trail is the durable
tool-call card U3 persists per `invoke_call` — `{name,
bounded+redacted args_summary, output_summary, code}` —
which is the full audit surface a repo read needs (who asked,
what was asked, how it resolved). Adding `oxpilot.read` audit
rows would double-write every model fetch for zero new signal;
the rows are already there.

## Cards as emitted telemetry (NFR6.1–6.3)

`emit_card(&self, session_id, card_kind, ref_id)` —
`state.oxpilot.turns().await.get(session_id).await` →
`Some(turn)` ⇒ `emit_frame(&turn, {"type":"card",
"card_kind":kind, "ref_id":ref_id})`; `None` or a post-terminal
frame drops silently (the `card{queued}` precedent,
session.rs:326–341; the terminal CAS drop, runtime.rs:113–122).
Card emits are PINNED to synchronous execution inside the
awaited `invoke` — no detached/spawned emission: `get()` can
only ever resolve the INVOKING turn because `release` post-dates
the turn task and `drain_and_chain` claims the successor only
after release under the session admission lock
(runtime.rs:1062–1149); a detached emit could land on a
SUCCESSOR turn's replay buffer — misattribution strictly worse
than the silent drop.

| card | emitted on | ref_id |
|---|---|---|
| `card{diff}` | `save_proposal` commits | `proposal_id` |
| `card{consent}` | `denied{consent_required}` decode | `grant_id` |

Cards are render cues for live sessions — best-effort, never
retried, never the system of record. The durable tool-call row +
the proposal/grant rows are authoritative; a dropped card is
cosmetic (NFR4.8).

## Tracing posture (NFR6.4/6.5)

- U5 adds NO new tracing instrumentation of its own — the seam
  layers already carry the spans: `invoke_call` logs tool errors
  at warn with `{session_id, tool, code}` (runtime.rs), the
  gateway traces dispatch/grant paths, the proc layer traces
  dispatch failures, `record_with` warns on audit-write failure.
- Where the design DOES depend on existing fields: `target_id`
  on exec/fs paths flows into the gateway's `action_id`/
  `target_id` correlation, and `proposal_id`/`pr_ref`/`grant_id`
  on the cards + audit rows are the join keys an operator
  follows tool-call → proposal → apply with.
- Severity: audit-write failure is warn (shipped
  `record_with` behavior — swallowed, logged); emit drops are
  silent-by-design (no log — a terminalled turn is an expected
  drop, not an event worth a line); `internal` ToolErrors arrive
  at warn via `invoke_call`'s error path.
- The unit emits NO metrics of its own — call counts/latency are
  derivable from the durable tool-call rows + the gateway's
  action rows; U5 adds nothing that needs a counter (NFR6.5's
  "no new observability surface" posture).
