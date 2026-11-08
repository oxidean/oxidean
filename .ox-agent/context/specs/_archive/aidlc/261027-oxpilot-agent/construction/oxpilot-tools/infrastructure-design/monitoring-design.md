# Monitoring Design — oxpilot-tools (U5)

Consumes: `nfr-design/observability-design.md`,
`nfr-requirements/observability-requirements.md` (NFR5.1–5.7,
NFR6.1–6.5), `functional-design/rules.md` (BR9),
`entities.md` (`ToolAuditRecord`).

NFR6.5 forbids new metrics surface — monitoring is three audit
events + cards + durable tool-call rows riding the existing
`audit` table, turn frame sink, and `tracing` pipeline.

## Operator signals

| signal | emission | reads |
|--------|----------|-------|
| `oxpilot.proposal_created` | `record_with` post-`save_proposal` commit; target `("oxpilot_proposal", proposal_id)`; detail `{session_id, proposal_id, title ≤TITLE_MAX redacted, file_count}` | a model proposal landed |
| `oxpilot.diff_applied` | `record_with` post-`mark_applied`; detail `{session_id, proposal_id, branch, pr_ref}` | apply chain committed; join to PR |
| `oxpilot.artifact_comment` | `record_with` post-`pull.comments.create`; target `("pull_request", pr_ref)`; detail `{session_id, pr_ref, comment_id}` | model commented on an artifact |
| `card{diff}` / `card{consent}` | `emit_card` → `turns().get` → `emit_frame` pub(crate) | best-effort render cue on the live turn |
| tool-call row | U3's durable row per invoke | the read trail — reads emit no audit rows (NFR5.5) |
| `internal` ToolError | `debug!` on `invoke_call`'s error arm (runtime.rs:620, `{session_id, tool, call_index, status}`) — below default `info` level | unexpected handler failure — the durable tool-call row is the operator-visible signal |
| actor | `find_user_by_id` → `Some((id, username))` else `None` | `admin.users.getActivity` join; event still emits on miss |

## What's absent by design

- No metrics exporter, alert rules, or dashboards — call counts
  and error codes are derivable from the durable tool-call rows
  the runtime already persists (NFR6.5).
- No per-tool audit row on every invocation — that double-writes
  every model fetch; the durable card already records
  who/what/how for reads.
- No content in any line — detail builders carry ids, counts,
  refs, and bounded redacted fields only (NFR5.2/5.6, NFR2.1–2.4).

## Failure-shape reading

- `diff_apply` failure ⇒ `apply_failed:<step>` terminal in the
  tool output — the `pending` row + self-named
  `oxpilot/proposal-<id8>` branch are the leftover to inspect;
  no rollback machinery exists by design (NFR4.11).
- `denied`/`consent` arms are success-shaped, not errors — a run
  of consent cards with no `diff_applied` audit means grants are
  stalling upstream in the gateway (U6's surface).
- Missing `oxpilot.*` audit rows while tool-call rows persist ⇒
  `record_with` swallow is firing — audit failure never fails a
  committed effect (NFR4.9/5.3); check db health, not the tool.
