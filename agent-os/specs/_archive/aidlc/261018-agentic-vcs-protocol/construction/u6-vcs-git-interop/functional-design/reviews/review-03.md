## Review

**Stage:** functional-design
**Unit:** u6-vcs-git-interop
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| F1 | MINOR | `is_ancestor_oid` duplicates existing `GitBackend::is_ancestor` (backend.rs:735-740; cli.rs:3332-3368 already runs `git merge-base --is-ancestor` with identical 0/1/else exit typing) — reuse, drop invented duplicate | entities.md:32 / questions Q2:49 | Open |
| F2 | MINOR | 0055 amendment scope misses the read side: `VcsDerivedIndexRow.covering_changeset` is `String` decoded via `map_index_row!` try_get (vcs.rs:100,281-283) — NULL rows fail decoding on query; extend to `Option<String>` | questions Q3:66 / entities:52 | Open |
| F3 | MINOR | `skipped_tips` claimed on wire and head `changeset_id` optional, but `VcsGitImportResponse` lacks the field and `VcsGitImportHead.changeset_id` is non-optional (vcs_types.rs:884,901-908) — record both as u1/u7 DTO amendments or drop wire claim | entities:11 / BR6.28 / traceability contract:vcs.git_import | Open |
| F4 | MINOR | BR6.13 sanitize vs BR6.23 refuse contradiction: sanitizing control bytes to `?` makes the control-byte refusal unreachable — pin one ordering (sanitize then refuse `<`/`>`/empty) stated once | rules.md BR6.13 vs BR6.23 | Open |
| F5 | MINOR | `commit_name_status` `-z` rename field order unpinned: git emits `R<score>\0<dst>\0<src>` (new-then-old, reversed from non-z) — a swapped parse silently inverts renames while validating | entities:28 / BR6.3 | Open |
| F6 | NIT | `manifest_diff`/`compose_author` referenced by name but are private fns (merge_engine.rs:1059, changeset_store.rs:439) unreachable under the no-u4/u5-edit constraint — mark as mirrored logic, not calls (`validate_path` :241 is `pub(crate)`, callable) | functional-spec WF5.2 / BR6.23 / Q5.5 | Open |
| F7 | NIT | `lazy_import` reuses `import_chain(from=oid-as-tip)` inheriting heads-preferred `from` resolution — a branch literally named a 40-hex oid shadows the target; pin direct rev resolution bypassing heads/tags tries | functional-spec WF2:3 / Q4:83 | Open |

### Summary

Revision-3 re-review under run floor STAGE_JUMPED#15 (request `review:74cbca57a136a4a17359d1a3ae5e0953`). The design specifies `GitInterop` — a `server`-feature `git_interop.rs` in `oxidean-vcs` — for deterministic git↔changeset import/export, plus a `GitBackend` trait extension, an in-crate `ProtectionChecker` port, and a u2 amendment making `vcs_derived_index.covering_changeset` nullable. Every prior NOT-READY finding was re-verified against real code and is genuinely resolved: `ProtectionBlockReasons` (protection_types.rs:267, wire-serialized via `forbidden`/`VcsForbiddenData` at vcs_types.rs:394/443), hook-parity intent derivation from `(old,new)` (protection/mod.rs:842-856), all nine `ObjectStoreError` variants enumerated, the explicit bounded lazy-import loop, and the two-phase ff gate with `Option<ChangesetId>` `RefConflict.actual`. Facade names, enum variants, DDL column types, method signatures, and plumbing argv (`-c` before subcommand, mysql `VARCHAR(32)`, TEXT cursor capacity) all spot-checked accurate. Residual risk is seven MINOR/NIT items: one duplicated backend method, one incomplete amendment scope on the row read side, two contract-drift items on the `vcs.git_import` response DTO, one sanitize/refuse ordering contradiction, and three unpinned impl traps — none blocking construction, all cheap to integrate in the post-review pass. READY.
