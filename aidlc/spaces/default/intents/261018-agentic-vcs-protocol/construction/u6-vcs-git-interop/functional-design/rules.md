# Rules — u6-vcs-git-interop

BR6.x — binding rules for `GitInterop`. Wire/persistence invariants live in u1/u2/u4; store/merge mechanics in u4/u5; these govern the git↔changeset bridge only. *(Revision 2 — dev pass + architecture review pass-1 folded in: nullable covering amendment, parents-first enumeration, forward ff-gate lookup, consumed-window cursor, Option heads, nonce-verified release, force-intent checker.)*

```yaml
rules:
  - id: BR6.1
    statement: >-
      Changesets write only through ChangesetStore::create — import and export never write rows directly.
    category: >-
      constraint
    applies_to: >-
      import_one, export
    trigger: >-
      every created changeset
    logic: >-
      IF a commit is being converted THEN the only write path is store.create — never hand-assembled inserts.
    violation: >-
      none — no other path exists.
    source: >-
      FR7.4, AC7.1.1
  - id: BR6.2
    statement: >-
      Lazy import and explicit backfill call the same import_one verbatim — convergence is structural, not tested-coincidence.
    category: >-
      calculation
    applies_to: >-
      import_chain, lazy_import
    trigger: >-
      any import
    logic: >-
      IF a git commit is converted THEN it flows through the single import_one — identical canonical inputs ⇒ identical changeset ids regardless of entry point. Already-linked commits early-skip on BOTH paths.
    violation: >-
      two parallel conversion implementations forbidden.
    source: >-
      AC7.1.2, AC7.4.2, FR9.4
  - id: BR6.3
    statement: >-
      Name-status flags are pinned config-independent — `-r --name-status -M -z -c diff.renameLimit=10000`, `--root --no-commit-id` on roots — identical on both paths.
    category: >-
      constraint
    applies_to: >-
      commit_name_status
    trigger: >-
      every per-commit diff
    logic: >-
      IF a commit's file delta is computed THEN the flags are exactly this set — `-M` at git's default similarity, `diff.renameLimit` pinned so per-repo config can't shift rename resolution between paths (dev 8); `--no-commit-id` required on the root form (it emits a leading commit-id record, dev 10); `-z` rename rows carry TWO path records consumed together (dev 20).
    violation: >-
      caller-supplied flags, config-dependent rename limits, missing --no-commit-id.
    source: >-
      AC7.1.2
  - id: BR6.4
    statement: >-
      Import is idempotent — insert-if-absent links + idem_key converge re-runs; false-return verifies stored id == ours else Internal.
    category: >-
      constraint
    applies_to: >-
      import_chain, lazy_import
    trigger: >-
      retry, crash recovery, raced imports
    logic: >-
      IF insert_import_link_if_absent returns false THEN stored changeset_id must equal the just-created one else Internal (divergent mapping = corruption). A `linked:{cs}` reverse-map derived row upserts alongside every link — heals interrupted pairs.
    violation: >-
      divergent git_oid mappings never overwrite.
    source: >-
      AC7.1.1, FR9.4
  - id: BR6.5
    statement: >-
      Topo-order invariant — a processed commit's git parents are all linked or earlier in the same batch; any miss ⇒ Internal.
    category: >-
      constraint
    applies_to: >-
      import_chain, lazy_import
    trigger: >-
      per-commit parent mapping
    logic: >-
      IF any parent oid lacks an ImportLink at process time THEN Internal — never reparent or fabricate.
    violation: >-
      changeset with wrong parents forbidden.
    source: >-
      AC7.1.3
  - id: BR6.6
    statement: >-
      The import claim is a BEST-EFFORT marker — no CAS verb exists; nonce + verify-read + ttl + re-verify-before-cursor-write shrink the window; convergence (BR6.4) is the correctness backstop.
    category: >-
      constraint
    applies_to: >-
      import_chain, lazy_import
    trigger: >-
      call entry
    logic: >-
      IF fresh active marker (< ttl) or nonce verify fails THEN ImportState; stale markers reclaimable; `{idle}` release upsert runs ONLY after nonce re-verify — a claim loser's release must not overwrite the winner's active marker (review-minor); two racers can both pass the re-read (dev 12) — a lost race converges through links/idem rather than corrupting.
    violation: >-
      claiming the marker guarantees mutual exclusion — it doesn't; never rely on it for correctness.
    source: >-
      contract vcs.import_state
  - id: BR6.7
    statement: >-
      The cursor is a continuity-checked snapshot — tips bind at first call, `skip` indexes the consumed-window position, `last_oid` identifies the final CONSUMED entry (not the final fresh conversion).
    category: >-
      policy
    applies_to: >-
      import_chain
    trigger: >-
      resume with cursor
    logic: >-
      IF cursor decodes THEN its tip snapshot is re-derived (not live refs); `skip>0` ⇒ `list[skip-1] == last_oid` else ImportState — a git upgrade/GC between pages can re-derive a different topo order and a bare skip silently drops commits (dev 13); `skip` advances by the whole consumed window INCLUDING already-linked/skipped entries (review-major — `skip += imported` breaks resumes); `last_oid = list[new_skip-1]`; `out.imported` counts fresh conversions ONLY; `from`+`cursor` co-supplied ⇒ cursor authoritative, `from` ignored; tips > cursor_tips_max ⇒ TooLarge; malformed ⇒ ImportState (never invalid_input).
    violation: >-
      resuming on a reordered list, resuming against moved refs, counting skipped entries as imported.
    source: >-
      contract limits, dev 13/22d, review-major 4
  - id: BR6.8
    statement: >-
      Enumeration bound + PARENTS-FIRST order — rev_list carries a max and reports truncation; `RevList.oids` is parents-first (backend reverses the children-first `--topo-order` output in-crate — `--reverse`+`--max-count` would truncate the wrong end, review-BLOCKER); truncated ⇒ TooLarge; empty tips ⇒ empty result.
    category: >-
      constraint
    applies_to: >-
      import_chain, lazy_import
    trigger: >-
      ancestor enumeration
    logic: >-
      IF RevList.truncated THEN TooLarge — a >rev_list_max SINGLE ancestor-cone cannot import at all (honest bound: `rev-list` cannot express `^oid` exclusions; per-ref `from` only helps multi-cone repos, review-nit e); `tips.is_empty()` ⇒ `{imported:0, done:true}` never Internal (dev 15).
    violation: >-
      unbounded output materialization, silent truncation, children-before-parents processing.
    source: >-
      NFR-scale guard, review-BLOCKER 2
  - id: BR6.9
    statement: >-
      Merge-commit changes list = diff vs first parent; ≥2-parent structure preserved in the parents list.
    category: >-
      calculation
    applies_to: >-
      import_one
    trigger: >-
      parents.len() >= 2
    logic: >-
      IF merge THEN name-status two-tree form vs parents[0]; all parents map to changeset parents in stored order.
    violation: >-
      combined-diff or per-parent splits forbidden.
    source: >-
      AC7.1.3, FR2.4 model
  - id: BR6.10
    statement: >-
      Mode mapping verbatim via tree_flat; blob size pre-checked before cat_blob.
    category: >-
      calculation
    applies_to: >-
      import_one
    trigger: >-
      per name-status row
    logic: >-
      IF a row maps THEN mode comes from the flat listing on the appropriate side — never defaulted; `size > import_blob_max` ⇒ TooLarge BEFORE the read (dev 17).
    violation: >-
      hardcoded 0o100644; post-read size enforcement.
    source: >-
      AC7.1.1
  - id: BR6.11
    statement: >-
      Gitlink rows skipped + warned — no VCS entity represents submodule pointers; the rule covers either side (gitlink→file renames too).
    category: >-
      constraint
    applies_to: >-
      import_one
    trigger: >-
      name-status row whose new OR old side resolves mode 0o160000/kind=commit
    logic: >-
      IF either side is a gitlink THEN skip + tracing::warn — deterministic on both paths; exports of that history emit no gitlink (recorded v1 residual).
    violation: >-
      fabricating an illegal-mode entry; silent skips.
    source: >-
      LEGAL_MODES (changeset_store.rs:38), dev 20
  - id: BR6.12
    statement: >-
      Import writes RAW-byte ident sidecars AFTER create — `ident:{git_oid}` + `ident-cs:{cs}` carrying base64'd author+committer fields — so export restores byte-exact idents even where the changeset author was sanitized.
    category: >-
      calculation
    applies_to: >-
      import_one, export_one
    trigger: >-
      every imported commit / every exported changeset
    logic: >-
      IF a commit is imported THEN both sidecar keys upsert {a_name_b64,a_email_b64,a_unix,a_tz,c_name_b64,c_email_b64,c_unix,c_tz} AFTER store.create returns the id; export prefers `ident-cs:` raw bytes over the collapsed author/committed_at reconstruction.
    violation: >-
      dropping raw fields collapses AC7.4.1 byte-identity to instant-identity; writing sidecars before create dangles rows (dev 22e).
    source: >-
      AC7.4.1, FR9.3, dev 5
  - id: BR6.13
    statement: >-
      Messages AND paths AND idents UTF-8-validate — non-UTF8 ⇒ InvalidInput deterministically on both paths; never lossy-convert.
    category: >-
      validation
    applies_to: >-
      import_one
    trigger: >-
      per-commit conversion
    logic: >-
      IF message bytes OR a path OR an author part fails UTF-8 THEN InvalidInput (the raw sidecar still preserves bytes for ident, but the changeset's own fields must be valid strings); author sanitizes {control,0x7f}→'?' identically on both paths.
    violation: >-
      U+FFFD-collapse on paths (dev 6), lossy message conversion.
    source: >-
      AC7.1.3, AC7.4.1, AC7.4.2
  - id: BR6.14
    statement: >-
      Export anchors on `linked:{cs}` derived rows verified against the object store — the unlinked ancestor prefix of head is the export set; every node's git parents resolve via linked rows or earlier-minted oids; a linked anchor whose git object was gc'd (verify_commit_oid misses) is treated as UNLINKED so re-mint heals.
    category: >-
      calculation
    applies_to: >-
      export
    trigger: >-
      chain walk
    logic: >-
      IF a changeset's parent resolves neither THEN Internal; set > export_chain_max ⇒ TooLarge; a `linked:` row surviving a denied/pruned export points at an unreferenced object — the existence probe on the NEXT walk re-mints through it (review-major). NO new facade — the reverse map lives in derived index.
    violation: >-
      exporting with unresolved parents; trusting a linked anchor whose object is gone (commit-tree -p <missing> ⇒ Internal); O(total-links) probing via import_link_page.
    source: >-
      AC7.2.1, dev 1, review-major 6
  - id: BR6.15
    statement: >-
      Ordering is write-chain → check → move — the checker needs the minted tip to evaluate the real (old,new) pair; a denied export leaves only unreferenced objects PLUS link/linked rows pointing at them — both healed by the next walk's existence probe (BR6.14); zero ref change.
    category: >-
      authorization
    applies_to: >-
      export
    trigger: >-
      after chain mint, before move
    logic: >-
      IF checker.check denies THEN Forbidden{verdicts} — push-denial-identical payload; force never bypasses it. Objects write first because the verdict evaluates the real (old,new) pair (dev 3); concurrent `git gc --prune` can reap minted-unreferenced objects before the move — same exposure `commit_files` already carries; a re-export re-mints (review-nit g).
    violation: >-
      checking before the tip exists (unevaluable), moving before checking, claiming a denied export wrote nothing.
    source: >-
      AC7.2.2, FR9.2, dev 3, review-major 6/nit g
  - id: BR6.16
    statement: >-
      force overrides fast-forward only — the ff gate resolves the existing tip via FORWARD `vcs_lookup_import_link(old_oid)` (review-BLOCKER: `linked:` is keyed on changeset ids and can never hit a git oid); an unlinked old tip (git-only history) is ALWAYS non-ff ⇒ RefConflict{actual} regardless of force; the protection check and hooked move never skip.
    category: >-
      authorization
    applies_to: >-
      export
    trigger: >-
      ff gate
    logic: >-
      IF old exists THEN `link_lookup(old_oid)` miss ⇒ RefConflict; hit ⇒ `is_ancestor_cs(old_cs, head)` OR `old_cs ∈ set` (set-membership covers the healed-half-pair: forward link present, `linked:` absent ⇒ re-mint identical oid ⇒ verify-equal ⇒ `linked:` upsert heals); non-ff ⇒ force ? proceed : RefConflict. Empty-set fast-path: head already anchored AND old == anchored oid ⇒ early `{git_oid: old, commits_count: 0}` — no writes, no move (review-nit h).
    violation: >-
      clobbering git-only commits; treating force as a protection bypass; reverse-direction lookups at the ff gate.
    source: >-
      AC7.2.2, contract limits, review-BLOCKER 3
  - id: BR6.17
    statement: >-
      Exactly one hook-driven, actor-capable move — push stderr classified in-method; never update_ref, commit_files, push_to_url, or automatic mirroring.
    category: >-
      constraint
    applies_to: >-
      export
    trigger: >-
      after chain write + verdict
    logic: >-
      IF the chain is written THEN move once via fast_forward_ref or force_update_ref carrying actor_capability; both methods run the ported classify_push_error (non-ff→Conflict, hook-decline→Denied — dev BLOCKER 3: they currently return raw Process); Denied ⇒ Forbidden{push_hook}; Conflict ⇒ RefConflict{re-read tip}.
    violation: >-
      bare update_ref, per-commit pushes, un-classified Process errors escaping as Internal.
    source: >-
      AC7.2.1, AC7.2.2, unit boundary, dev 3
  - id: BR6.18
    statement: >-
      Error bridge is total across ALL consumed enums — GitError, ChangesetError, IndexError, ObjectStoreError, facade String.
    category: >-
      validation
    applies_to: >-
      all workflows
    trigger: >-
      any error
    logic: >-
      GitError: NotFound→NotFound, Denied→Forbidden{push_hook}, Conflict→RefConflict (move) else Internal, InvalidArg→InvalidInput for validation-shaped calls else Internal, Process/Io→Internal (FIXED message — stderr may carry paths). ChangesetError (changeset_store.rs:56-74 — `ParentNotFound|ObjectNotFound|ChangesetNotFound|InvalidInput|Internal|DbNotConfigured`, NO quota variant — review-minor): ParentNotFound/ChangesetNotFound→NotFound, InvalidInput→InvalidInput, ObjectNotFound→Internal (blob raced GC = integrity loss), DbNotConfigured→DbNotConfigured, Internal→Internal. IndexError: CursorMalformed→Internal (engine-minted = corruption), DbNotConfigured→DbNotConfigured, rest→Internal. ObjectStoreError: QuotaExceeded→QuotaExceeded(scope), InvalidHash/NotFound/Stream→Internal, DbNotConfigured→DbNotConfigured. Facade String→Internal.
    violation: >-
      unmapped variants; raw stderr/rejected-value echo in error text (devsecops-8 convention).
    source: >-
      contract taxonomy, dev 18a
  - id: BR6.19
    statement: >-
      Caps typed + construction-validated — InteropCaps::validate pins blob caps ≤ objects.max_object_bytes; every overflow is the typed error, never clamp.
    category: >-
      constraint
    applies_to: >-
      all workflows
    trigger: >-
      construction + each cap site
    logic: >-
      IF a cap trips THEN TooLarge (or InvalidInput for limit>cap); fields: import_commit_cap(1000), rev_list_max(250_000), cursor_tips_max(512), import_tree_max(10_000), import_blob_max, export_chain_max(10_000), export_blob_max, export_ancestor_probe_max(250_000), import_lock_ttl_secs(900).
    violation: >-
      silent clamping; missing export_ancestor_probe_max for the ff BFS (dev 18b).
    source: >-
      contract cap, NFR guards, dev 18b
  - id: BR6.20
    statement: >-
      Actor capability threads end-to-end — checker.check AND the move get the caller's real capability; the backend methods take the new param.
    category: >-
      authorization
    applies_to: >-
      export
    trigger: >-
      check + move
    logic: >-
      IF export runs for actor X THEN both evaluate as X — hardcoding "admin" inside interop forbidden (the hook reads OXIDEAN_ACTOR_CAPABILITY; production hooks deny unmapped capabilities).
    violation: >-
      admin-evaluated user exports = protection bypass.
    source: >-
      AC7.2.2, ADR-006
  - id: BR6.21
    statement: >-
      State lives only in sanctioned rows — derived keys {active, cursor, ident:<oid>, ident-cs:<cs>, linked:<cs>} + vcs_import_links; no new tables, no in-memory resumable state. `active`/`cursor` marker rows carry `covering_changeset = NULL` — the u2 amendment (migration 0055 makes the column nullable; `VcsDerivedIndexUpsert.covering_changeset → Option<&str>`) is u6-authored and lands in u6's manifest (review-BLOCKER: `""` violates the NOT NULL FK in all three dialects).
    category: >-
      constraint
    applies_to: >-
      all workflows
    trigger: >-
      any state write
    logic: >-
      IF state persists THEN it uses the u2 facades only; `covering_changeset = None` for active/cursor rows (last imported cs where one exists, still NULL-safe), `Some(cs)` for ident/linked rows.
    violation: >-
      `""`/placeholder covering values (FK violation); local files/singletons; a marker store that requires a changeset before any exists.
    source: >-
      u2 boundary, dev 1, review-BLOCKER 1
  - id: BR6.22
    statement: >-
      Observability — tracing spans per op (unit=vcs.git_interop); gitlink skips warn; no content/secret/path leakage in errors.
    category: >-
      policy
    applies_to: >-
      all workflows
    trigger: >-
      op entry/exit, warnings, errors
    logic: >-
      IF an op runs THEN span + structured fields (repo_id, op, counts) — never blob contents or auth material.
    violation: >-
      silent skips; stderr echo.
    source: >-
      NFR8
  - id: BR6.23
    statement: >-
      Deterministic-refusal inventory — commits with >64 parents, >65535-byte messages, unrepresentable timestamps (`from_timestamp` `None` ⇒ InvalidInput, never panic — review-nit c), >512-char paths, `.`/`..` segments or control bytes in paths (validate_path:241-266 full set), author name/email containing `<`/`>`/control bytes/DEL or empty post-sanitize (compose_author:446-455 — email unsanitized), or non-UTF8 paths/messages/idents refuse via InvalidInput on BOTH paths.
    category: >-
      validation
    applies_to: >-
      import_one
    trigger: >-
      per-commit conversion
    logic: >-
      IF a commit hits a refusal bound THEN InvalidInput — such histories need git-side remediation; degradation is never an option (convergence would fork). Dash-led `from`/rev values are rejected caller-side BEFORE any git argv (review-nit f — dash-led values parse as flags).
    violation: >-
      silent truncation or substitution that diverges between paths; unwrap-panic on timestamp bounds.
    source: >-
      dev 23, u4 schema bounds, review-nit c/d/f
  - id: BR6.24
    statement: >-
      Backend-extension pins — verify_commit_oid peels to commits + types misses; check_refname is git-parity; mktree -z + {:06o} modes; commit.gpgSign=false; stdin messages; six ident envs; cat_commit tolerates folded headers.
    category: >-
      constraint
    applies_to: >-
      all backend additions
    trigger: >-
      method authoring in code-generation
    logic: >-
      IF a plumbing call runs THEN: `rev-parse --verify --quiet {rev}^{commit}` for resolution (tag peel + NotFound-typing, dev 4/11); `git check-ref-format` for refnames (dev 16); `mktree -z` (non-z C-unquotes names, dev 9); `-c commit.gpgSign=false` (repo config can't trigger signing, dev 7); message via stdin (NUL-safe, dev 7); `GIT_{AUTHOR,COMMITTER}_{NAME,EMAIL,DATE}` all set; cat-file continuation headers skipped, malformed ident → typed error not panic (dev 19).
    violation: >-
      unpinned plumbing behavior that varies with repo config or output shape.
    source: >-
      dev findings 4,7,9,11,16,19
  - id: BR6.25
    statement: >-
      The protection check carries force-intent — `checker.check(..., force)` maps to the evaluator's Push-vs-ForcePush split (`allow_force_pushes` bites only under ForcePush).
    category: >-
      authorization
    applies_to: >-
      export
    trigger: >-
      verdict evaluation
    logic: >-
      IF force is requested THEN the check evaluates the ForcePush rule branch — a check that ignores intent would deny-force or pass-force incorrectly vs real `git push --force` behavior.
    violation: >-
      intent-blind checks diverge from push parity.
    source: >-
      review-MINOR 7, protection/mod.rs:216-229
  - id: BR6.26
    statement: >-
      Same-oid export alias proceeds — two distinct changesets can mint the same git oid (identical tree+parents+idents+message); `link_insert` false with stored≠ours on EXPORT is a content-identical alias, not corruption — still write `linked:{ours}→oid` and proceed.
    category: >-
      calculation
    applies_to: >-
      export
    trigger: >-
      link_insert false-return during export chain write
    logic: >-
      IF the minted oid is already linked to a different changeset THEN the stored forward mapping stays with the first-minter; `linked:{ours}→oid` still upserts so both changesets resolve forward-consistent.
    violation: >-
      Internal on a benign alias; overwriting the stored link to point at the second changeset.
    source: >-
      review-MINOR 11
  - id: BR6.27
    statement: >-
      `heads[].changeset_id` is Optional — `link_lookup(tip)` may miss mid-import when independent tips exceed the page (`--topo-order` interleaves unrelated tips by commit-date); unresolved tips emit `null` — contract errata for u7 (`changeset_id|null` while unresolved, all resolved once `done`).
    category: >-
      constraint
    applies_to: >-
      import_chain
    trigger: >-
      ImportPage.heads emission
    logic: >-
      IF a tip's link lookup misses THEN the head row carries `null` — never Internal; on `done` every tip resolves.
    violation: >-
      non-optional field forced on a legitimately-unresolved tip; Internal on a normal pagination state.
    source: >-
      review-MAJOR 5, contract errata
  - id: BR6.28
    statement: >-
      `tree_flat` is cap-bounded AND tag targets peel honestly — `flat.len() > import_tree_max ⇒ TooLarge`; tips resolving to non-commit objects (tag→tree/blob) drop + `tracing::warn`, never abort the import.
    category: >-
      constraint
    applies_to: >-
      import_chain, import_one
    trigger: >-
      per-commit flat listing / tip peel
    logic: >-
      IF a commit's tree exceeds the leaf cap THEN TooLarge; IF a listed ref peels to a non-commit THEN it is excluded from tips (annotated tags pointing at commits DO resolve via `verify_commit_oid`'s `^{commit}` suffix).
    violation: >-
      unbounded flat materialization; erroring on a benign non-commit tag.
    source: >-
      review-MINOR 10
```

## Summary table

| Range | Theme | Key contracts |
|-------|-------|---------------|
| BR6.1–BR6.4 | write-path + convergence | `create` only; shared `import_one`; links + `linked:` reverse map |
| BR6.5–BR6.8 | order, claim, cursor, enumeration | topo invariant; nonce-verified claim/release; consumed-window `skip`+`last_oid`; parents-first `RevList`, honest single-cone bound |
| BR6.9–BR6.13 | conversion fidelity | first-parent diffs; `tree_flat` modes+sizes; gitlink skip; RAW ident sidecars; UTF-8 refusals |
| BR6.14–BR6.17 | export integrity | existence-probed `linked:` anchors; forward `link_lookup` ff gate; write→check→move; classified hooked move; empty-set fast-path |
| BR6.18–BR6.22 | errors, caps, actor, state, observability | total bridge incl. all consumed enums (real ChangesetError set); typed caps; actor threading; NULL-covering marker state; tracing |
| BR6.23–BR6.28 | refusals + plumbing + edge semantics | deterministic InvalidInput set; config-independent plumbing; force-intent check; same-oid alias; Option heads; flat cap + tag peel |
