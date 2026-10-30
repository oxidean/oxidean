<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across columns. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: ts — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
2026-10-18T09:00:00Z — specced internal seams at signature granularity only (façade method names + arg shapes), not prose contracts — Rust typechecking carries the rest and over-specifying creates doc drift
2026-10-18T09:00:00Z — resolved parked vcs_changeset_get MCP question YES (13 tools): agents need point reads for lazy-import debugging; absence list stays deliberate for admin/ref-update/import-export/checkout

## Deviations
<!-- example: ts — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->

## Tradeoffs
<!-- example: ts — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
2026-10-18T09:00:00Z — chose NULLable push columns on insert_repo_activity over a generalized signature: one migration vs a new façade shape every consumer must migrate to; ref-free ops write NULLs
2026-10-18T09:00:00Z — file_get inline cap 256 KiB balances one-round-trip convenience vs RPC as bulk channel; object route stays the bulk path

## Open questions
<!-- example: ts — confirm the retention window with compliance before the next stage hardens the schema -->

## Review convergence record (5 reviewer passes across 3 engine attempts)
- Pass 1 (NOT-READY, 23 findings): C1/C2 spec blocks rewritten wholesale against frozen pins — POST /objects restored, quota_exceeded 413 mapping, typed vcs.conflict envelopes, non-idempotent CAS, cookie scope narrowed to object routes only.
- Pass 2 (NOT-READY, 9 findings): GET 403 gloss corrected to never-enabled-only; merge→a/b and rebase→{changeset_id,new_parent_id} realigned to pinned wire shapes with supersessions recorded; author made optional (FR2.2 server-side resolution).
- Pass 3 (NOT-READY, 3 findings): blame ≥2-refs-no-main fallback closed; rebase commits_count→1; diff from/to typed.
- Pass 4 (NOT-READY, 2 findings): changeset_id type-token propagated to tree/file_get/rebase inputs; git_export before_oid stated.
- Pass 5 (NOT-READY, 3 findings): merge fast-path direction-neutral (descendant a-or-b); merge commits_count→1|0; errors: key harmonized.
- Pass 6 (READY, 0 findings): terminal sweep clean — 17/17 FR10.1 names, 8/8 YAML blocks parse, all upstream pins verified against real code (bearer.rs:735, app.rs:389).
- Engine mechanics learned: each redo-jump grants exactly 1 review pass; post-verdict edits invalidate fingerprint; the rejected→revise→fresh-request cycle inside one attempt works until budget exhaustion, then redo-jump is the only evidence-preserving path.
