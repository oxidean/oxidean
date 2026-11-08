# Logical Components — oxpilot-tools (U5)

Consumes: `functional-design/entities.md` + `rules.md`,
`contract-summary.md` (C1/C6/C7), `nfr-design-questions.md`
(Q1–Q7), `nfr-requirements/tech-stack-decisions.md` (TS-1–TS-9),
shipped `oxidean-api-oxpilot/src/{lib,runtime,session,services,
connector/*}.rs` + `oxidean-api-core/src/oxpilot.rs`.

## Module map (TS-1: same crate — `emit_frame` is `pub(crate)`)

| file | contents | upstream deps it touches |
|---|---|---|
| `tools/mod.rs` | `OxpilotTools { state: Arc<AppState> }`; `impl ToolExecutor`: `catalog()` + `invoke()` (the dispatch ladder); shared helpers `resolve`/`emit_card`/`audit`/`bounded`/`dispatch_proc`; `InvokeCtx{ctx, session, entry}` | `oxidean_api_core::{oxpilot,rpc,audit}`; `crate::{runtime,services}` |
| `tools/registry.rs` | `CatalogEntry`, `EntryKind`, `ToolClass`, `static CATALOG: &[CatalogEntry]` (23 rows); per-tool `schema()` fns | `serde_json` only |
| `tools/validate.rs` | `validate(schema, value, path, depth)`; `FieldErr`; closed `pattern` matcher table | `serde_json` only |
| `tools/read.rs` | 15 handlers: `repo_tree`/`repo_file_read`/`repo_log`/`repo_commit_get`/`repo_refs`, `issue_list`/`issue_get`, `pull_list`/`pull_get`, `actions_list_runs`/`actions_get_run`/`actions_get_job`/`actions_get_job_log`/`actions_list_workflows`/`actions_job_annotations` — BR3.2's tenth literal `repo.get` is allowlisted-but-undispatched in v1 (no `repo_get` catalog entry; the session row already carries the binding so no handler needs the read, and a per-invoke precheck dispatch would break NFR3.2's exactly-one-`rpc_dispatch` pin — the BR3.2 "(session-binding precheck)" annotation reconciled upstream) | `dispatch_proc` + `EntryKind::Proc` binding |
| `tools/propose.rs` | `propose_diff`, `proposal_get` (+session pin), diff↔ops coherence check | `svc.save_proposal`/`svc.proposal` |
| `tools/dispatch.rs` | `exec_shell`/`fs_read`/`fs_write` (target resolution + `dispatch_action` + `map_denied`), `targets_list`, `action_status` | `crate::connector::gateway` |
| `tools/comment.rs` | `pr_comment` + `ArtifactScope` rebuild (`find_oxpilot_proposals_by_session`) | `state.db` + `dispatch_proc` |
| `tools/procs.rs` | USER-ONLY procs: `oxpilot.diff_list`, `oxpilot.diff_get`, `oxpilot.diff_apply` (ordered proc chain) | `require_enabled`/`require_verified` + `ctx.rpc_dispatch` + `svc.*` |

Wiring (outside `tools/`): the three proc names appended to
`PROCEDURES` + three match arms in the crate `dispatch.rs`
following the shipped oxpilot arms (U6's `oxpilot.connector.*`
precedent); one `Arc::new(OxpilotTools::new(state.clone()))`
installed onto the `pub` `OxpilotRuntimeSlot.tools` field at
shell assembly POST-`AppState::new`, beside the shipped
`install_sessions`/`install_gateway` calls (main.rs) — either a
direct `state.oxpilot.slot.write().await.tools = Some(...)` or a
new `install_tools` mirroring `install_gateway` (the slot is
built inside api-core `OxpilotRuntime::new`/`from_slot`, which
`AppState::new` calls at field-init, so a finished `Arc<AppState>`
cannot exist at slot construction; the settings rebuild carry —
`prior.tools.clone()` — already ships on every rebuild path);
the `ops_json` additive chain — NEW migration
`NNNN_oxpilot_proposals_ops.sql` ×3 dialects (applied migrations
are append-only) + `ProposalWrite.ops_json`
(core/oxpilot.rs:337-342) + `NewOxpilotProposal.ops_json`
(db/oxpilot.rs:1784-1793) + `OxpilotProposalRow.ops_json`
(:776-788) + the `insert_oxpilot_proposal` column/bind list
(:1796-1850) + the `PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` column
lists (:967-980) + `map_proposal!` (:874) + the `save_proposal`
signature/impl pass-through (services.rs:625-647) — the full
chain upstream enumerates at entities.md:93-97, since
`diff_body` alone cannot represent deletes/renames/exact
content — plus a `find_oxpilot_proposals_by_session` db read.

## Component wiring

