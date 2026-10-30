# Code Summary — oxpilot-tools (U5)

The model-facing tool surface for Oxpilot: the closed 23-entry
`CATALOG`, the `OxpilotTools` executor implementing `ToolExecutor`
with the fail-closed dispatch ladder, a hand-authored closed-vocabulary
JSON-schema walker (zero new deps), fifteen read handlers routed
through `dispatch_proc`, `propose_diff`/`proposal_get` with the
`ops_json` additive chain, exec/fs dispatch through
`ConnectorGateway::dispatch_action` only, ArtifactScope-bound
`pr_comment`, the three SessionOnly explicit-confirm procedures
`oxpilot.diff_{list,get,apply}`, boot wiring beside
`install_gateway`, the regenerated typed client, and colocated +
dialect + integration coverage.

## Data layer

- **`crates/oxidean-db`** — migration `0089_oxpilot_proposals_ops.sql`
  x3 dialects: `ops_json` non-null with `[]` default/backfill on
  SQLite and Postgres; MySQL uses `MEDIUMTEXT NOT NULL` (no literal
  default) and the store layer interprets empty as `[]`.
- `src/oxpilot.rs` — `NewOxpilotProposal`/`ProposalWrite` carry
  `ops_json`; `find_oxpilot_proposals_by_session` added per dialect;
  `PROPOSAL_SELECT_{PG,MYSQL,SQLITE}` column lists kept in parity so
  id-keyed and session-keyed reads both surface `ops_json`.
- `tests/dialect_oxpilot.rs` — ops round-trip + backfill default,
  newest-first session scoping, id-keyed read + `pending→applied`
  transition, empty-list-not-error, and verbatim no-redaction storage.

## Tool executor (`crates/oxidean-api-oxpilot/src/tools/`)

- `registry.rs` — the closed `&'static [CatalogEntry]` catalog,
  exactly 23 provider-legal names in fixed order with `EntryKind`
  internal class metadata that never reaches the wire `ToolSpec`.
  Write tools are exactly `propose_diff`, `pr_comment`, `exec_shell`,
  `fs_read`, `fs_write`; control/mutation procs are absent by
  construction.
- `validate.rs` — hand-authored closed schema walker (`type`,
  `required`, `properties`, `additionalProperties`, `enum`, bounds,
  `items`, `maxItems`, named patterns). `additionalProperties:false`
  throughout; `repo_path` rejects absolute/`~`/backslash/NUL/`.git`/`..`.
- `mod.rs` — `OxpilotTools::invoke` ladder: unknown name →
  `unknown_tool`; malformed/schema-invalid args → `invalid_args`;
  missing/foreign/dead session → `not_found`; then handler dispatch.
  Session identity comes only from `ToolCtx` + the live row; per-call
  synthesized `RpcCtx` (`oxpilot:`-namespaced, `ClientMeta::default()`)
  keeps proc-level authz; output clamp + audit/`record_with` swallow.
- `read.rs` — 15 read handlers, each exactly one `dispatch_proc` with
  `{owner,name}` injected from the resolved session row.
- `propose.rs` — `propose_diff` bounds (256KiB body, 50 files,
  title cap), diff/ops path-set coherence incl. rename old+new,
  `pending`-only persistence, diff card via `emit_frame`, and
  session-pinned `proposal_get`.
- `dispatch.rs` — `exec_shell`/`fs_read`/`fs_write` resolve explicit
  `target_id` or the exactly-one-connected-target fallback (`no_target`
  / `ambiguous_target`), strip `target_id`, bound args at
  `ACTION_ARGS_MAX`, and return `{action_id,state}` on insert. Denial
  decode is structural on `data.denied`: `consent_required` emits a
  consent card + success-shaped do-not-re-issue output;
  `not_connected`/`offline`/`capability` → `action_denied`;
  missing/malformed/unknown → `internal` (fail closed).
- `comment.rs` — `pr_comment` membership check against applied
  proposals' `pr_ref` (`owner/name#number`), nonempty ≤4KiB body
  redacted before `pull.comments.create`, post-commit
  `oxpilot.artifact_comment` audit.
- `procs.rs` — `oxpilot.diff_list` (owned session, newest-first),
  `oxpilot.diff_get` (user-scoped), `oxpilot.diff_apply` (enablement +
  verified user): branch `oxpilot/proposal-<id8>` → ordered file ops
  (batched create/update uploads, delete/rename ops) → PR create →
  `mark applied` → `oxpilot.diff_applied`. Failures are
  `apply_failed:<step>` with no rollback; second apply is
  `diff_not_pending`. No push/merge/default-branch mutation.
- `lib.rs` + `dispatch.rs` — the three procedure names registered and
  matched; `crates/oxidean-api/src/main.rs` calls
  `install_tools(OxpilotTools)` beside `install_gateway`.

## Tests (test-after, standard strategy)

- Colocated `tools/*.rs` tests — catalog order/closure pin, schema
  walker accept/reject matrix, ladder ordering, denial decode matrix,
  ArtifactScope join-key shape.
- `oxidean-db::dialect_oxpilot` — 28 tests pass incl. five U5
  proposal/`ops_json` cases.
- `oxidean-api::oxpilot` — 30 tests pass incl. U5 flag-off/anonymous
  denials, session-scoped diff list/get, apply materialization +
  second-apply rejection, and fail-stop pending retention.
- `oxidean-api-oxpilot` suite — 158 tests pass;
  `cargo check -p oxidean-api-oxpilot --tests` clean;
  `make rpc-gen` + `make rpc-sync-check` in sync with zero new
  dependencies.
