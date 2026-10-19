## Review

**Reviewer:** aidlc-architecture-reviewer-agent
**Stage:** nfr-design
**Unit:** u4-vcs-changeset-core
**Verdict:** READY
**Iteration:** 1

### Verified (claim → where verified)

- **Sentinel decode** — exactly four literals, whole-string match (vcs.rs:20-23); non-match → `Internal` is the only safe decode.
- **CAS arm table** — complete and correct vs all three dialect arms (:1640-1950); R18's MySQL create-race `Err`→`Internal` divergence verified at :1874-1890.
- **`(String,String)` heap keys** — `ChangesetId` has no `Ord` (vcs_types.rs:183); normalized `…Z` timestamps read back identically on all dialects so lexicographic = chronological.
- **Key grammar arithmetic** — 83/85/542/59/67/29 chars all confirmed; `\x01` storable everywhere.
- **LC4.7 ALTERs** — valid full re-declarations vs 0053 widths; all PKs <3072 bytes; no FKs touch altered columns; `migration_parity` is filename-set only.
- **Cargo.toml delta** — five dep additions all genuinely absent; versions resolve in Cargo.lock; no cycle.
- **`ObjectStore::get` → `tokio::fs::File`** — by-value + second-instance/`Arc` guidance accurate (mod.rs:311,153-156).
- **12-method facade surface** — all exist with pinned signatures; `vcs_get_ref` confirmed absent.
- **SD4.12 corrections register** — all 9 entries accurate; WF5.2's unimplementable payload-path filter correctly superseded.
- **Platform contribution fidelity** — all 11 U4-ND findings integrated with visible landing spots.
- **Coverage completeness** — all 72 upstream_ids have OK rows; carried ids resolve in named targets.

### Findings

| ID | Severity | Finding | Location | Recommended fix |
|----|----------|---------|----------|-----------------|
| U4-AR-01 | minor | `update_for` pinned return type `Vec<VcsDerivedIndexUpsert>` cannot be implemented — facade struct fields are all `&'a str` but `index_key`/`payload` are freshly allocated in-fn (E0515). Forced fix, unambiguous. | LC4.4 | Pin an owned intermediate (`Vec<PathTouchRow{String fields}>`); form `VcsDerivedIndexUpsert` borrows at the tx call site. |
| U4-AR-02 | minor | BR4.15's existence-check discipline isn't restated in SD4.8's walk bullets — absent `a`/`b` in merge_base yields `BaseNotFound` instead of pinned `NotFound` (facade returns `[]` parents for absent); `tree`/`file_at` on absent cs could return ambiguous empty. | SD4.8 | One clause: every read entry existence-checks changeset inputs via `vcs_get_changeset` before walking. |
| U4-AR-03 | nit | Edge list omits `ReadIndex → ChangesetStore::resolve_git_oid` (log `from` 40/64-hex arm); `RefRegistry::resolve` must be `pub(crate)` for ReadIndex. | logical-components.md edges, LC4.3 | Add the edge; pin `resolve` pub(crate). |
| U4-AR-04 | nit | traceability SR4.11 note says "17-row register" — actual is 19 rows (R18/R19 added). | traceability.json | Update count. |
| U4-AR-05 | nit | BR4.11 coverage target `SD4.11-R5` covers only the cost residual; BR4.11's substance (no implicit refs, no head-merging) lives in SD4.4/SD4.6. | traceability.json | Retarget to `SD4.4 + SD4.6`. |
| U4-AR-06 | nit | `diff_lines(&[String])` can't represent non-UTF-8 blob content — File→lines conversion + invalid-UTF-8 error mapping unpinned; the seam propagates to u5. | LC4.1/LC4.5/SD4.8 | Pin `&[Vec<u8>]`/`AsRef<[u8]>` line inputs, or `from_utf8_lossy` + pinned error mapping. |

### Summary

**READY.** The artifacts are exceptionally well-grounded — every code-facing pin exercised against the tree holds. All 11 platform findings integrated with full fidelity; SD4.12's corrections register is accurate on all nine entries. Two minors carry forward (the `update_for` borrow pin — forced canonical fix — and the per-walk existence-check restatement); four nits carry to code generation. No requirement is lost, nothing contradicts the facade or schema.
