## Review

**Stage:** code-generation
**Unit:** u4-vcs-changeset-core
**Reviewer:** aidlc-architecture-reviewer-agent
**Iteration:** 1
**Verdict:** READY

### Findings

1. **Nit — `map_changeset_err`/`map_ref_err` warn on remap** though the inner error already warned at origin (read_index.rs:121-139; acknowledged in-comment at :126). Consistent with one-warn-per-typed-error policy; log noise only.
2. **Nit — `merge_base` `BaseNotFound` warns** on the routine `diff` recovery path (diff falls back to full-delta at :1206). Non-blocking.
3. **Nit — PG `ORDER BY path` collation** on `vcs_file_changes.path` (TEXT under DB-default collation) can diverge from Rust byte-order on non-ASCII paths under non-C locales; affects fold-order equivalence only for exotic-path chained renames. SQLite BINARY + MySQL utf8mb4_bin are byte-order correct.
4. **Nit — same-changeset rename-source recreation** (`{rename A→B, add A}` where A sorts after B): fold is path-order sequential — submission order unrepresentable, self-consistent, documented.

### Resolved this pass

- **Chained-rename test** `chained_rename_in_one_changeset_no_phantoms` (read_index.rs:2371-2432) — single-changeset `{a→b, b→c}`; asserts `b.txt`→PathNotFound, `a.txt`→PathNotFound, `c.txt`→b2, tree==["c.txt"]. Passes (test 42/70).
- **`write_blame_shards` chunked** — :1504 routes through `self.upsert_rows` (chunks at SCAN_PAGE=500, IndexError-typed `?`); shard-count `info!` retained.

### Gate results

- `cargo nextest run -p oxidean-vcs --features server` → **70/70 pass**
- `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings` → clean

### Summary

All iteration-2 residuals verified resolved in-tree; 70/70 crate tests; clippy clean. Four non-blocking nits carried forward. Implementation satisfies the pinned design contract: canonical recipe (sorted `{"changes":[…]}`, `{"v":1,…}` input, order-preserved deduped parents), exact error shapes, CAS-only refs, sequential move-fold, chunked derived-index writes.
