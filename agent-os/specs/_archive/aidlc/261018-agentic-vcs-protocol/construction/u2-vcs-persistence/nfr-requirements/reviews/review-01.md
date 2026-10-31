## Review

**Stage:** nfr-requirements
**Unit:** u2-vcs-persistence
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| R-01 | major | SR2.8 claims FK actions "as pinned in entities" — they aren't; and for object-pointing FKs (object_links.hash, file_changes.blob_hash) SET NULL/CASCADE are both harmful — RESTRICT/NO ACTION is the correct fail-safe (protects manifest history vs GC); enabled_by SET NULL correct; changeset-pointing FKs CASCADE | security-requirements.md SR2.8 | Unresolved |
| R-02 | major | Traceability NFR2/NFR5/NFR8 mislabeled: NFR2 (lock-free writes)→SR2.4/TD2.4; NFR5 (wire compat)→TD2.3 CasOutcome; NFR8 (typed AppError)→honest partial note; add missing NFR6 (dialect parity), NFR9 (security), covered BRs, AC1.3.2/AC3.5.2/FR1.5 | traceability.json | Unresolved |
| R-03 | minor | TD2.2 "matches exactly" overstates — in-module impl Database is a deliberate variation; no pool() accessor exists (require_pool/dialect/self.pool) | tech-stack-decisions.md | Unresolved |
| R-04 | minor | SR2.1 "query!-style macros" — crate has zero macro call sites; real pattern is runtime sqlx::query*+.bind() | security-requirements.md | Unresolved |
| R-05 | minor | Paginated façade methods need in-crate limit clamps (house precedent repo_activity.rs:185 clamp(1,500)) | security-requirements.md gap | Unresolved |
| R-06 | minor | Carry-over: import_link still plain insert (crash-retry PK violation); C5 yaml stale vs entities.md | upstream | Unresolved |

### Summary

READY. NFR set covers the real DB-unit risks; all ~20 code-level precedent claims verified verbatim (AssertSqlSafe, migrate parity, star_repository idiom, lfs SUM-JOIN, FK-side index, mysql/pg/sqlite recipes, JSON-in-TEXT, index_key, sha256:-wire-only, 0053 numbering, timestamptz cast). Two Majors are small precise fixes for code-generation.
