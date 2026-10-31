## Review

**Stage:** functional-design
**Unit:** u2-vcs-persistence
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

All 12 prior developer findings verified integrated against real code (read-side RepoActivityRow widening + 3 api de-wrap sites; sqlite rebuild + pg/mysql ALTER; index_key; object_link insert-if-absent; mysql type pins within 3072B; TEXT-JSON; count_for_repo pair; CasOutcome + per-dialect idioms; 7 secondary indexes; insert-ignore-reselect; branch_rename CHECK; misc pins). New findings:

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-13 | major | "FK violation → vcs.not_found" has no mechanism through `Result<_,String>` — only literal impl is banned error-substring matching. Reachable: changes[].blob naming nonexistent object hits blob_hash FK; bogus parents produce dangling lineage (TEXT-JSON, no FK). Pin mechanism: pre-validate inside create_tx with sentinel error, a typed outcome enum, or declare raw FKs defensive-500s with pre-validation owned by u4/u7 | rules.md error table | Unresolved |
| R-14 | minor | Second-level FK delete actions unpinned: refs.changeset_id, derived_index.covering_changeset, repo_surface_flags.enabled_by — pin CASCADE / SET NULL | entities.md | Unresolved |
| R-15 | minor | import_link plain `insert` contradicts idempotent resume — retry after crash → PK violation; pin insert-if-absent or caller lookup order | entities.md:113 | Unresolved |
| R-16 | minor | C5 spec block now stale (object_link insert, count_for_repo, query `key`) — artifact right, contract lags; u2 may amend C5 yaml on landing | contract-summary.md:181-185 | Unresolved |
| R-17 | minor | Wording nits: BR2.4 over-broad (vcs_objects is global, no repo_id — "every repo-scoped VCS table"); mysql upsert idiom `ON DUPLICATE KEY UPDATE` missing from WF1; pick one CHAR/VARCHAR pin | rules.md/spec | Unresolved |
| R-18 | minor | Insert-side Some(...) call sites unenumerated: activity.rs:144,183,220,264,299 + tests repo_activity_list.rs:181, atom_feeds.rs:245,422,436; feeds.rs:546 literal needs Option fields | entities.md:136 | Unresolved |
| R-19 | minor | CAS edge: expected_old NULL+absent+new NULL could insert NULL-changeset ref — pin `new=NULL ⇒ delete path only`; git_oid CHAR(40) assumes sha1-git | entities.md:77,98 | Unresolved |

### Summary

READY. The 12-finding integration is faithful and fully verified; R-13 is a one-line mechanism pin carried to code-generation, the minors are cheap clarifications. No Critical blockers.
