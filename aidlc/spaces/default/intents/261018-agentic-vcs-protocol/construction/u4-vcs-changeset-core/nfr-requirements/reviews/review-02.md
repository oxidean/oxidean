## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Stage:** nfr-requirements
**Unit:** u4-vcs-changeset-core
**Verdict:** READY
**Iteration:** 1

### Verified (claim → where verified)

- **AR-01 fix landed — SR4.1 `old_path` uniqueness** — security-requirements.md:5 pins `old_path` unique across the manifest; disjointness from `path` explicitly not required (move-and-recreate legal).
- **AR-01 fix landed — SR4.6 `\x01r` key segregation** — rename old-path row keyed `{sha256(old_path)[0:32]}\x01{committed_at}\x01{cs}\x01r` (85 chars ≤ VARCHAR(560)); sorts after the plain row inside the `{hash32}\x01` prefix block; supersedes the FD's 83-char pin.
- **AR-01 collision analysis — fully closed.** Complete row-shape enumeration: plain×plain needs equal `path` (banned), rename×rename needs equal `old_path` (now banned), plain×rename on equal hash32 segregated by `\x01r`. Cross-kind collisions impossible — PK is `(repo_id, kind, index_key)`.
- **`\x01r` scan-shape compatibility** — `vcs_query_derived_index` (vcs.rs:2089-2168) matches TD4.7's scan shapes; `payload.path == arg` filter still applies; same-second ordering stays (ts, cs)-lexical.
- **AR-02 fix landed** — SR4.8 blame cap 65,536 touches injectable → `FrontierTooLarge`; SR4.11 adds blame O(#touches)-inside-cap + log depth-ceiling entries.
- **AR-03/AR-04 landed** — traceability NFR7 → SR4.7+TD4.8; NFR8 → +SR4.12; all six AR-0x rows present.
- **AR-05 landed** — TD4.1 reworded to `ChangesetId`/`ChangesetId::parse` oracle (`is_changeset_id` private vcs_types.rs:156; `ChangesetId::parse` :188).
- **AR-06 landed** — TD4.8 pins `crates/oxidean-db/tests/dialect_vcs.rs`, `DATABASE_URL`-gated skip-pass, CI `-E` widening optional (dialect_probe.rs:10-15 idiom confirmed).
- **No regressions** — mode set, PK math, upsert LWW, `\x01` separator safety all intact.

### Findings

| ID | Severity | Finding | Location | Recommended fix |
|----|----------|---------|----------|-----------------|
| AR-07 | nit | "no two rows in one changeset can share a key" is categorical — a 128-bit path-hash collision still merges keys (same accepted class as the id-prefix residual, but unnamed). | security-requirements.md SR4.6, SR4.11 | Extend the collision register entry to name path hashes. |
| AR-08 | nit | "the one accepted unbounded-cost residual" still miscounts — cold tree/file_at fold O(history) and skip-replay O(pages²·limit) are also uncapped-accepted. | security-requirements.md SR4.8 | Reword to "an accepted schema-unbounded residual" or name the distinguishing property. |
| AR-09 | nit | TD4.8's validation arm doesn't name the new `old_path`-uniqueness rule explicitly (implied under "manifest uniqueness"). | tech-stack-decisions.md TD4.8 | Spell out "(path unique, old_path unique across manifest, old_path≠path)". |

### Summary

All six iteration-1 findings landed correctly; no new blockers/majors. AR-01 fully closed — the two-rule fix eliminates every reachable same-changeset `path_touch` collision class (complete enumeration verified; only residue is the 128-bit hash window, AR-07). AR-02 substantially closed — blame capped-and-named, log depth ceiling registered. AR-03..AR-06 verified against `vcs_types.rs`, `dialect_probe.rs`, `ci.yml`, `Cargo.toml`. Three wording nits carry forward. **READY.**
