<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
- 2026-10-18T05:20:00Z — read FR6.1's "(default head `main`)" as the conventional head name, not auto-provisioning — AC5.2.1 asserts a fresh repo's `ref_list` is empty and `main` is created by the first `vcs.ref_update`; recorded in the stories.md conventions block.
- 2026-10-18T05:25:00Z — review markdown files must render `## Review` + `**Verdict:**`/`### Findings` table with `R-NN` finding ids; the engine rejects non-`R-` ids and any H1/H2 heading after the verdict — two rejected `log review` calls this stage before the shape was right.

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
- 2026-10-18T04:45:00Z — landed 34 stories vs the ~20-26 granularity the Q3 answer bounded; drift is concentrated in negative-path/edge ACs the mob's quality pass surfaced, which read better as their own stories than buried ACs.
- 2026-10-18T04:45:00Z — kept US ids stable during integration rather than renumbering: US7.3's story body was folded into a G7 compatibility-invariant note + AC7.2.4 after design flagged it as a constraint wearing a story costume.
- 2026-10-18T05:20:00Z — no Could-band stories shipped though Q4 named candidates (status detail, GC tuning knobs); GC knobs were partially absorbed into AC3.5.3 and the partial status coverage lives in AC1.1.2 — the remainder is genuinely Won't/deferred.

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
- 2026-10-18T04:45:00Z — elevated `vcs.git_export` (US7.2) to Must after design flagged it as the human-trust loop's only landing path; a read-only VCS strand would fail the "humans gate consequential machine changes" posture from the article.
- 2026-10-18T04:45:00Z — pinned merge granularity to diff3-style line merge for text + file-level for binary/mode/rename after developer flagged an unspecified 3-way merge as epic-scale ambiguity; content-level merge is the common agent case (two agents, different parts of one file).
- 2026-10-18T04:45:00Z — rewrote sparse fetch as client-side manifest resolution (`vcs.changeset_get` → filter → request wanted blobs) after developer proved no server-side filtered-fetch op exists in the defined surface; keeps the server dumb per the want-based model.
- 2026-10-18T05:20:00Z — elevated US8.3 (MCP tools) to Must beyond the ratified Q4 Should-band answer, after product review flagged the unrecorded contradiction; MCP is the primary agent persona's surface. Also recorded US8.4/US8.5/US8.6 as Must — the protocol's shell and auth surfaces aren't optional in any callable MVP.

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
- 2026-10-18T04:45:00Z — non-fast-forward ref moves are CAS-guarded only (no ancestry enforcement) this cut; contract-design should confirm that's the intended minimal-coordination posture.
- 2026-10-18T05:20:00Z — NFR ids appear as AC tags ([NFR1], [NFR5, NFR8]) while their traceability rows are fully Deferred to nfr-requirements; that stage should treat those ACs as pre-pinning rather than starting from zero.
- 2026-10-18T05:55:00Z — R-13 for contract-design: AC2.4.2's `{path, kind}` conflict list needs its surface shape named — typed `vcs.conflict` error payload vs success response (and `vcs.conflict`/`vcs.unauthenticated`/`vcs.forbidden` in AC8.1.4 need named consumers).
- 2026-10-18T05:55:00Z — R-14 for contract-design: `vcs.diff` is absent from AC4.1.2's limit/cursor enumeration — confirm the intended manifest-bounded cost model or add it to the pagination contract.
