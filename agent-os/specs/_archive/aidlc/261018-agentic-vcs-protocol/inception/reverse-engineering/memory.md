<!-- INVARIANT: examples are single-line HTML comments so a fresh template parses to total=0 (MEMORY_EMPTY). Do NOT un-comment or split across lines. t100 guards this. -->
> This file is kept up to date automatically while the stage runs. Add observations at the review step, not by editing here directly.

## Interpretations
<!-- example: 2026-05-29T10:14:32Z — chose REST over GraphQL; the consuming team only needs CRUD, revisit if subscriptions land -->
- 2026-10-18T04:50:00Z — Human granted standing autonomy for this intent (skip human verification until PR with green CI; self-answer stage questions from the Anthropic Scion article); real prompts recorded through the human-turn hook since this harness does not fire it automatically
- 2026-10-18T04:50:30Z — Prefer Docker for debugging/testing on this project; applies to verification commands in later stages (docker compose stack, make up/smoke, act for CI lanes)

## Deviations
<!-- example: 2026-05-29T10:14:32Z — skipped the optional caching layer the stage prose suggested; the dataset is small enough that it adds risk -->

## Tradeoffs
<!-- example: 2026-05-29T10:14:32Z — picked TDD over BDD this run; the team is unit-first and the domain is well-understood -->
- 2026-10-18T04:51:00Z — Chose Full rescan over Focused scan: existing store is STALE with prior analyzed.paths ./; a focused merge would demote the rest of the repo to shallow and surface NARROWER at the gate, while this feature touches git plumbing, API, DB, CLI and MCP broadly

## Open questions
<!-- example: 2026-05-29T10:14:32Z — confirm the retention window with compliance before the next stage hardens the schema -->
