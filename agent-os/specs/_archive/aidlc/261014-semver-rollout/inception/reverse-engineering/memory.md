<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
- 2026-10-14T02:20:00Z — full-scan first build of the codekb store: developer link found 10 versionable components, ~10 CARGO_PKG_VERSION embed sites across oxidean-cli/oxidean-api, three separate wire-contract constants (RPC_PROTOCOL_VERSION=1, MIN_CLI_VERSION, OpenAPI info.version), and real gaps: ox-cli.yml path filters omit crates/oxidean-core despite the cli→core dep; OXIDEAN_RPC_VERSION env var is documented but read by no code.
- 2026-10-14T02:20:00Z — learnings ritual deferred under the user's standing "keep going to an open PR" instruction; diary kept current, candidates triaged at PR review instead of a mid-run question.
