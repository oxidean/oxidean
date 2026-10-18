## Review

**Stage:** nfr-design
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-16 | minor | `.gc-lock` acquisition vs SD3.5's two early gates unpinned — lock-first breaks both: `create_new` on absent dir fails ENOENT (not `Ok(zero stats)`), and a pre-gate lock makes the dir non-empty so a fresh dir refuses. Pin: absent-dir check + marker gate precede `.gc-lock` acquisition (LFS lfs_gc.rs:39-42 confirms order) | security-design.md SD3.4–SD3.5 boundary | Unresolved |
| R-17 | minor | Row-less sweep probe `!vcs_get_object(hash) → remove_file` doesn't pin Err-vs-absent — `unwrap_or(None)` deletes over a live row on transient DB error; `?` aborts the pass. Pin: probe `Err` ⇒ keep file + `errors += 1` — an unconfirmable probe never reads as rowless | security-design.md SD3.5 | Unresolved |
| R-18 | nit | `put` bound spelled `S: Stream<…>` while pins upstream use `StreamExt<…>` (TD3.3, BR3.1, store.rs:42) — equivalent but inconsistent spelling in a verbatim-pin doc | logical-components.md LC3.2 | Unresolved |
| R-19 | nit | `Config` scoped to "startup refusal" but SD3.5's marker gate returns it from `gc_once` at job time — pin the job-time surface (abort pass, logged refusal) | LC3.3 vs SD3.5 | Unresolved |
| R-20 | nit | SR3.9/BR3.7 targets say "LC3.2 config.rs" — config.rs lives under LC3.1; LC3.2 is ObjectStore methods | traceability.json | Unresolved |
| R-21 | nit | `from_mode` idiom cite mislabels cli.rs:430 — it uses `set_mode(0o700)` on metadata-derived perms; only host_keys.rs:41 uses `Permissions::from_mode` | SD3.8 | Unresolved |
| R-22 | nit | Corrections register lists `Internal` bare — write `Internal(String)` for parity with LC3.3 | SD3.10 | Unresolved |
| R-23 | nit | `repo_quota_bytes=0` admits `size=0` carve-out unregistered in SD3.10 (supersedes SR3.9's "deny-all" wording) | SD3.6 vs SD3.10 | Unresolved |

### Summary

READY per rubric (zero Critical, zero Major). All fifteen iteration-1 findings verified resolved in current bytes — `repo_id` in the pinned SQL; marker-before-tmp in both LC3.2 and SD3.8; SD3.5 gate order absent→non-empty-and-no-marker; dual-NOT-EXISTS dialect-portable with serving indexes verified across all three migrations; `has_lfs_link` idiom confirmed at crates/oxidean-db/src/lfs.rs:230-266; Makefile:237-243/405/410 and ci.yml:207 pins verified; `is_skipped`/`skipped`/`Database: Clone`/`object_links` param confirmed; `unwrap_or(0)` idiom at vcs.rs:673/lfs.rs:437. New items are one-line pins to fold into code-generation: lock-vs-gate ordering (R-16) and probe-Err handling (R-17) are semantic pins the implementation must follow; R-18–R-23 are doc-level polish.
