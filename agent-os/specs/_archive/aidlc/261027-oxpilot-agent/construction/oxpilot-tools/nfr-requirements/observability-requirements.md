# Observability Requirements — oxpilot-tools (U5)

Consumes: functional-design `rules.md` BR9 (all), `entities.md`
(ToolAuditRecord), `functional-spec.md` Observability, questions Q6-A,
`inception/requirements-analysis/requirements.md` NFR5 (write-class
mutations attributable to the user) + NFR6 (reuse existing surfaces —
no new observability stack), `audit.rs` (`record`/`record_with`),
`oxidean-db` `find_user_by_id`.

## Audit events — write-class only, post-commit

| id | event(s) | emitted when | detail fields (bounded, redacted) |
|----|----------|--------------|-----------------------------------|
| NFR5.1 | `oxpilot.proposal_created` | `propose_diff`'s pending row commits | session_id, proposal_id, title (redacted), file_count |
| NFR5.2 | `oxpilot.diff_applied` | `mark_applied` lands inside the user proc's apply success | session_id, proposal_id, branch, pr_ref |
| NFR5.3 | `oxpilot.artifact_comment` | `pull.comments.create` returns a comment_id | session_id, pr_ref, comment_id |

| id | requirement | basis |
|----|-------------|-------|
| NFR5.4 | All three emit via `audit::record_with(&state.db, &ClientMeta::default(), actor, event, target, detail)` — the ctx-free entry point (the tool holds `ToolCtx`, not `RpcCtx`); `actor` is `Some((ctx.user_id, username))` resolved via `find_user_by_id`, falling back to `None` rather than skipping the event. Emission is AFTER the effect commits and failure is swallowed — auditing a failed write fabricates an event that never happened; a broken sink never fails a committed effect | BR9.1, BR9.3, `audit.rs:25`, `lib.rs:3131` |
| NFR5.5 | Read-class tools emit ZERO audit rows — the durable tool-call card row U3 persists (name, bounded args/output summaries, status) is already the read trail; a per-read audit row is noise the C6 class rule explicitly exempts | BR9.2, C6 |
| NFR5.6 | exec/fs dispatch gets NO U5 audit row — U6's `oxpilot.action_dispatched` (plus the consent/grant lifecycle rows) already covers the same effect; U5 never double-audits | BR9.1, U6-BR7 |
| NFR5.7 | Audit `detail` carries ids, counts, refs, and bounded redacted fields (`title` ≤ TITLE_MAX, redacted) ONLY — never diff bodies, file contents, command strings, tool args, or result payloads beyond summary length; every user-derived field passes `oxidean_core::redact` before dispatch | BR9.3, NFR2.4 |

## The durable read trail

| id | requirement | basis |
|----|-------------|-------|
| NFR6.1 | The U3-persisted tool-call card row is the per-invocation record — tool name, `args_summary` ≤ ARGS_SUMMARY_MAX (2 KiB, redacted), `output_summary`/error ≤ OUTPUT_SUMMARY_MAX (2 KiB, redacted), status — U5 guarantees the bounded inputs it feeds (NFR3.7); every read-class call is visible there without an audit row | BR9.2, `runtime.rs:500/582–586` |
| NFR6.2 | Card frames (`card{diff\|consent, ref_id}`) are emitted through the same `turns().get` → `emit_frame` seam `message_queue` uses — buffered for replay + fanned to live SSE; best-effort after terminal (NFR4.8), never a separate notification channel | BR4.5, BR7.4 |

## Logging conventions

| id | requirement | basis |
|----|-------------|-------|
| NFR6.3 | Degraded-path logging rides the existing `tracing` surface: `warn!` on a swallowed audit failure or a dropped post-terminal emit; `error!` reserved for actual defects (registry inconsistency, unreachable states). Model-caused denials are NORMAL `ToolError`s — they log nothing beyond the card row; no per-call `info!` spam on the invoke hot path | NFR6, BR9.3 |
| NFR6.4 | Log/audit fields carry ids and counts ONLY for tracing, and ids, counts, refs, and bounded redacted fields for audit detail — a `tracing` field or audit detail carrying tool args, diff bodies, command strings, or file contents is a defect (same secret class as BR9.3) | BR9.3, NFR2 |
| NFR6.5 | No new observability stack: cards (durable rows + frames), the three audit events, and existing `tracing` — metrics stay on the route/proc surfaces the repo already carries | NFR6 |

## Scope boundary

- Consent/grant/action lifecycle audit (`action_dispatched`,
  `connect_grant_*`, `action_grant_*`, `action_result`,
  `action_cancelled`) is U6's vocabulary — U5 emits the three
  write-class rows above and nothing else on `audit_events`.
- Transcript storage redaction (the persisted message/tool-call
  content itself) binds U2/U3 — U5's duty is the bounded, pre-redacted
  strings it hands those writers.
