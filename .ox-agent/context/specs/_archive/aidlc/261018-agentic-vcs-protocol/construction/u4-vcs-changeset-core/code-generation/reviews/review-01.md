## Review

**Stage:** code-generation
**Unit:** u4-vcs-changeset-core
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

### Findings

1. **High — canonical identity recipe deviates from the pinned recipe.** FD-Q1/BR4.1/SD4.3 pin `manifest_json = {"changes":[…]}` with entries **stable-sorted by `path` then `old_path` (None first)** and `canonical_input = {"v":1,"repo":…,"parents":[…],"author":…,"message":…,"manifest_hash":…}`. Implemented: bare `[{…}]` array, entries in caller order (never sorted), `canonical_input` with no `"v":1` and different field order (`changeset_store.rs:302-365`). Impact: (a) byte-exactness breaks vs u6 import-path id convergence; (b) unsorted manifest entries → a logically-identical manifest with reordered entries produces a different id/idem_key — silently defeats content-idempotency. Note: the plan pin's "parents(sorted)" wording is itself wrong vs FD/SD "deduped-ordered" — parents[0] is first-parent semantics; preserve order, sort only manifest entries. Fix: emit the FD/SD recipe exactly + regenerate golden vectors.
2. **High — error-enum payload shapes follow superseded entities.md, not LC4.6.** Pinned: `InvalidInput(String)` (reason) + `Internal(String)` (raw facade/io detail for host diagnostics). Implemented: `InvalidInput{field,reason}` + unit `Internal` everywhere (changeset_store.rs:61-63, ref_registry.rs:33-35, read_index.rs:71-74). Impact: u7 wire-mapping contract deviation AND `Internal` drops all raw error detail — `map_err(|_| Internal)` discards e.g. the R18 `vcs cas_ref create:` text; tracing warns carry no `error=` field either, so a non-sentinel DB failure leaves zero diagnostic residue. Fix: `Internal(String)` carrying raw detail; `InvalidInput(String)` carrying reason.
3. **Medium — chained-rename manifests materialize phantom paths.** `{rename A→B, rename B→C}` is legal (old_path unique; disjointness from path not required). `read_index.rs:828-847` materialize does all removals then all writes → `B` re-inserted with A's blob (move-and-leave-copy); `read_index.rs:921-934` file_state_at returns `Some` for `B` instead of `None`. Fix: per-entry move semantics in manifest order, or skip writes whose path is in the manifest's removal set (delete paths ∪ rename old_paths); add a chained-rename test.
4. **Low — `resolve` visibility.** `ref_registry.rs:264` is `pub`; LC4.3 pins `pub(crate)`.
5. **Low — observability gaps vs SD4.10/SR4.12.** No `warn!` on sentinel decodes (guard at changeset_store.rs:595 excludes them); no `info!` index-write count on create (only merge_base/tree/blame upserts log); warns carry no `error = %e` (gc.rs precedent); double-warn on unmapped create error (:596-599 + :474-477).
6. **Nit — Crockford comment wrong** (changeset_store.rs:42-43): last char carries 3 significant bits, not 2 (test :820-822 documents correctly).
7. **Nit — byte-vs-char length.** `validate_path` uses `path.len()` (bytes) vs `compose_author` `chars().count()` — multibyte paths over-rejected vs pinned 1..=512 chars (VARCHAR(512) char-typed). Fail-closed but stricter than spec.
8. **Low — `write_blame_shards` un-chunked** (read_index.rs:1476-1480): one `vcs_upsert_derived_index_rows` call; `upsert_rows` chunks at 500 — huge files exceed a single batch.
9. **Nit — `tree` prefix `"/"` silently unfiltered** (read_index.rs:965-976): strips to empty filter instead of `InvalidInput`.

### Verified clean

Sentinels (4 literals whole-string, R18 raw-Err→Internal at vcs.rs:1891); `mode::bigint` fix is a REAL PG break correctly fixed (INT4→i64 decode); all CAS arms exact incl. no-pre-read; key grammar all five kinds + `\x01r` + payload.path guards; `update_for` static `pub(crate)` owned rows; all walks (heap/skip-replay, (lo,hi) mark-BFS+cache, BTreeSet diff, blame ancestry+shard, first-parent fold snap-LAST); existence check before every walk; caps injectable + all four wired to FrontierTooLarge; cursor codec (URL_SAFE_NO_PAD ≤1KB 5 shapes); all SR4.1 validation arms incl. 40960-mode and 41471-rejection test; facade consumption only (no new methods, no vcs_get_ref); server-gating + client dep-free; `&[&[u8]]` diff_lines + LINE_CAP.

### Summary

High-quality implementation matching ~13/15 pins, 68 test attrs verified. Two pinned-surface deviations (identity recipe, error shapes) + one real chained-rename correctness bug block. Fix cycle should also reconcile the four conflicting recipe texts (FD/SD win; plan/summary re-baseline) and regenerate golden vectors.
