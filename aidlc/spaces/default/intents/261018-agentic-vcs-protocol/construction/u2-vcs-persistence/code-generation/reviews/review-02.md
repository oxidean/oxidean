## Review

**Stage:** code-generation
**Unit:** u2-vcs-persistence
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings
| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| F-01 | minor | MySQL `INSERT IGNORE` masks non-duplicate violations as dedup: standalone link/object inserts return `Ok(false)` on FK/NOT NULL failures — indistinguishable from "already exists". Covered inside `create_tx` by in-tx pre-validation; u3/u4 must pre-create objects or confirm with a SELECT when rows_affected()==0 | crates/oxidean-db/src/vcs.rs:514,615,1077,2126 | Unresolved |
| F-02 | minor | MySQL CAS deviates from pinned `<=>`+rows_affected: uses `SELECT … FOR UPDATE` + in-Rust compare + guarded write in one tx — semantically equivalent-or-stronger, but (a) diverges from BR2.7/SD2.4 text; (b) concurrent create-CAS on absent row can InnoDB-deadlock; victim handled via rollback→actual re-read yet may surface `Err` rather than `Conflict{actual}` if winner uncommitted at re-read | crates/oxidean-db/src/vcs.rs:1747-1865 | Unresolved |
| F-03 | minor | Concurrency test gap: BR2.7 wants "concurrent CAS → one Applied one Conflict{actual}", BR2.8 "concurrent pair → one row"; all tests sequential (pinned sequential semantics all asserted) | crates/oxidean-db/src/vcs.rs:2541-3208 | Unresolved |
| F-04 | minor | pg `CHAR(29)` bpchar footgun: ids pinned to exactly 29 chars so no padding today; a shorter id would read space-padded on pg while mysql/sqlite never pad — latent cross-dialect asymmetry; consider TEXT or edge assertion | migrations/postgres/0053_vcs_schema.sql:30,47,61,71,79 | Unresolved |
| F-05 | minor | `map_activity!` widened fields decode `.unwrap_or(None)` — genuine decode drift silently becomes NULL; commits_count keeps hard map_err; consistent with pre-existing commit_message/pr_number style | crates/oxidean-db/src/repo_activity.rs:61-67 | Unresolved |
| F-06 | minor | `VcsObjectLinkRow` defined + re-exported but never constructed — link writes return bool, live hashes return Vec<String>; dead public surface, keep or drop deliberately | crates/oxidean-db/src/vcs.rs:48-52 | Unresolved |

### Summary
READY (verification re-pass — no regression). 6 carried minor findings, all re-confirmed against current bytes.
