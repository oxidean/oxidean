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

- 3 questions self-answered; 3 artifacts (library kind).
- Platform blind contribution: NEEDS CHANGES, 1 blocker + 7 minor/notes. Blocker verified: LC3 mislocated activity constants — they're oxidean-core wire entities (u7's recorder lives in oxidean-vcs, sees core only; oxidean-db can't import oxidean-api); VALID_PUSH_TYPES extension is u7's. Integrated: LC3 crate/path correction + u1/u2 ownership split (DTO widening + row_to_item Some-wrap + web-reader guards = u1; insert/columns = u2); manifest client-contract pin (procedures = dispatchable names, not capabilities; vcs.status is the discovery call); depends_on=[] + skeleton constraints (zero deps for machete, literal version); WIRE_VERSION second-literal sync assertion; Nullable serialize/JsonSchema gotchas; rules.md:73 depends_on drift corrected.
2026-10-18T06:52:00Z — reviewer pass READY, 12 findings integrated: entities.md ownership split aligned to LC3, u2's Some()-unwrap follow-up pinned, 17 separate single-name dispatch arms pinned (parity-scanner shape trap), u8 exit-3 pin quoted, depends_on empty-until-deps clause, RefName-is-not-a-newtype correction, no-skip_serializing_if on widened fields, mermaid dep view + LC5→LC6 edge, ~240-lines scoped to the match body, pub Nullable + schemars(with) alternative, row-activity two-touch acknowledged
