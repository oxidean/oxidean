# Contribution — aidlc-developer-agent (functional-design, u5-vcs-merge-engine)

Pre-review finding pass on the FD artifacts against actual code. Verbatim findings (14):

1. [BLOCKER] rename-provenance probe unbuildable as designed: path_touch keys are hashed-path-major — no query enumerates a side's rename rows; and WF2's ordering resolves every path before the rename check fires (silent rename/rename corruption). Working mechanism: mirror collect_side_paths — mark base∪ancestors(base) via vcs_changeset_parents, BFS from tip skipping marked, vcs_list_file_changes per node → per-side old_path→new_path maps; detection = separate cross-path pass; bonus — maps enable real `rename` manifest entries for cleanly-taken theirs renames.
2. [Major] BR5.20's ∅-base merge unreachable (disjoint → BaseNotFound → NoCommonAncestor) — strike merge half.
3. [Major] text_diff exposes no line_index/matches helpers — merge3 builds on diff_lines hunks + read_index::split_lines (pub(crate)); LINE_CAP=200_000 degeneration must pin TooLarge, not phantom conflict.
4. [Major] "shared NUL/8KiB text rule" does not exist — text_diff is binary-safe; the predicate is u5-new; record mergeable≠diffable divergence.
5. [Major] QuotaExceeded→TooLarge fold contradicts vcs.quota_exceeded mapping; plus full error-bridge layer (IndexError/ChangesetError/ObjectStoreError) was missing.
6. [Major] merge_base dominated pick undermines ancestor guarantees — engine needs own bounded is_ancestor BFS for fast-path + rebase guard.
7. [Major] tree(a)→merged→Vec<FileChangeInput> mapper unspecified — spec'd: add/delete(mode required)/modify + provable rename; ReadIndex::diff can't serve (stored-changeset input, paged, no rename kind).
8. [Minor] tree() paged at SCAN_PAGE=500 — needs cursor loop; header claim "writes ONLY create+put" missed lazy index writes inside reads.
9. [Minor] merge_paths_max=65_536 exceeds wire's 10_000-change manifest bound — re-pinned 10_000.
10. [Minor] rebase edges: no-op on ≥2-parent node pinned; disjoint target legality pinned LEGAL; tree(∅) is synthesized BTreeMap not tree() call.
11. [Minor] mode-vs-content divergence over-conservative vs git — kept deliberately (recorded decision).
12. [Minor] BR5.5 vs BR5.21 ordering — size check is separate pre-step before text test.
13. [Nit] VcsConflictEntry lacks PartialEq → MergeError doesn't derive it (recorded deviation; tests field-compare).
14. [Nit] facade name vcs_changeset_parents; LEGAL_MODES includes 16384(tree)→type; existence via row probe not full get; RebaseOutcome no-flag noted.

Verified-correct claims (unchanged): IndexError::BaseNotFound variant exists; create() idempotent return; compose_author single validation; conflict() truncation; put/get signatures; wire DTOs authorless; materialize first-parent-only.

Integration: all 14 findings folded into the four artifacts (Q2/Q3/Q4/Q5/Q6/Q7/Q8/Q9 rewrites; BR5.5/5.7/5.14/5.17/5.20-5.26 rewrites+additions; entities caps/types/consumed table; spec WF0-WF7).
