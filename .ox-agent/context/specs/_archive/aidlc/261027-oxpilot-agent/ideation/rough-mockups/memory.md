<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
- 2026-10-27T01:28:00Z — grounded wireframes in the discovered layout idiom (AppPage + PageHeader + settings nav + Tailwind tokens) rather than inventing a chat-specific design system; /settings/ai stays the external-agent surface and links INTO /oxpilot's targets manager.
- 2026-10-27T01:28:00Z — read consent depth as: explicit connect → per-action or per-session grants → no cross-session "always allow" in v1; recorded as an assumption pending the nfr-design security model.

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->
- 2026-10-27T01:28:00Z — the review pass was dispatched to a real subagent (the engine forbids the lead writing its own review); verdict READY with 4 minor findings, all applied in-artifact (env-var prefix, link direction, rail status row, missing new-session surface).

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
- 2026-10-27T01:28:00Z — chose transcript-first three-pane workspace (rail|transcript|drawer) over a tabbed or dashboard layout: matches the converged industry pattern and keeps action cards in reading order for screen readers.

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
- 2026-10-27T01:28:00Z — exact route filename convention for the session view ($session param naming) — decided at code-generation against the route manifest.
