# Performance Test Instructions — agentic-vcs-protocol

NFR1 is the only measurable performance target this cut; requirements.md
itself records that dedicated perf infrastructure does not exist (recorded
debt). The pass/fail evidence is the bound test, not a benchmark.

## NFR1 — incremental commit-path cost

Expectation: every commit-path query is bounded by the change manifest and
direct parent edges — no unindexed full-history or full-tree walk. The
evidence is a nextest assertion that row/object visit counts during
`changeset_create` + derived-index updates stay flat while committed-history
depth grows.

```bash
cargo nextest run -p oxidean-vcs --all-features -- changeset
```

Look for the visit-count bound tests in `changeset_store.rs` /
`read_index.rs` (the NFR1 honest-bound family: `seen` cap 65536, mark-set
cap 250_000, touch cap 65_536 — all named, injectable, `FrontierTooLarge`
on overflow rather than truncation).

## Adjacent bounds exercised as pass/fail, not benchmarks

- u5 `MergeCaps`: `ancestor_probe_max` 250_000, `side_walk_max` 25_000,
  `rename_probe_max` 10_000, `merge_paths_max` 10_000 — `TooLarge` overflow.
- u6 `InteropCaps`: `rev_list_max` 250_000, `import_tree_max` 10_000,
  `export_chain_max` 10_000, subprocess stdout `max_bytes` incremental kill.
- u3 store: `max_object_bytes` mid-stream abort; quota membership probe.

## Deferred

No load/benchmark lane exists; if one is scheduled later it owns regression
detection — this stage's matrix marks only the bound-test targets `Met`.
