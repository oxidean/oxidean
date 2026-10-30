<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
2026-10-18T06:00:00Z — rendered "mockups" as annotated terminal/RPC transcripts + one component spec; the feature's UI is developer experience (CLI, MCP, envelopes, one settings control), so ASCII flows carry the design better than visual comps
2026-10-18T06:00:00Z — anchored CLI output to `Out::Rpc {envelope, pretty}` (RPC variant; drives `error_exit_code`) — NOT `Out::Report` which is local-only and always exits 0; product-lead mob review caught this as a blocking pin error
2026-10-18T06:00:00Z — pinned `{owner, name}` as separate wire fields; a combined `"repo":"owner/name"` string doesn't exist in `repo_types.rs`/`mcp.rs` and would fail-open `fg_repo_access_check`

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
2026-10-18T06:00:00Z — exit codes inherit the existing `error_exit_code` mapping (auth tier → 3, domain → 1) rather than inventing per-code numbers in the mockups

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
2026-10-18T06:00:00Z — object routes get real HTTP statuses (404/403/413 + JSON envelope body) since they're mixed-media (bytes vs envelope); RPC keeps single-200 + typed envelope
2026-10-18T06:00:00Z — `--change` grammar extended with a mode slot (`path:kind:mode:blobspec`) so executable/symlink entries (AC2.6.1) are expressible at the CLI

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
2026-10-18T06:00:00Z — `vcs.status` field set is provisional; contract-design owns the exact shape (requirements pin only "live + empty history")
2026-10-18T06:00:00Z — parked from user-stories for contract-design: exact merge-conflict payload fields beyond `{path, kind}` (R-13), `vcs.diff` pagination/boundedness (R-14), `ref_update` arg naming (repo `name` vs ref `name` collision in one args object)
2026-10-18T06:45:00Z — resolved AC8.4.1's "`show`/`cat`" ambiguity: they are DISTINCT commands (show → vcs.changeset_get manifest, cat → vcs.file_get bytes) — 16 command names. Contract-design owns final naming confirmation; whether `vcs_changeset_get` also ships as an MCP tool is parked for contract-design
