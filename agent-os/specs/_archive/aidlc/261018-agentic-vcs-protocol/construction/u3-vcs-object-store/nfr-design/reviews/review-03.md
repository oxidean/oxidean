## Review

**Stage:** nfr-design
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-16 | minor | `.gc-lock` acquisition position vs the two early gates unpinned in SD text (absent-dir → marker gate → lock). Landed code is correct: gc.rs is_dir check → marker gate → acquire_lock. | SD3.4–SD3.5 boundary | Unresolved in text; code verified correct |
| R-17 | minor | Row-less probe `Err`-vs-absent unpinned — landed code keeps the file and counts `errors += 1` on probe `Err`; an unconfirmable probe never reads as rowless. | SD3.5 | Unresolved in text; code verified correct |
| R-18 | nit | `put` bound spelled `S: Stream<…>` in LC3.2 while upstream pins and landed code use `StreamExt` — equivalent, inconsistent spelling. | LC3.2 | Unresolved |
| R-19 | nit | LC3.3 scopes `Config` to startup refusal but `gc_once` returns it at job time (marker gate) — job-time surface unpinned. | LC3.3 vs SD3.5 | Unresolved |
| R-20 | nit | Traceability SR3.9/BR3.7 targets say "LC3.2 config.rs" — config.rs lives under LC3.1. | traceability.json | Unresolved |
| R-21 | nit | `from_mode` idiom cite mislabels cli.rs:430 (uses `perms.set_mode`); only ssh/host_keys.rs uses `Permissions::from_mode`. | SD3.8 | Unresolved |
| R-22 | nit | Corrections register lists `Internal` bare — the enum carries `Internal(String)`. | SD3.10 | Unresolved |
| R-23 | nit | `repo_quota_bytes=0` admits `size=0` carve-out (SD3.6 + code comment) not registered vs SR3.9's "deny-all" wording. | SD3.6 vs SD3.10 | Unresolved |
| R-24 | minor | "No `oxidean-core` dep in U3" now contradicted by the landed manifest: `oxidean-core` is a dev-dependency (required — `create_user` takes `oxidean_core::Role`). Substance preserved: server feature closure and client builds unaffected; the literal claim + machete rationale are stale. | LC3.6, SD3.10 vs Cargo.toml | Unresolved |
| R-25 | nit | SR3.9 pins `MAX_OBJECT_BYTES ≥1`; `from_env` accepts `0` — fail-closed deny-all, coherent with `repo_quota_bytes=0`, but an unregistered deviation. | SR3.9 vs config.rs | Unresolved |
| R-26 | nit | SD3.8 pins `0o700` on "`tmp/` + shard dirs"; `set_dir_private_async` chmods only the `{cd}` leaf — the intermediate `{ab}` dir inherits umask. Blobs still get `0o600`. | SD3.8 vs mod.rs | Unresolved |
| R-27 | nit | SD3.6 "cast surface, complete" claims `object_count` compares — only `storage_bytes` feeds the quota compare; imprecise in a "complete" claim. | SD3.6 | Unresolved |

### Summary

Re-verification pass on unchanged bytes: READY holds per rubric (zero
Critical, zero Major). All 15 iteration-1 findings confirmed resolved and
code-verified — `repo_id` in pinned SQL across all three dialects, absent-dir
zero-stats before marker gate, marker-before-tmp construct order, closed error
enum carrying Config/Stream/Internal, bool-gated conditional delete in both PUT
cleanup and GC orphan pass, dialect-portable dual `NOT EXISTS` DELETE with
serving indexes, `VcsGcStats{skipped,errors}`, LockGuard sync-Drop +
set_modified heartbeat, per-object tolerance with `%hash`/`%error`-only warns,
and the landed CI/Makefile server-feature lanes. Remaining items are doc-pin
gaps and nits only — the landed code implements both minors (R-16 lock
position, R-17 probe-Err) correctly. R-24's dev-dep contradiction and the
R-25/26/27 nits carry forward to a future cleanup pass; none block stage
progress.
