# Support Contribution — aidlc-aws-platform-agent
**Stage:** nfr-design | **Unit:** u3-vcs-object-store | **Verdict:** NEEDS CHANGES (2 majors; all integrated)

1. **[MAJOR] Marker gate ordered wrong — dead code, voids SR3.10.** LC3.2 had `new()` write the marker unconditionally before the foreign-dir check could ever fire. **Fixed: `new()` order is resolve → dir-is-file ⇒ Config → non-empty (exempt marker/`lost+found`) && no-marker ⇒ Config → create + write marker (SD3.8).**
2. **[MAJOR] "Config error" promised in six places but the closed enum had no `Config` variant.** **Fixed: `Config(String)` added (LC3.3) — construct/boot refusal, never wire-mapped; `From<VarError> ⇒ Config`.**
3. **[MINOR] `SELECT EXISTS(1 FROM …)` invalid SQL.** **Fixed: probe uses `has_lfs_link` shape — `SELECT 1 AS ok … fetch_optional().is_some()` (LC3.4).**
4. **[MINOR] Conditional DELETE bool polluted by `vcs_file_changes` RESTRICT FK** — manifest-referenced unlinked objects would `Err` every pass forever, not self-heal. **Fixed: `AND NOT EXISTS(SELECT 1 FROM vcs_file_changes WHERE blob_hash=?)` added; false=protected, Err=genuine (LC3.4).**
5. **[MINOR] Marker-gate edge cases unpinned** — `lost+found` false-positive; pre-marker upgrade path. **Fixed: `lost+found` exempted (SD3.8); `touch` recovery documented (SD3.9-R9).**
6. **[MINOR] Empty-foreign-dir residual unregistered** — empty LFS/packages dir gets marked then swept. **Fixed: SD3.9-R7 + `from_env` sibling-dir equality refusal vs `OXIDEAN_LFS_DIR`/`OXIDEAN_PACKAGES_DIR`.**
7. **[MINOR] Makefile delta under-specified** — "lint" target doesn't exist (clippy lives in `dead-code-check`); `cargo test` fallback arm uncovered; doc tests run nowhere; `cargo doc` default-features; `nextest archive --workspace` needs a `-p` counterpart. **Fixed: LC3.6 rewritten — dead-code-check/docs-check named, fallback arm covered, no-doc-examples pinned, archive + feature-unification notes carried to infrastructure-design.**
8. **[MINOR] Cross-artifact drift left uncorrected** — WF1.4 prefixed hash, WF1.3 file-stays, WF4.4 stats fields, entities Stream/Internal. **Fixed: SD3.10 corrections register marks them superseded (upstream artifacts fingerprint-bound).**
9. **[MINOR] Quota arithmetic quiet edges** — `try_get().unwrap_or(0)` silent-zero; `size:u64→i64` bind site unlisted; `quota=0` admits empty objects. **Fixed: complete cast surface + inherited-idiom caveat + pinned intent in SD3.6.**
10. **[MINOR] `From` impls mislabel** — `io::Error` must branch NotFound; `From<String>⇒Db` collides with `Stream(String)`. **Fixed: SD3.7 + LC3.3 carve-outs.**
11. **[MINOR] Lock lifecycle residuals** — steal-then-drop transient double-GC; `clear_stale_lock` single-node-only; design strictly better than LFS (panic-safe release). **Fixed: SD3.4 + SD3.9-R8.**
12. **[MINOR] `grace_secs`/`interval_secs` upper bounds unpinned** — `u64::MAX` parses then breaks `chrono::Duration`. **Fixed: SD3.9-R10 — both capped `[0 or 1, i64::MAX]` in `from_env`.**
13. **[MINOR] `oxidean-core` dep contradictory** — declared but unused → cargo-machete. **Fixed: dropped from U3 edges (LC3.6) — added when a real use lands.**

## Verifications (all source-checked)
- Consumed facade surface verbatim-accurate (vcs.rs:2269-2469); the two additions genuinely absent and fit `require_pool`+`rows_affected` bool idiom (`delete_ref→bool` :1869 precedent).
- Dual-NOT-EXISTS DELETE dialect-portable; `idx_vcs_object_links_hash`+`idx_vcs_file_changes_blob` serve subqueries.
- `is_skipped`/`require_pool`/`Database: Clone` confirmed (lib.rs:131,156-160,116). `VcsObjectRow{hash,size:i64,created_at}` (:40-44) — `u64::try_from` genuinely required.
- LFS precedents confirmed incl. `validate_oid`/`shard_path`/`put_stream`/`create_new`; **LFS sets no permissions** and lacks fsync — "shared v1 hole" accurate.
- Lock soundness under tokio: Drop uses sync `std::fs::remove_file`; SIGKILL→age-stale covers; `File::create` heartbeat would truncate — corrected to `set_modified`.
- cfg-gate mechanics: resolver="2", first `dep:`-gated feature in workspace; `#[cfg(all(test,feature="server"))]` compiles under the pinned invocation. Feature-unification nuance recorded.
- Dep pins verified (sha2 0.11.0, chrono 0.4.45, futures-util 0.3, tracing 0.1, tempfile; `bytes` new direct dep; per-crate pins required).
- Wire codes `vcs.quota_exceeded`/`vcs.not_found`/`vcs.invalid_input` exist (vcs_types.rs:310-352); `db.not_configured` api-layer.
- PUT ordering sound: tmp-under-tmp + pid/counter/nanos closes the shared-tmp race LFS's `with_extension("tmp")` would reintroduce; row-first delete beats LFS's file-first.
