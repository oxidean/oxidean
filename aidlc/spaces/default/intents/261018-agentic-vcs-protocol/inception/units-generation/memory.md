<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across columns. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: ts — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
2026-10-18T08:00:00Z — units slice along domain-component boundaries: 10 units = wire foundation + persistence + 4 engine/component slices + API surface + 3 client surfaces; each is a Bolt-verifiable deliverable that never crosses an ownership boundary
2026-10-18T08:00:00Z — multi-unit stories get ONE primary unit (owns the behaviour) + secondary cells naming only the surface share (e.g. US2.7 → U7 primary, U10 renders, U2 signature)

## Deviations
<!-- example: ts — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
2026-10-18T08:00:00Z — GC job spawn + config/doc wiring lands in U7 (host wiring), not U3 — the job registration site is oxidean-api even though ObjectStore owns GC behaviour; documented on both sides so the split is explicit

## Tradeoffs
<!-- example: ts — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
2026-10-18T08:00:00Z — separate U5/U6 units (vs folding into changeset-core): keeps the WAL-core unit small but makes U4 the DAG bottleneck — three units (U5,U6 + transitively U7–10) all queue behind it
2026-10-18T08:00:00Z — CLI/MCP/web as three independent post-API units: maximizes parallel fan-out but none can be verified until U7 ships real endpoints (contract stubs alone don't suffice for e2e checks)

## Open questions
<!-- example: ts — confirm the retention window with compliance before the next stage hardens the schema -->
2026-10-18T08:00:00Z — contract-design owes: insert_repo_activity nullability call (gates U7's op→row mapping), vcs.status frozen schema (gates U1 client regen), file_get inline cap, diff boundedness, conflict payload fields, vcs_changeset_get MCP decision, import commits-per-call cap
