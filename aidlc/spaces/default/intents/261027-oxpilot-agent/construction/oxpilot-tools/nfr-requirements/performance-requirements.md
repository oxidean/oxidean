# Performance Requirements — oxpilot-tools (U5)

Consumes: functional-design `rules.md` BR3/BR4/BR6/BR7/BR8/BR9,
`functional-spec.md` Pins, `entities.md` (OxpilotTools, ArtifactScope),
questions Q3-A/Q5-A, shipped constants (`runtime.rs`
ARGS_SUMMARY_MAX/OUTPUT_SUMMARY_MAX = 2 KiB, TOOL_OUTPUT_MAX = 32 KiB,
TOOL_CALL_TIMEOUT = 60 s, MAX_TOOL_ROUNDS = 8; `oxpilot.rs`
SUMMARY_MAX = 512 B, RESULT_MAX = 2 KiB, ACTION_ARGS_MAX = 2 KiB;
`oxidean-api-core/src/repo/files.rs` UPLOAD_MAX_FILES = 50).

## Invocation cost — inside the 60 s tool window

| id | requirement | basis |
|----|-------------|-------|
| NFR3.1 | Every `invoke` completes well inside TOOL_CALL_TIMEOUT (60 s — the U3 race cancels past it): arg validation is O(args) over a closed schema vocabulary, session re-resolution is one indexed service read, and no handler performs unbounded work before its first dispatch | BR1.4, BR2.1, `runtime.rs:48` |
| NFR3.2 | A read-class invoke makes exactly ONE `rpc_dispatch` call — never a retry loop, never auto-pagination inside the call. `actions_get_job_log` never passes `wait_ms` (a long-poll inside the tool window starves the turn loop); incremental reads ride `offset`/`next_offset` across calls | BR3.2, BR6.4, WF3 |
| NFR3.3 | exec/fs dispatch returns when the gateway's insert returns — the tool NEVER blocks inside `invoke` waiting on a connector; the connector result arrives asynchronously and rides `action_status` on a later round/turn | BR7.5, AC4.3.2 |
| NFR3.4 | Target resolution costs one `list_targets` call only on the no-arg fallback; the arg-present path adds zero queries beyond the gateway's own chain | BR7.3 |
| NFR3.5 | `pr_comment`'s ArtifactScope is ONE session-scoped proposals query (`find_oxpilot_proposals_by_session`) per call — O(applied proposals in the session), rebuilt fresh; no cache to invalidate | BR8.1, entities, Q10-A |
| NFR3.6 | The USER proc `diff_apply` is the only multi-proc path and is bounded by construction: 1 `repo.branchCreate` + ≤1 batched `repo.file.upload` (create/update ops batch into ONE commit, ≤ UPLOAD_MAX_FILES = 50) + ≤50 per-op delete/rename + 1 `pull.create` — ≤ ~53 sequential dispatches on the caller's ctx | BR5.3, BR4.4, `oxidean-api-core/src/repo/files.rs:28` |

## Payload bounds

| id | requirement | basis |
|----|-------------|-------|
| NFR3.7 | `ToolOutput.content` ≤ TOOL_OUTPUT_MAX (32 KiB) with an explicit `truncated` marker; `output_summary` ≤ OUTPUT_SUMMARY_MAX (2 KiB) — the handler self-bounds rather than relying on the runtime clamp (runtime.rs:582–586 enforces the same ceilings; a well-formed tool stays under them) | BR9.4, `runtime.rs:41/44` |
| NFR3.8 | Log reads are tail-bounded to TOOL_OUTPUT_MAX + `truncated` marker, paged via `next_offset` — a large log arrives across bounded calls, never in one payload | BR6.4, WF3 |
| NFR3.9 | `ActionView` passes through VERBATIM — the gateway-redacted `args_summary` ≤ SUMMARY_MAX (512 B) and `result_summary` ≤ RESULT_MAX (2 KiB); U5 adds no fields and performs no un-redaction | entities ActionView, `oxpilot.rs:542–543` |
| NFR3.10 | Arg-side bounds are schema-enforced where the vocabulary reaches, handler-enforced where it can't: `path` ≤ 4 KiB normalized + `pattern` (no `..`, no leading `/`), `ref`/`sha` ≤ 256 chars, pagination `maximum` ≤ 100 rows per call, `repo.commits` limit ≤ 100; `propose_diff` title ≤ 200, `diff_body` ≤ DIFF_BODY_MAX (256 KiB), `files` `maxItems` = MAX_PROPOSAL_FILES (50), summed decoded `content_b64` ≤ 256 KiB (handler — needs the decoded byte count); `pr_comment` body ≤ COMMENT_MAX (4 KiB); exec/fs serialized args ≤ ACTION_ARGS_MAX (2 KiB); op-conditional requireds handler-side `invalid_args` | BR3.3, BR4.4, BR8.3, BR7.2 |

## Turn-level discipline

| id | requirement | basis |
|----|-------------|-------|
| NFR3.11 | Per-turn tool-call count is bounded by MAX_TOOL_ROUNDS = 8 on the U3 side — U5 keeps each call bounded so the round budget, not a single call, is the pacing knob; multi-call workflows (run → jobs → log tail) distribute across rounds | `runtime.rs:46`, U3-WF2 |
| NFR3.12 | The arg validator is a minimal closed-vocabulary check (type/required/enum/`properties`/`items`/`minItems`/`maxItems`/`minimum`/`maximum`/`pattern`/`additionalProperties`) — no regex sweeps beyond authored `pattern`s, no external validation service, no full-schema-engine cost per call | BR1.4, Q8-A |
| NFR3.13 | `catalog()` is pure: same registry, same output, O(entries) — called once per turn under `turn_parts()` and never inside the invoke path | entities, WF1 |
