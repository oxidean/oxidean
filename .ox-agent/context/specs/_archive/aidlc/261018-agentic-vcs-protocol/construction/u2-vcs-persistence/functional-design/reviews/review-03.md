## Review

**Stage:** functional-design
**Unit:** u2-vcs-persistence
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| RV-7 | minor | entities.md `git_oid CHAR(40)` literal stale — landed mysql column is CHAR(64) per the file's own type-conventions header and the C5 errata. Internal inconsistency only. | entities.md | Cosmetic residue |
| RV-8 | minor | entities.md lists `import_link: {insert,...}`; landed/contract form is `insert_import_link_if_absent`. Naming drift, semantics pinned elsewhere. | entities.md | Cosmetic residue |
| RV-9 | minor | BR2.7's mysql idiom text (`<=>` + rows_affected) describes a different mechanism than landed `SELECT … FOR UPDATE` + guarded write in one tx — contract-equivalent, arguably stronger. Doc drift only. | rules.md BR2.7 | Cosmetic drift |
| RV-10 | minor | BR2.4 "Every VCS table" is loose — `vcs_objects` is global (no repo_id) by design; second-level FK delete actions unpinned in entities.md (landed sensibly). | rules.md | Cosmetic residue |

### Summary

Re-verification pass on unchanged bytes: prior READY holds — zero Critical,
zero Major. AC coverage confirmed for U2's assigned stories (US1.3 primary,
US2.7 share; AC2.7.2 correctly unclaimed — u7's recorder share; enabling
claims all map to real BRs). Entity shapes match the landed 8-table ×
3-dialect migrations byte-for-byte in substance (index_key, CHAR(29)/CHAR(64),
bounded VARCHARs, TEXT-JSON parents, 7 secondary indexes). BR2.x coherent with
code (sole-SQL-site + sanctioned de-wraps, require_pool fail-closed, per-dialect
recipes, single-tx ordering, insert-if-absent→re-SELECT, merge upserts, rows in
db re-exported). FK actions match landed SQL (CASCADE repo/changeset;
RESTRICT/NO ACTION object-pointing; SET NULL enabled_by). The R-13 FK-error
mechanism major is fully resolved in code (typed sentinels + in-tx
pre-validation, codes registered in rpc_gen manifest). U3's sanctioned facade
delta present — expected composition. Residual items cosmetic literal drifts.
