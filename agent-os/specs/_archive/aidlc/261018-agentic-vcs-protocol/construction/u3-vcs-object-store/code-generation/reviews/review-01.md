## Review

**Stage:** code-generation
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| U3-01 | Minor | `OXIDEAN_VCS_MAX_OBJECT_BYTES` floor is 0 vs SR3.9's `[1, i64::MAX]` — accepts 0 (all non-empty PUTs then denied). Carried to a fast-follow. | config.rs:55-60 | Open |
| U3-02 | Minor | `paths::shard_path(&dir, &orphan.hash)?` inside the orphan loop propagates InvalidHash — a malformed vcs_objects row aborts the pass vs BR3.6's per-object tolerance. Unreachable via put(); reachable via direct facade. Two-line match fix carried. | gc.rs:59 | Open |
| U3-03 | Minor | 0o700 applied to tmp/ + leaf shard dir only; {ab} fanout + objects_dir keep umask; recreated tmp/ after external deletion keeps umask. Content protected (blobs 0600); fanout-name enumeration only. | mod.rs:186-188,244-253 | Open |
| U3-04 | Minor | rename→set_blob_private gap: blob sits at umask perms until chmod; on chmod failure a row-less permissively-moded file survives to grace. Chmod tmp before rename closes it. | mod.rs:257-262 | Open |
| U3-05 | Observation | gc_once treats dir-is-regular-file as absent (zero stats) vs new()'s Config — asymmetric; unreachable post-new(). | gc.rs:27-29 | Open |
| U3-06 | Observation | sibling-dir equality uses absolutize only — `..`/symlink forms evade collision refusal. | config.rs:79-89 | Open |
| U3-07 | Observation | future-mtime .gc-lock never stale (elapsed() errs -> Busy) — clock-skew wedge bounded by clear_stale_lock. | gc.rs:312-317,397-402 | Open |
| U3-08 | Observation | mysql SUM(size)->DECIMAL try_get::<i64> reads as 0 — accepted house idiom (lfs.rs:437), registered in SD3.6. | vcs.rs:662-675 | Open |
| U3-09 | Observation | put on a store never built via new() creates tmp/ without marker -> later gc_once refuses (in-crate-only path). | mod.rs:205-209 | Open |
| U3-10 | Observation | lock-skipped pass returns before summary info! — Busy emits no log line (skipped flag is the sole signal). | gc.rs:44-47 | Open |

### Summary

All 10 pinned correctness invariants verified PASS: new() marker-gate ordering, gc_once gate ordering, PUT pipeline incl. probe-gated quota + eager cleanup, closed error surface + From carve-outs, GC conditional-delete gating + NotFound tolerance + hash-only logs, lock lifecycle (create_new/stale-retry/heartbeat non-truncate/Drop-release), shape-filtered canonicalized sweep, env ranges, CI flag+vcs-server leg+needs+warning parity, per-dialect facade SQL binds. Zero doc examples; oxidean-core dev-dep only; no wire/RPC code; real sqlite+tempfile tests. 0 Critical, 0 Major, 4 Minor + 6 Observations carried to a fast-follow cleanup -> READY.
