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

## u1-vcs-wire-foundation

- Plan: 6 steps, standard test strategy. Implemented via subagent; all gates green (nextest 69+233, vitest 595, clippy/fmt/doc clean, version-check, web-lint/format, dead-code; rpc-sync-check pending generated-file commit as designed).
- Key catch in impl: plain `Option::deserialize` delegation silently maps absent→Null — `Nullable<T>` must buffer through `serde_json::Value` to get the absent/null/value trichotomy.
- rpc-sync-check fails only on HEAD-diff of uncommitted generated client — goes green on commit.
2026-10-18T07:30:00Z — reviewer READY, 2 fixes applied post-verdict: reverse closed-set assertion in vcs_procedures_registered_and_classified (unlisted vcs.* in PROCEDURES/arms now fails), VcsForbiddenData+ProtectionBlockReasons+VCS_ACTIVITY_KINDS added to rpc_gen template + regenerated index.ts; nextest 861/861 green
