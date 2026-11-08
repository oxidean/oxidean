## Review

**Stage:** code-generation
**Unit:** u6-vcs-git-interop
**Reviewer:** aidlc-architecture-reviewer-agent
**Verdict:** NOT-READY
**Iteration:** 1

### Findings

| ID | Severity | Finding | Location | Status |
|----|----------|---------|----------|--------|
| 1 | MAJOR | `export_one` early-returns an unverified `linked:` oid — `export_order` de-anchors gc'd links for re-mint, but `export_one` re-reads the row and returns the stale oid; children then `commit-tree -p` a dangling parent (`Internal`), or `minted_tip` itself dangles when the head was the sole scheduled node. Pin requires "gc'd links count as unlinked": early-return only on `verify_commit_oid` `Some`; else re-mint and let the idempotent link upsert converge. | `git_interop.rs` `export_one` entry | Resolved |
| 2 | MINOR | `map_git_err`'s `Conflict` arm emits `RefConflict{refname:"",actual:None}` — export never hits it (it re-resolves properly), so document that the arm is for non-export callers. | `git_interop.rs` `map_git_err` | Resolved |
| 3 | MINOR | `let mut ff_ok = existing.is_none();` is subtle — add "unborn ref = trivially ff" comment. | `git_interop.rs` `export` | Resolved |
| 4 | NIT | `import_claimed` takes `from` though the seeded path bypasses it — harmless but slightly misleads. | `git_interop.rs` `import_claimed` | Resolved |
| 5 | NIT | `unix_to_rfc3339(-1)` test asserts `is_err() \|\| is_ok()` (tautology) — pin the intended arm. | `git_interop.rs` tests | Resolved |

### Summary

Adversarial pass over the bound bytes: `git_interop.rs`, the eight new `GitBackend` methods + `actor_capability` plumbing, the `Option` `covering_changeset` column + `0055` migrations ×3, the two DTO amendments, and the `oxidean-api` call sites.

One MAJOR correctness defect (finding 1): a gc'd `linked:` row is correctly de-anchored by `export_order` but re-read without verification inside `export_one`, so the stale oid escapes as a parent/tip. Verified good: refusal-inventory-before-writes, full-bounded-walk `rev_list` with `truncated→TooLarge`, nonce-verified claim/release/cursor-upsert, exactly one hook-gated move with the checker first, `skipped_tips` surfacing, gitlink drop+warn, value-free error text, and the byte-exact round-trip golden test. NOT-READY pending integration of finding 1.
