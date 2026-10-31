## Review

**Stage:** nfr-design
**Unit:** u6-vcs-git-interop
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| F1 | MAJOR | `ident-cs:` sidecar miss ⇒ `Internal` contradicts WF5.3's deterministic fallback — `ident-cs:` rows are written only by `import_one`, so every native (never-imported) changeset lacks one and `export_one` fails `Internal` on the primary export path | logical-components.md LC6.4:58,62 vs functional-spec.md WF5:161-163 | Open |
| F2 | MAJOR | Bounded-reader overflow types `GitError::Process` → `Internal` where SD6.6/SR6.1 require `TooLarge` — `GitError` has no overflow variant and no translation mechanism is pinned | security-design.md SD6.4:53 vs SD6.6:97-98 | Open |
| F3 | MINOR | Cursor byte cap off-by-~4/3: inbound bounds the opaque b64url string, upstream bounds "serialized `CursorState` JSON" — a JSON-side cap mints ~43,690-char strings that fail resume | security-design.md SD6.2:26/SD6.6:95 + LC6.4:56 vs entities.md:10 | Open |
| F4 | MINOR | `export_one` base-tree acquisition unpinned: unchanged entries' git blob oids must come from `tree_flat(repo, parent_git_oid)` (+bound + parents[0] rule) or resolve via seen-map/`objects.get` — codegen cannot tell which | functional-spec.md WF5:151-160 + LC6.4:62 | Open |
| F5 | MINOR | `IndexError` bridge false rationale: index cursors are interop-minted paging tokens, never caller input — `CursorMalformed`⇒`Internal`, `FrontierTooLarge`⇒`TooLarge` per u5 (merge_engine.rs:126-128) | security-design.md SD6.7:119-121 | Open |
| F6 | MINOR | LC6.2 says `*_max < usize::MAX`, SD6.1 says `≤ usize::MAX/2048` — weaker form permits wrap of ×1024/×2048/×41 derived bounds | logical-components.md LC6.2:32 vs security-design.md SD6.1:20 | Open |
| F7 | NIT | "all before ANY git argv or indexing" untrue — tip resolution/`rev_list` run before decode; reword to "before any cursor-derived value reaches argv" | security-design.md SD6.2:24 | Open |
| F8 | NIT | `release(repo_id, nonce, ok)` — `ok` effect unpinned | logical-components.md LC6.4:54 | Open |
| F9 | NIT | nonce entropy "process-random" names no source — pin `std::collections::hash_map::RandomState` | logical-components.md LC6.4:53 | Open |
| F10 | NIT | `Internal`→`internal` vs Q7's `rpc.internal` literal (app.rs:838 namespaced convention) | logical-components.md LC6.6:75 vs questions Q7:123 | Open |
| F11 | NIT | `vcs_list_file_changes` listed consumed but no interop path calls it — file deltas fold `index.tree` per entities.md:51 | logical-components.md LC6.5:70 | Open |
| F12 | NIT | `export` param order `(…, actor_capability, force)` vs WF4's `(…, force, actor_capability)` — codegen-facing drift | logical-components.md LC6.2:35 vs functional-spec.md WF4:13 | Open |
| F13 | NIT | Export pipeline tail "→ outcome row" implies interop writes a persisted row; activity row is u7's write — fields live on `ExportOutcome` | security-design.md SD6.3:46 | Open |

### Summary

Support-pass integration verified faithful — all nine prior items (facade names/signatures at vcs.rs:2511/2520/2540/2550, `/2048` caps bound, six-variant `GitError` bridge, `list_refs` named residual, `CAT_COMMIT_MAX`, encode-side field bounds, 40-only `is_git_oid` rationale, Q7 cite, `ident_sidecar_read` naming) check out against real code. Every spot-checked source claim is accurate: `run_git*` readers, `classify_push_error` :1735, move methods :3267/:3423, `is_ancestor` backend.rs:735, `ProtectionBlockReasons` :265-279, `vcs.import_state` :331/354/1101. Two MAJORs block readiness: the `ident-cs`-miss⇒`Internal` pin contradicts WF5.3's deterministic fallback and would break export of every native changeset, and the bounded-stdout kill types `Process`→`Internal` where the bound table and SR6.1 require `TooLarge` with no bridging mechanism. Four MINORs and seven NITs are cheap drift/pin corrections recorded above. The contested `diff-tree -z` rename order resolves in the FD's favor — `status\0src\0dst\0` confirmed.
