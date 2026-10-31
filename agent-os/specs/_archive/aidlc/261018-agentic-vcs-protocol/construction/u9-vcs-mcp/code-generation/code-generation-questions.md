# Code Generation — Questions (u9-vcs-mcp)

## Plan Approval

Approve the code-generation plan and unit-test instructions for `u9-vcs-mcp`: the `oxidean-vcs` `client::mcp` `McpSurface` (13 `McpToolDef` descriptors — name/description/procedure/crate-local `McpAccess`/input-schema — for the FR13.1 dozen plus `vcs_changeset_get`, with separate `owner`/`name` props and C1 `required`-field parity; deliberate absences `repo_enable`/`ref_update`/`git_import`/`git_export`/`checkout`), and the `oxidean-api::mcp` host wiring (`tool_defs()` extension mapping `McpAccess`→`Access::{RepoRead,RepoWrite}`, a `build_rpc_input` `vcs_log` arm injecting `from:"main"`, existing `enforce_pat_scope` + `isError` paths reused unchanged).

[Approval Fingerprint]: sha256:v3:PENDING
[Planned Source]: PENDING

1. Approve Plan — proceed to code generation
2. Request Changes — revise the plan

[Answer]:
