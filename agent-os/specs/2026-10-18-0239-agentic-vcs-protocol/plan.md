# Plan — Agentic VCS Protocol (imported record)

> Imported 2026-10-31. Source: AI-DLC intent `261018-agentic-vcs-protocol`
> ([`../_archive/aidlc/261018-agentic-vcs-protocol/`](../_archive/aidlc/261018-agentic-vcs-protocol/)).
> 14/18 stages ran; the remaining verification/operation tail stages were
> closed out by the merge itself.

## Status: shipped via #159 (merged 2026-10-19)

Side-by-side agentic source-control protocol running alongside git —
inspired by Anthropic's internal VCS redesign — implemented in a dedicated
crate consumed internally by server, CLI, and MCP.

## Bolt plan (all delivered in #159)

- **B1 wire-spine** (U1–U2): `oxidean-vcs` crate skeleton (client/server
  features), 17 `vcs.*` proc names + proc classes in `oxidean-core`,
  3-dialect migrations, `VcsPersistence` façade
- **B2 changeset-core** (U3–U4): sha-sharded CAS object store with GC grace
  window, `ChangesetStore` create + ReadIndex in one tx, `RefRegistry` CAS
  with `ref_conflict{actual}`, derived log/tree/diff/blame/path_history/
  merge_base queries
- **B3 history-algebra** (U5–U6): merge engine (clean changeset_input /
  typed conflicts / fast-path), single-changeset rebase, git `import_chain`
  + `lazy_import`, `export` only via ProtectionChecker verdicts
- **B4 api-surface** (U7): all 17 `vcs.*` RPCs on `/api/rpc` (+ws) and in
  `system.manifest`, per-repo flag gate, object GET/POST routes with
  PAT/OAT-only auth, `vcs.*` repo-activity rows
- **B5 agent-surfaces** (U8–U10): `ox vcs` 16 commands + `WorkingCopy`
  sparse checkout, 13 MCP tools, repo settings toggle, activity-row
  rendering