```text
provider ─ToolCall─> invoke_call (U3, runtime.rs)
                        │ mints ToolCtx from the owned session row
                        ▼
                OxpilotTools::invoke ── CATALOG lookup
                        │ schema gate (validate.rs)
                        │ svc.get_session re-resolve
                        ▼
        ┌───────────────┼──────────────────┬────────────────┐
        ▼               ▼                  ▼                ▼
   read handlers   propose.rs        dispatch.rs       comment.rs
   (EntryKind::    (Proposal entries) (Connector +     (ArtifactScope
    Proc literals)                     GatewayRead)     + Proc literal)
        │               │                  │                │
        ▼               ▼                  ▼                ▼
   dispatch_proc    svc.save_proposal  gateway.         dispatch_proc
   -> RpcCtx synth  svc.proposal       dispatch_action   -> pull.comments.
   -> rpc_dispatch  svc.mark_applied   list_targets         create
   (10+6+1 allowlisted;                action_status
    repo.get undispatched)
                                              │
   user procs (procs.rs, on PROCEDURES):
   diff_list -> svc.get_session (ownership)
            -> state.db.find_oxpilot_proposals_by_session
   diff_get  -> svc.proposal
   diff_apply -> svc.proposal -> pending check
                 -> "oxpilot/proposal-<id8>" verified
                    UNPROTECTED via repo.file.commitPolicy
                    (direct_commit_allowed required, else
                    apply_failed:protected_branch) AND ABSENT
                    via repo.refs (else
                    apply_failed:branch_exists — the
                    RepoFileCommitOptions protected-branch
                    fallback is excluded)
                 -> CALLER's ctx.rpc_dispatch
                 (branchCreate{owner,name,
                    branch:"oxpilot/proposal-<id8>",
                    start:row.base} — start REQUIRED
                  -> repo.file.*{owner,name,...,
                    message:<synthesized>,
                    branch:"oxpilot/proposal-<id8>"}
                    (upload files[]: content_base64)
                  -> pull.create{owner,name,title,body,
                    base_ref:row.base,
                    head_ref:"oxpilot/proposal-<id8>"})
                 -> svc.mark_applied
        │
        ▼ (post-commit, best-effort)
   emit_card (turns().get -> emit_frame)  |  audit -> record_with
```

## Dependency direction

`tools/*` depends DOWNWARD only:

- `mod.rs` → `registry.rs` (static data), `validate.rs` (pure fn),
  the four handler files (their named fns fill the table rows).
- handlers → the shared helpers on `OxpilotTools` (single
  direction, no handler→handler calls), `crate::services` (C5),
  `crate::connector::gateway` (C7), `crate::runtime` (frame
  sink), `oxidean_api_core::{rpc,audit}`, `oxidean_db` queries,
  `oxidean_core::redact`.
- `procs.rs` → the same service/db/dispatch seams — NEVER the
  tool surface (`diff_apply` is callable by a user, not by the
  model; routing it through `invoke` would re-expose the closed
  registry).
- `registry.rs`/`validate.rs` depend on NOTHING U5 — pure data +
  a pure walker; pin-testable without an AppState.

## Reused seams (zero new deps — TS-9)

| seam | owner | how U5 uses it |
|---|---|---|
| `ToolExecutor`/`ToolCtx`/`ToolOutput`/`ToolError` | api-core `oxpilot.rs` (C6) | implemented verbatim — no shape change |
| `state.rpc_dispatch` + `build_rpc_ctx_with_session` | api-core `rpc.rs`/`app.rs` | per-call `RpcCtx` synth in `dispatch_proc` |
| proc allowlists | repo/pulls/issues/actions `dispatch.rs` | `EntryKind::Proc` literal bindings — never arg-derived |
| `ConnectorGateway` | `crate::connector` (C7) | `dispatch_action`/`list_targets`/`action_status`; `denied` marker decode |
| `SessionService` | `crate::services` (C5) | `get_session`/`save_proposal`/`proposal`/`mark_applied` — no `proposals`/`list_proposals` verb exists; `diff_list` rides `get_session` (ownership) + `state.db.find_oxpilot_proposals_by_session` |
| `require_enabled`/`require_verified` | free fns — `oxidean_api_core::oxpilot::require_enabled` (oxpilot.rs:1324) / `auth::gate::require_verified` (gate.rs:23) | proc-side gates on the `diff_*` procs' CALLER ctx — not `SessionService` members |
| `emit_frame` | `crate::runtime` (`pub(crate)`) | `emit_card` — the TS-1 same-crate pin |
| `audit::record_with` | api-core `audit.rs` | post-commit writes, swallowed failure |
| `oxidean_core::redact` | core | every egress boundary |
| `serde_json` | already in tree | args parse, schemas, details |
| new deps | — | NONE — the pattern matcher table is hand-authored fns; no `jsonschema`, no `regex` |
