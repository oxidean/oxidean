## Review

**Stage:** code-generation
**Unit:** u2-vcs-persistence
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| F-01 | minor | MySQL `INSERT IGNORE`/`OR IGNORE` standalone inserts mask non-duplicate violations as `Ok(false)` dedup — `create_tx` covered by in-tx pre-validation, but u3/u4 callers of standalone inserts must confirm with a SELECT when `false`. | vcs.rs | Carried (adjudicated) |
| F-02 | minor | MySQL CAS uses `SELECT … FOR UPDATE` + in-Rust compare instead of the pinned `<=>`+rows_affected — semantically equivalent-or-stronger; concurrent create-CAS on an absent row may surface `Err` rather than `Conflict{actual}` as InnoDB deadlock victim. | vcs.rs | Carried (adjudicated) |
| F-03 | minor | No concurrency tests — concurrent-pair races asserted only sequentially. | vcs.rs tests | Carried |
| F-04 | minor | pg `CHAR(29)` bpchar padding footgun — ids pinned to exactly 29 chars today; a shorter id would read space-padded on pg only (latent asymmetry). | pg 0053 | Carried |
| F-05 | minor | `map_activity!` widened fields decode `.unwrap_or(None)` — decode drift silently becomes NULL (consistent with pre-existing style). | repo_activity.rs | Carried |
| F-06 | minor | `VcsObjectLinkRow` defined + re-exported but never constructed — dead public surface. | vcs.rs, lib.rs | Carried |
| I-01 | info | code-summary claims "21 vcs_* methods"; current file carries 22 U2-era + 2 U3-sanctioned — the +1 is `vcs_list_file_changes`, an additive manifest reader the C5 yaml omits. Doc-count nit; contract fully covered. | vcs.rs | Noted |
| I-02 | info | `code-generation-questions.md` absent — conditional artifact, same shape as u1. | stage dir | Noted |

### Summary

READY re-verified on current bytes — zero Critical, zero Major. U2's
contribution stands intact beneath U3's sanctioned delta: all three 0053
migrations match FK/index pins in substance; the facade implements every design
pin (in-tx sentinel pre-validation, idem reselect, CasOutcome with delete-only
new=None, insert-if-absent links/imports, per-dialect idioms, clamp(1,500),
grace-bounded orphan scan, AssertSqlSafe, require_pool fail-closed). API
de-wraps type-consistent end-to-end; all 15 pinned tests present; manifest
covers all 12 writes; traceability upstreams genuine; C5 errata landed. The
U3 facade additions are expected composition, not U2 defects. Six carried
minors previously adjudicated, re-confirmed accurate, non-blocking.
