# ADR: auto-merge orchestration lives in oxidean-api-core beside merge_queue

- **Status**: accepted (WI-0003 / COL-07)
- **Date**: 2026-11-22
- **Supersedes**: none — extends the merge-queue placement precedent.

## Decision

Auto-merge-when-green is implemented as an orchestrator module inside
`oxidean-api-core` (the `auto_merge` module, sharing the merge-queue
`MqDeps`/`QueueScope`/`repo_coords` seam-deps pattern), and `pull.merge`'s
post-gate execute-and-fan-out body is extracted into a core-level
merge executor that both the `pull.merge` RPC handler and the auto-merge
fire path call with an explicit actor.

## Rationale

The decisive re-evaluation seams are not pull-domain RPCs: check-suite
completion (`oxidean-api-check`), commit-status upserts
(`oxidean-api-repo` and `oxidean-api-core::actions`), head-move
synchronization (`oxidean-api-core::pull`), and pull lifecycle
(`oxidean-api-pulls`). Domain crates sit at one layer and never import
each other, so only a core-level orchestrator is reachable from every
seam — the same reason the merge-queue orchestrator lives in core.

Two rejected shapes, recorded so they are not re-litigated:

- *Auto-merge via internal merge-queue enrollment* — the queue lands
  merge-commit chains on `gh-readonly-queue/*` refs and cannot honor
  squash/rebase; it is train-shaped, not per-PR; its event vocabulary
  (`enqueued`/`dequeued`) differs from GitHub's
  `auto_merge_enabled`/`auto_merge_disabled`.
- *Orchestrator in `oxidean-api-pulls`* — unreachable from api-check /
  api-repo / core seams without inventing an event bus (parallel
  infrastructure).

## Consequences

- One merge-execution implementation serves `pull.merge`, auto-merge,
  and (in shape) merge-queue landings; the existing `pull_merge.rs`
  suite pins the extracted path.
- Auto-merge reuses `evaluate_merge_gate` verbatim — "green" is defined
  exactly once.
- Semantics follow GitHub: repo `allow_auto_merge` toggle; enable =
  Write+ with repo-allowed method; fire when the gate passes; disable
  on head push / retarget / close / explicit disable;
  `auto_merge_enabled|disabled` activity on webhooks + Actions; REST PR
  `auto_merge` field populated (no REST verb — GitHub is GraphQL-only).
