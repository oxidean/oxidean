## Review

**Stage:** functional-design
**Unit:** u3-vcs-object-store
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings
| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| F-01 | Minor | Contract C6 yaml line is stale — still lists `insert_links_tx(repo_id, [hash])`; the C6a table row already matches the design. U3 owns the spec block and may amend on landing. | contract-summary.md:196 vs functional-spec.md WF3 | Unresolved |
| F-02 | Minor | vcs_delete_object_if_unlinked file-delete gating implied not pinned: file delete must be conditioned on the conditional DELETE's rows_affected > 0 (link racing in -> NOT EXISTS fails -> row survives -> deleting the file strands a live link). Pin -> bool + "delete file only when the row was actually deleted" at code-gen. | functional-spec.md WF4.2, rules.md BR3.6 | Unresolved |
| F-03 | Minor | gc_once never specifies releasing the lockfile at pass end — under the age-stale rule a lingering lock makes every pass skip for gc_grace_secs, silently overriding gc_interval. Pin "remove lockfile on pass completion". | functional-spec.md WF4.1, rules.md BR3.6 | Unresolved |
| F-04 | Nit | entities.md says "deleted only by GC pass 2" while BR3.6 orders the orphan-row pass first — cosmetic numbering drift. | entities.md:31 vs rules.md BR3.6 | Unresolved |

### Summary
READY — 4 non-blocking findings. Every checkable claim verified against the landed U2 facade, LFS precedent code, frozen contract, and all three dialect migrations; the two sanctioned facade additions are cleanly addable under existing idioms. All 12 developer-contribution findings incorporated.
