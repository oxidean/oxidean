<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
- 2026-10-18T04:20:00Z — read the original request plus the Scion article as authoritative product direction; all nine interview questions were answered by evidence-backed recommendation under the user's standing autonomy instruction, ratifiable at PR review.
- 2026-10-18T04:20:00Z — interpreted "side-by-side" as additive and per-repo opt-in: enabling VCS on a repo never changes Git routes, PRs, or the web surface on that repo.

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
- 2026-10-18T04:20:00Z — kept the whole VCS surface under `/api/` (`/api/vcs/…`, `vcs.*` RPC) instead of a top-level `/{owner}/{repo}.vcs` namespace; the `/api` prefix already exists in all three edge layers (Traefik, Caddy, `oxidean-web` proxy table), so the choice buys zero edge-parity work at the cost of slightly longer URLs.
- 2026-10-18T04:20:00Z — made checkout a client-side disk materialization over want-based fetch rather than a server endpoint; MCP/API agents who cannot run the CLI use `vcs.tree`/`vcs.file_get`/`vcs.diff` reads directly, matching the article's API-first model.
- 2026-10-18T04:20:00Z — chose `oxidean-db` façade + per-dialect migrations over a crate-local store: preserves Postgres/MySQL/SQLite parity and existing lifecycle seams at the cost of three-way migration authoring.

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
- 2026-10-18T04:20:00Z — eager vs lazy derived-data policy per type is deliberately parked to domain-design with cost evidence; same for import rename-detection heuristics, object size ceilings, filter-glob breadth, and GC cadence (OQ1–OQ5 in the artifact).
