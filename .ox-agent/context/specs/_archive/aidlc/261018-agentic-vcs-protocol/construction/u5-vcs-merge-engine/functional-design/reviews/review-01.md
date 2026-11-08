## Review

**Verdict:** NOT-READY
**Stage:** functional-design
**Unit:** u5-vcs-merge-engine
**Iteration:** 1
**Reviewer:** aidlc-architecture-reviewer-agent

### Findings

- [BLOCKER] side_manifest mark phase bounded by side_walk_max (4_096) — ancestors(base) is full repo history; virtually every real repo would TooLarge. Split: mark ≤ ancestor_probe_max (250k), side-exclusive walk ≤ side_walk_max.
- [Major] rebase rename provenance uses wrong side's map — replayed renames come from vcs_list_file_changes(cs) directly (one call, no walk), not a target-side ancestry walk.
- [Major] rename pass + per-path classify can double-emit on one path (theirs renames X→Z AND wrote Y) — needs suppression: classify skips rename-marked targets.
- [Major] side_manifest stale-chain/collision semantics unspecified — chained renames leave stale prov[old]=p → false conflicts + missed honest renames. Compose chains to tip-visible target; ambiguous duplicate olds drop (no provenance).
- [Major] manifest output unbounded — manifest_diff can emit ~20k entries > wire's 10k bound; add changes.len() check before create.
- [Major] merge3 re-serialization + trailing-newline hole: split_lines drops terminators — "a" vs "a\n" identical vectors; carry per-line terminator flag, pin join rule, trailing-nl-only divergence inside unstable region = conflict.
- [Minor] manifest_diff rename emission needs old ∈ first_tree (provenance vs first parent) + entry == (mode,blob) vs t_t[p].
- [Minor] error-bridge gaps: QuotaScope dropped (carry it); facade String errors → Internal; enumerate all variants (IndexError::{PathNotFound,CursorMalformed}, ObjectStoreError::{Config,Db,Io,Internal}).
- [Minor] detail can exceed 200 chars (targets: two ≤512-char paths) — pin deterministic truncation + collection order.
- [Nit] signature drift: vcs_list_file_changes takes no repo_id; tree() uses Option<prefix>/Option<cursor>; SCAN_PAGE private — merge_engine uses own const; put(stream, repo_id); stream::once takes a future.
- [Minor/perf] mark(base) computed identically twice — share across both side_manifest calls.

### Summary

Pass-1 integrations verified correct; the redesign itself carries fresh defects — most critically the mis-bounded mark phase (blocks every mature repo) and wrong-source rebase provenance. 11 findings, all mechanically fixable.
