## Review

**Stage:** code-generation
**Unit:** u9-vcs-mcp
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Scope

`crates/oxidean-vcs/src/client/mcp.rs` (`McpSurface` — 13 `McpToolDef`
descriptors + crate-local `McpAccess`), `client/mod.rs` export, and the
`oxidean-api/src/mcp.rs` host wiring (`vcs_tool_defs()` mapping,
`tool_defs()` chain, `vcs_log` `from` injection arm).

### Verified clean

- Tool set matches contract C3 exactly: the FR13.1 dozen plus
  `vcs_changeset_get`; deliberate absences (`vcs_repo_enable`,
  `vcs_ref_update`, `vcs_git_import`, `vcs_git_export`, `checkout`)
  asserted absent on both sides of the seam.
- Every schema requires separate `owner`/`name` — the fields
  `fg_repo_access_check` extracts (mockups.md blocking note honored).
- Required-field parity with the C1 wire DTOs verified per tool;
  `vcs_log.from` is schema-optional with host-side `"main"` injection,
  matching the `repo_tree` `""` convention and the CLI default.
- Access split mirrors `proc_classes`: write tier is exactly
  {changeset_create, merge, rebase}; all others RepoRead.
- Dependency direction: descriptors carry crate-local `McpAccess`; the
  host maps onto its private `Access` — no `oxidean-api` types cross the
  seam (AC8.3.3). `client` feature placement keeps descriptors dep-free.
- `enforce_pat_scope`, `OXIDEAN_MCP_ENABLED`/`admin.mcp.*` overrides, and
  `tool_error`/`isError` mapping reused unchanged — the 47-test MCP suite
  (auth, FG denial, envelopes) passes untouched.

### NITs (non-blocking)

- The `vcs_changeset_create` `changes` description lists an informal
  `{path, kind, blob, mode, size, content_b64?}` shape — `ManifestEntry`
  has no `size` and renames use `old_path`. Prose-only inaccuracy
  (schemas are advisory to MCP clients); corrected inline.

### Summary

9 tests added (5 crate-side descriptor invariants, 4 host-side wiring
asserts) — all green; `oxidean-api` mcp suite 47/47. READY.
