# Code Summary — u9-vcs-mcp

## What landed

`oxidean-vcs` `client::mcp` (`crates/oxidean-vcs/src/client/mcp.rs`) — the
`McpSurface`: 13 `McpToolDef` descriptors (the FR13.1 dozen plus
`vcs_changeset_get`, per contract C3's resolved parked question). Each
descriptor carries `name`/`description`/`procedure`/crate-local
`McpAccess`/`input_schema`; the crate never names `oxidean-api`'s private
`ToolDef`/`Access` (AC8.3.3 structural seam — host→crate direction kept).

Schemas mirror the C1 wire DTOs: separate `owner`/`name` required on every
tool (the fields `fg_repo_access_check` extracts), and `required`-field
parity with each procedure's request struct (`diff.from/to`,
`tree.changeset_id`, `file_get.changeset_id+path`, `merge_base.a/b`,
`rebase.changeset_id+new_parent_id`, `changeset_get.id`,
`changeset_create.parents/message/changes`, `blame.path`,
`path_history.path`). `vcs_log`'s `from` is schema-optional — `vcs.log`
requires it on the wire, so `build_rpc_input` injects `"main"` (same
convention as `repo_tree`'s `""` and the CLI's own default).

Access classes mirror C1 `proc_classes`: the ten read-tier tools map to
`RepoRead`; `vcs_changeset_create`/`vcs_merge`/`vcs_rebase` map to
`RepoWrite`. `enforce_pat_scope`, `OXIDEAN_MCP_ENABLED`/`admin.mcp.*`
overrides, and `tool_error`/`isError` domain-denial mapping are generic
over `ToolDef` and needed no changes.

Deliberate absences per C3: `vcs_repo_enable`, `vcs_ref_update`,
`vcs_git_import`, `vcs_git_export`, `checkout` — asserted by tests on both
sides of the seam.

## Files

- `crates/oxidean-vcs/src/client/mcp.rs` — descriptors + 5 crate tests.
- `crates/oxidean-vcs/src/client/mod.rs` — module export.
- `crates/oxidean-api/src/mcp.rs` — `vcs_tool_defs()` mapping,
  `tool_defs()` chain, `vcs_log` `from` injection arm, 4 host tests.

## Verification

- `cargo nextest run -p oxidean-vcs mcp::` — 5/5.
- `cargo nextest run -p oxidean-api mcp` — 47/47 (incl. 4 new vcs tests +
  full pre-existing MCP suite: auth, FG scope denial, isError envelopes).
- `cargo clippy -p oxidean-vcs --all-features` / `-p oxidean-api` — clean.
