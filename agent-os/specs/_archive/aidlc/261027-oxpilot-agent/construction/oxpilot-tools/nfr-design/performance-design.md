# Performance Design — oxpilot-tools (U5)

Consumes: `nfr-requirements/performance-requirements.md`
(NFR3.1–3.13), `functional-design/rules.md` (BR3/BR4/BR6/BR7/BR8/
BR9), `functional-spec.md` (Pins), `entities.md` (`OxpilotTools`,
`ArtifactScope`), shipped constants — `runtime.rs`:
`TOOL_CALL_TIMEOUT` 60s, `TOOL_OUTPUT_MAX` 32 KiB,
`ARGS_SUMMARY_MAX`/`OUTPUT_SUMMARY_MAX` 2 KiB, `MAX_TOOL_ROUNDS` 8;
`oxpilot.rs`: `ACTION_ARGS_MAX` 2 KiB, `SUMMARY_MAX` 512 B,
`RESULT_MAX` 2 KiB; `repo/files.rs`: `UPLOAD_MAX_FILES` 50;
`services.rs`: `TITLE_MAX` 200, `DIFF_BODY_MAX` 256 KiB,
`MAX_PROPOSAL_FILES` 50.

## Per-invoke cost budget (NFR3.1–3.6)

The ladder bounds every `invoke` to constant work plus ≤ a
handful of awaited calls — far inside U3's 60s race on every
well-formed call:

| ladder step | cost |
|---|---|
| name lookup | O(23) linear scan over `&'static` rows |
| args parse | `serde_json::from_str` — O(args bytes) |
| schema validate | O(schema keywords × args nodes), depth ≤ 16 |
| session re-resolve | ONE indexed service read (`get_session`) |
| handler | per-class dispatch count below |

| class | awaited calls per invoke |
|---|---|
| repo/issue/pull/actions read | 1 `rpc_dispatch` — never retry, never auto-pagination (NFR3.2); `actions_get_job_log` never passes `wait_ms` |
| `proposal_get` | 1 svc read + post-fetch session pin |
| `propose_diff` | 1 `svc.save_proposal` (+ best-effort card/audit) |
| `pr_comment` | 1 session-scoped proposals query (ArtifactScope) + 1 `rpc_dispatch` |
| exec/fs trio | ≤1 `list_targets` (ONLY on the no-arg fallback — NFR3.4) + 1 `dispatch_action` |
| `targets_list`/`action_status` | 1 gateway read |
| `diff_apply` (user proc) | ≤ ~55 sequential dispatches: 1 `repo.file.commitPolicy` + 1 `repo.refs` (apply-name prechecks — protected ⇒ `apply_failed:protected_branch`, exists ⇒ `apply_failed:branch_exists`) + 1 `repo.branchCreate{owner, name, branch:"oxpilot/proposal-<id8>", start:row.base}` (`start` REQUIRED — omitted ⇒ repo default branch) + ≤1 batched `repo.file.upload` (≤50 ops, `files[].content_base64`) + ≤50 per-op `repo.file.{delete,rename}` — every `repo.file.*` op carries `{owner, name, ..., message:<synthesized>, branch:"oxpilot/proposal-<id8>"}` + 1 `pull.create{owner, name, title, body, base_ref:row.base, head_ref:"oxpilot/proposal-<id8>"}` (NFR3.6) |

`dispatch_action` returns when the gateway's INSERT returns — the
tool NEVER blocks on a connector inside `invoke` (NFR3.3);
connector results arrive async and surface through
`action_status` on a later round.

## Output bounding strategy (NFR3.7–3.9)

- Every handler self-bounds BEFORE returning: `ToolOutput.content`
  ≤ `TOOL_OUTPUT_MAX` (32 KiB) with an explicit `truncated`/`…<n>
  bytes truncated` marker applied at the HANDLER — UTF-8
  char-boundary trim, the `truncate_marker` convention
  (runtime.rs:141–150); `output_summary` ≤ `OUTPUT_SUMMARY_MAX`
  (2 KiB), redacted. The runtime clamp is the second line of
  defense, not the design (runtime.rs:582–586 enforces the same
  ceilings before persisting).
- Log reads tail-bound to `TOOL_OUTPUT_MAX` + marker; the
  `{content, next_offset, done}` tuple is carried so a large log
  pages across bounded calls instead of one unbounded fetch
  (BR6.4/NFR3.8).
- `ActionView` passes through VERBATIM — `args_summary` ≤
  `SUMMARY_MAX` (512 B), `result_summary` ≤ `RESULT_MAX` (2 KiB),
  both gateway-redacted; U5 adds no fields, re-derives nothing
  (NFR3.9).

## Arg-side bounds (NFR3.10)

Schema-enforced where the closed vocabulary reaches; handler-
enforced where it can't:

| bound | enforced at |
|---|---|
| `path` ≤ 4 KiB; `pattern` shape (no `..`, no leading `/`) | schema (`maxLength` + matcher table) |
| `ref`/`sha` ≤ 256 chars; `per_page`/`limit`/`skip` ≤ 100 | schema |
| `title` ≤ 200; `diff_body` ≤ 256 KiB; `files` maxItems 50 | schema |
| summed decoded `content_b64` ≤ 256 KiB | handler — needs the decoded byte count |
| op-conditional requireds (rename⇒`new_path`; create/update⇒`content_b64`) | handler — `invalid_args` (schema can't express) |
| diff↔ops path-set coherence | handler — `invalid_args` |
| `command` ≤ 1 KiB | schema |
| exec/fs serialized args ≤ `ACTION_ARGS_MAX` (2 KiB) | handler — `serde_json::to_string(&args).len()` post-validate, pre-dispatch ⇒ `invalid_args` (BR7.2: the gateway stores the redacted string; a 4 KiB `path` passes field bounds but dies here). Bound and send the SAME string: re-serialize the validated args, measure THAT string, pass exactly that string to `dispatch_action`'s `args` param — never the raw `call.args_json` (whitespace padding would slip the check and hit the gateway's `head_bounded` truncation mid-JSON) |
| comment `body` ≤ 4 KiB | schema + handler length check |

## Validator bound (NFR3.12)

O(schema keywords × args nodes), depth-capped at 16 (authored
schemas nest ≤3), closed keyword set — `pattern` matchers are
hand-authored substring/char-class scans, not a regex engine; no
external validation service, no draft-engine overhead. Worst
case is `files[]` — 50 elements × a handful of keywords — still
linear and small.

## `catalog()` purity (NFR3.13)

`catalog()` maps 23 static rows into owned `ToolSpec`s — pure,
lock-free, O(entries); it runs once per turn inside `turn_parts`
and never inside the `invoke` path.

## Turn-level discipline (NFR3.11)

`MAX_TOOL_ROUNDS` = 8 bounds calls per turn on the U3 side; U5's
contract is per-call boundedness — the round budget, not a single
call, is the pacing knob. Multi-call workflows (list → get →
comment) distribute across rounds by design.

## Headroom statement

The dominant awaited cost is a single proc dispatch or gateway
call per invoke — the same seams synchronous UI actions ride.
`diff_apply`'s ~53 sequential dispatches is the only multi-second
path, and it is a USER-side proc bounded by `ops_json` size, not
a model call.
