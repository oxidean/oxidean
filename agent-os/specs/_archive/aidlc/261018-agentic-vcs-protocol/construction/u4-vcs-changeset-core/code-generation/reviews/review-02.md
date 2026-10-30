## Review

**Stage:** code-generation
**Unit:** u4-vcs-changeset-core
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** NOT-READY

### Findings

1. **Major — required chained-rename test missing.** The `{rename A→B, B→C}` fold fix (materialize :840-862, file_state_at :942-955) is verified correct — per-entry removal-then-write in stored `ORDER BY path` sequence; B absent, C present; `{delete A, add A}` vacuous (dup-path `InvalidInput` + PK) — but the iteration-1 pin mandated the test and none exists (all 16 read_index tests enumerated; both rename tests use a single rename). Add a chained-rename `file_at`+`tree` test asserting B absent and C present with B→C's blob.
2. **Minor — `write_blame_shards` still un-chunked** (read_index.rs:1503-1507): one `vcs_upsert_derived_index_rows` call bypasses the chunked `upsert_rows` (:476-493, SCAN_PAGE=500 + `info!`). One-line fix: `self.upsert_rows(repo_id, &upserts).await`.

### Findings resolved since iteration 1

Canonical recipe (`{"changes":[…]}` sorted path→old_path; `{"v":1,…}` field order; parents deduped order-preserved — proven by `canonical_manifest_sorted_and_parents_order_preserved` + byte-exact golden vector incl. `idem_key == sha256_hex(canonical_input)`); error enums (`InvalidInput(String)` + `Internal(String)` all three, raw detail carried, no value echoes — 47 call sites audited); chained-rename fold code; `resolve` pub(crate); observability (`warned()` on every typed error incl. sentinels, `error = %e`, `info!` counts on create/materialize/blame, no double-warn); Crockford comment (3 bits); `chars().count()` path length; `/` prefix rejection.

### Fresh-eyes (non-blocking)

- `map_changeset_err`/`map_ref_err` warn the new IndexError though the inner already warned — acknowledged at :126, consistent with one-warn-per-return policy; log noise only.
- `merge_base` `BaseNotFound` warns though `diff` :1206 recovers it to full-delta — routine-condition warn noise.
- PG `ORDER BY path` collation can diverge from Rust byte-order on non-ASCII paths under non-C locales — affects fold-order equivalence only for exotic-path chained renames; SQLite BINARY + MySQL utf8mb4_bin are byte-order.
- `{rename A→B, add A}` ≡ `{add A, rename A→B}` canonically (sorted manifest); sequential fold → A absent, B=blob — self-consistent; same-changeset rename-source recreation survives only when target sorts before source (inherent to stored `ORDER BY path` — submission order unrepresentable).
- `create` Ok-arm `info!` precedes unreachable `ChangesetId::parse` — ordering quirk only.

### Summary

7/9 findings verified resolved against the tree; test count 69 confirmed by attr enumeration. Two residuals: one mandated test + one one-line chunking fix. No new defects introduced by the fix cycle.
