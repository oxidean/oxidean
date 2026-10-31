## Review

**Verdict:** NOT-READY
**Stage:** functional-design
**Unit:** u6-vcs-git-interop
**Iteration:** 1
**Reviewer:** aidlc-architecture-reviewer-agent

### Findings

- [BLOCKER F1] `ProtectionChecker` returns `Vec<{rule,outcome,detail}>` which cannot produce the pinned wire payload: `VcsErrorCode::forbidden(message, verdicts: ProtectionBlockReasons)` (vcs_types.rs:394) / `VcsForbiddenData.verdicts` (:444). `evaluate_push` produces a flat deny-reasons list only (protection/mod.rs:201-253); no per-rule evaluation exists. Trait must return `ProtectionBlockReasons` verbatim + C7 errata.
- [MAJOR F2] `check(..., force)` intent semantics wrong: real hook derives intent from `(old,new)` alone — `new==0`→Delete, `old==0`→Push, else `git_is_fast_forward`→Push|ForcePush (mod.rs:842-856). Drop `force` from check(); keep it in the interop ff-gate only.
- [MAJOR F3] Error bridge omits 4 `ObjectStoreError` variants: `Config|Db|Io|Internal` (object_store/mod.rs:47-69) reachable on get/put — map to `Internal` explicitly.
- [MAJOR F4] `lazy_import` pagination loop unspecified: single call converts ≤1000 commits; must loop on emitted cursor until `link_lookup(target)` hits or `done`.
- [MINOR F5] MySQL 0055 mistyped: landed column is `VARCHAR(32) NOT NULL` (mysql/0053:78) not `CHAR(29)` — that's the postgres type (postgres/0053:71).
- [MINOR F6] Cursor payload needs serialized-bytes cap: 512 tips × ~330B can exceed mysql `payload TEXT` 64KiB (mysql/0053:77) → permanent resume failure.
- [MINOR F7] ff-gate alias-twin dead-end: BR6.16 judges changeset ancestry; when old's forward-link holder is an alias twin outside head's ancestry while git-reachable, gate wrongly emits RefConflict and --force recovery is blocked under allow_force_pushes=false.
- [MINOR F8] `RefConflict.actual` type mismatch: in-crate carries git oid `Option<String>`; wire constructor takes `Option<ChangesetId>` (vcs_types.rs:368,414; CasOutcome::Conflict.actual is changeset-id, vcs.rs:122). Bridge via `old_cs`.
- [MINOR F9] `-c diff.renameLimit=10000` placed inside diff-tree argv (BR6.3) — `-c` is a top-level `git` option; must be `git -c diff.renameLimit=10000 diff-tree ...`.
- [MINOR F10] Export tree-cap enforcement point unspecified on paged `index.tree` — pin "enforce during accumulation" (u5 full_tree precedent).
- [MINOR F11] Import-side `stored!=ours ⇒ Internal` needs retry-heals property stated (transient race, next lookup early-skips).
- [MINOR F12] BR6.23 refusal inventory omits composed-author ≤255-char refusal (changeset_store.rs:457-458, just outside cited :446-455 range).
- [NIT] mermaid ImportHead bound `1..512` → `0..512`; traceability self-references BR6.x as upstream_ids; questions header mislabels review-01 finding counts; `Forbidden{[push_hook]}` detail text unpinned; heads[] silent tip-drop needs output signal; export_chain_max lacks honest-bound note; covering_changeset zero-import page semantics unpinned; lib.rs module-export line unmentioned.

### Summary

All 12 prior fixes verified present and correct in the artifacts (nullable covering_changeset, parents-first rev-list, forward ff lookup, consumed-window cursor, optional heads, probe/remint healing, real ChangesetError variants, nonce-verified release, tree_flat cap, benign same-oid aliases, backend contract). The revision introduced a new blocker: the pinned `ProtectionVerdict` struct cannot produce the `ProtectionBlockReasons` wire payload AC7.2.2 requires, and the force-intent semantics diverge from hook parity. Traversal/cursor/linkage/determinism spine is otherwise sound. Integrate F1–F12 and re-review.
