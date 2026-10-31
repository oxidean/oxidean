---
phase: 07-git-repos-browse
reviewed: 2026-09-12T20:03:32Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - crates/oxidean-git/src/cli.rs
  - crates/oxidean-api/src/repo/mod.rs
  - crates/oxidean-api/src/routes/repo_raw.rs
  - crates/oxidean-api/tests/repo_branch_soft_protect.rs
  - crates/oxidean-api/tests/repo_archive.rs
  - crates/oxidean-api/tests/repo_create.rs
  - apps/web/src/lib/repo-browse.ts
  - apps/web/src/lib/repo-browse.unit.test.ts
  - apps/web/src/routes/$owner.$repo.tree.$.tsrx
  - apps/web/src/routes/$owner.$repo.blob.$.tsrx
  - apps/web/src/routes/$owner.$repo.blame.$.tsrx
findings:
  critical: 0
  warning: 4
  info: 3
  total: 7
status: issues_found
---

# Phase 07: Code Review Report

**Reviewed:** 2026-09-12T20:03:32Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Gap-closure plans **07-19 / 07-20 / 07-21** close the prior **CR-01** (archive `--output=`), **CR-02** (`branchCreate(-D)`), **WR-01** (create compensate), **WR-02** (raw `/` parity), and **WR-03** (longest-prefix `parseRefAndPath`) issues from the earlier phase review. Defense-in-depth is real: leading-`-` rejects at CLI + HTTP/API, and `"--"` precedes user operands on archive and branch mutate. Residual **warnings** are incomplete API reject on rename/delete, silent hierarchical-parse fallback when `repo.refs` fails, soft_delete errors swallowed in compensate, and a too-loose injection regression assertion.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: `branch_rename` / `branch_delete` skip API `reject_option_like_branch`

**File:** `crates/oxidean-api/src/repo/mod.rs:543-610` (contrast create at `521-528`)
**Issue:** `repo.branchCreate` rejects leading-`-` names at the API before git. Rename/delete rely only on CLI `validate_treeish` + `"--"`. Not exploitable today (CLI blocks), but defense-in-depth is inconsistent with the CR-02 pattern and a future CLI bypass would hit rename/delete first.
**Fix:** Call `reject_option_like_branch` on `from`/`to` (rename) and `branch` (delete) the same way as create:

```rust
reject_option_like_branch(from)?;
reject_option_like_branch(to)?;
// ...
reject_option_like_branch(branch)?;
```

### WR-02: Hierarchical browse silently regresses when `repo.refs` fails

**File:** `apps/web/src/routes/$owner.$repo.tree.$.tsrx:80-84` (same pattern in blob `79-83`, blame `84-87`)
**Issue:** On `refsRes.ok === false`, routes set `knownRefs = []` and still call `parseRefAndPath(splat, knownRefs)`. Empty known refs forces first-segment split, so `feature/foo/src` becomes ref=`feature`, path=`foo/src` — WR-03 / D-17 breaks without a user-visible refs error.
**Fix:** If refs fail after a successful `repo.get`, surface an error phase (or retry) instead of parsing with an empty known-ref list; only fall back to first-segment when refs intentionally empty (new empty repo).

```typescript
if (!refsRes.ok) {
  setPhase({ kind: "error", message: refsRes.error.message || "Could not load refs." });
  return;
}
const knownRefs = refsRes.data.refs.map((r) => shortRefName(r.name));
```

### WR-03: `compensate_failed_create` ignores soft-delete failure

**File:** `crates/oxidean-api/src/repo/mod.rs:104-111` (callers `726`, `749`)
**Issue:** If `soft_delete_repository` fails, compensate only logs and create still returns `repo.git_init_failed` / `repo.git_seed_failed`. The live row remains (`deleted_at IS NULL`), so the name stays blocked — the WR-01 failure mode the compensate path was meant to eliminate.
**Fix:** Propagate soft-delete failure (or retry once) and return a distinct error so the client/ops know the row was not cleared; do not claim a clean git-only failure:

```rust
async fn compensate_failed_create(...) -> Result<(), AppError> {
    ctx.db.soft_delete_repository(repo_id).await.map_err(|e| {
        tracing::error!(error = %e, repo_id, "soft_delete_repository failed during create compensate");
        AppError::new("repo.create_compensate_failed", "failed to clean up after repository create error")
    })?;
    // best-effort path remove...
    Ok(())
}
```

### WR-04: Option-injection regression assertion accepts any `repo.*` code

**File:** `crates/oxidean-api/tests/repo_branch_soft_protect.rs:251-255`
**Issue:** The CR-02 test allows `code.starts_with("repo.")`, so a mis-routed `repo.git_failed` / unrelated `repo.*` error would still pass while soft-protect on `main` might coincidentally hold. Weakens the regression harness for the exact fail-closed contract (`repo.invalid_ref`).
**Fix:** Assert the specific code (and optionally message shape) produced by `reject_option_like_branch`:

```rust
assert_eq!(
    create["error"]["code"], "repo.invalid_ref",
    "option-like branchCreate must be invalid_ref — {create}"
);
```

## Info

### IN-01: `head()` still first-segment-only for hierarchical refs

**File:** `apps/web/src/routes/$owner.$repo.tree.$.tsrx:22-25` (blob `22-25`, blame `20-23`)
**Issue:** Document titles can show the wrong path segment for slashy branches because `head()` calls `parseRefAndPath(splat)` without `knownRefs` (intentional per 07-21). Page body parse is correct after refs load.
**Fix:** Optional later: load refs in a route loader before `head`, or omit path from title until body resolves.

### IN-02: Duplicated HTTP ref validators risk drift

**File:** `crates/oxidean-api/src/routes/repo_raw.rs:42-90`
**Issue:** `validate_ref` and `validate_archive_treeish` are identical. Future hardening on one path can miss the other (as happened historically with `/` bans).
**Fix:** Extract a shared `validate_http_treeish` used by both raw and archive.

### IN-03: Non-branch/archive git calls still omit `"--"` before revisions

**File:** `crates/oxidean-git/src/cli.rs` (`ls_tree` ~445, `log` ~614, `show` ~641, `blame` ~812, `diff` ~737)
**Issue:** Those commands place user treeish after flags without an end-of-options marker. Leading-`-` is already rejected by `validate_treeish` (07-19), so CR-01-class injection is blocked; `"--"` would be extra belt-and-suspenders only.
**Fix:** Optionally insert `"--"` before revision operands for consistency with archive/branch.

---

_Reviewed: 2026-09-12T20:03:32Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
