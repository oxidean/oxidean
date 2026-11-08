# Logical Components — u6-vcs-git-interop

## LC6.1 — Crate layout delta

```
crates/oxidean-vcs/
  Cargo.toml              — server feature gains `oxidean-git` + `async-trait`
  src/lib.rs              — server block gains `pub mod git_interop`
  src/git_interop.rs      — GitInterop, InteropCaps, ImportPage, ImportHead,
                            ExportOutcome, InteropError, CursorState,
                            TipSnapshot, ImportMarker, IdentSidecar,
                            ProtectionChecker (trait), + in-file tests
crates/oxidean-git/
  src/backend.rs          — GitBackend trait +8 methods (TD6.2 authoritative sigs)
  src/cli.rs              — impls + classify_push_error shared helper +
                            run_git_stdout_bounded reader + actor_capability
                            params on both move methods + call-site updates
crates/oxidean-db/
  migrations/{postgres,mysql,sqlite}/0055_vcs_derived_index_nullable_covering.sql
  src/vcs.rs              — covering_changeset: Option on BOTH struct sides
                            (u2-claimed — recorded deviation)
crates/oxidean-core/
  src/vcs_types.rs        — VcsGitImportHead.changeset_id: Option<ChangesetId>;
                            VcsGitImportResponse.skipped_tips: u64
                            (u1-claimed — recorded deviation, wire-superset)
```

No new tables, no new index kinds (`kind="vcs:import"` rides the existing derived-index table), no edits outside the claimed set.

## LC6.2 — `GitInterop`

- `new(db: Database, objects: ObjectStore, store: ChangesetStore, index: ReadIndex, backend: Arc<dyn GitBackend>, checker: Arc<dyn ProtectionChecker>, caps: InteropCaps) -> Result<Self, InteropError>` — `caps.validate(objects)` pins `import_blob_max ≤ objects.config.max_object_bytes`, `export_blob_max ≤ same`, and `*_max ≤ usize::MAX / 2048` (bounds the LARGEST multiplier — identical rule to SD6.1) → `InvalidInput` on host misconfig.
- `import_chain(repo_id: &str, repo: &Path, from: Option<&str>, cursor: Option<&str>, limit: Option<usize>) -> Result<ImportPage, InteropError>` — the WF1 ordered pipeline (SD6.3): `is_skipped` → limit `1..=import_commit_cap` → dash-led `from` reject → nonce claim → tips (head-preferred `verify_commit_oid` → `list_refs` peel+drop+warn) → cursor decode (SD6.2) → `rev_list` → parents-first window with per-oid `link_lookup` early-skip → `import_one` + `link_insert` + `linked:` row → cursor upsert (nonce re-verify, byte-cap pre-check, `covering_changeset` = cumulative last-imported) → nonce-verified release → `ImportPage{imported, cursor iff !done, done, skipped_tips, heads}`.
- `lazy_import(repo_id: &str, repo: &Path, git_oid: &str) -> Result<ImportPage, InteropError>` — WF2: oid-shape check `InvalidInput` → `link_lookup` early `done:true` → `verify_commit_oid` `None` ⇒ `NotFound` → LOOP `import_chain` seeded `tips=[oid]` DIRECTLY (bypasses `from` resolution — review3-F7) → per-page `link_lookup(oid)` break; `page.done ∧ unlinked ⇒ Internal`; cursor carries.
- `export(repo_id: &str, repo: &Path, head: &str, branch: &str, force: bool, actor_capability: &str) -> Result<ExportOutcome, InteropError>` — WF4 param order verbatim: `check_refname` → capped BFS over `vcs_changeset_parents` with `linked:`-anchor existence probes (`verify_commit_oid`; gc'd ⇒ treat unlinked) → empty-set fast-path → SD6.5 two-phase gate → write chain → `checker.check` → ONE classified move → `ExportOutcome{branch, git_oid, commits_count, before_oid}`.
- `export` NEVER calls `update_ref`/`commit_files`/`push_to_url`; import never moves refs at all.

## LC6.3 — `ProtectionChecker` + verdict flow

```rust
#[async_trait]
pub trait ProtectionChecker: Send + Sync {
    async fn check(&self, repo: &Path, refname: &str, old: Option<&str>,
                   new: &str, actor_capability: &str)
        -> Result<ProtectionBlockReasons, String>;
}
```

`oxidean-vcs` defines it; `oxidean-api` supplies the impl (u7 wiring); u6 tests use a mock. Empty `reasons` ⇒ pass; non-empty ⇒ `Forbidden{verdicts}` verbatim via u1's `forbidden(message, verdicts)` (vcs_types.rs:394). NO force param — the impl derives intent `f(old,new)` like `check_ref_update` (protection/mod.rs:842-856). `Err(_)` ⇒ `Internal` opaque — quota/`refs/pull` denials express only through `reasons` (devsecops-12).

## LC6.4 — Internal primitives (private to git_interop.rs)

- `claim(repo_id) -> Result<String /*nonce*/, InteropError>` — upsert `active{state:active,nonce,started_at}` (`covering=None`) → re-read → foreign active ⇒ `ImportState`; TTL `import_lock_ttl_secs` stale reclaim. `nonce` mints from `sha2` over `{repo_id, started_at, u64::from(RandomState)}` — the process-random source is `std::collections::hash_map::RandomState` (std-only, no `rand`/`uuid` dep; hex-encoded digest as the nonce string).
- `release(repo_id, nonce, ok)` — re-read; nonce-match ⇒ upsert `{state: ok ? "idle" : "failed"}`; mismatch ⇒ skip (another importer owns it) — never an error. `ok` is terminal-state observability only; it never changes release semantics.
- `decode_cursor(&str) -> Result<CursorState, InteropError>` — SD6.2's five-step pipeline.
- `encode_cursor(&CursorState) -> Result<String, InteropError>` — serde+b64url; the `cursor_payload_max_bytes` check applies to the EMITTED opaque string (b64url output, matching the inbound `cursor.len()` check — not the inner JSON, which is ~¾ the emitted size; a JSON-side cap would mint exactly the unresumable cursor it exists to prevent).
- `link_lookup(repo_id, oid) / linked_lookup(repo_id, cs)` — `vcs_lookup_import_link` forward (`Option<String>` cs id) / derived `linked:{cs}` row reverse.
- `ident_sidecar_read(repo_id, cs) -> Result<Option<IdentSidecar>, InteropError>` — named read-back primitive for export: `vcs_query_derived_index(repo_id, "vcs:import", Some("ident-cs:{cs}"), None, 1)` first-row decode (vcs.rs:2520; support-F9) — `Some` ⇒ decode (malformed row ⇒ `Internal`); `None` ⇒ caller applies WF5.3's synthesized fallback (NEVER silently re-mints idents for a corrupt row, but absence is expected for every native changeset).
- `resolve_tips(repo, from) -> Result<Vec<TipSnapshot>, InteropError>` — head→tag→raw `verify_commit_oid` tries; `None` path = `list_refs` heads+tags peeled, non-commit peel ⇒ drop + `warn!` + `skipped_tips += 1`; `> cursor_tips_max` ⇒ `TooLarge`.
- `import_one(repo_id, repo, oid) -> Result<ChangesetId, InteropError>` — WF3: `cat_commit` → parents `link_lookup` (miss ⇒ `Internal`) → `commit_name_status(base)` + `tree_flat(sha)` (+parent flat lazily for D/R) → `FileChangeInput`s (gitlink skip+warn, `size > import_blob_max` ⇒ `TooLarge` pre-read, `cat_blob` → `objects.put` streamed) → refusal inventory (BR6.23 + mirrored `compose_author` guards: sanitize {control,0x7f}→'?' FIRST then refuse `<`/`>`/empty/composed>255) → `store.create` → `ident` sidecars (`Some(cs)` covering; payload > `cursor_payload_max_bytes` ⇒ `InvalidInput` pre-create) → id.
- `is_ancestor_cs(repo_id, anc, desc) -> Result<bool, InteropError>` — local BFS over `vcs_changeset_parents`, `visited ≤ export_ancestor_probe_max`; `anc == desc` ⇒ true (mirrored — u5's is private).
- `export_one(repo_id, repo, cs) -> Result<String /*oid*/, InteropError>` — WF5: parents → `linked_lookup` (miss ⇒ `Internal`) → base git tree = `tree_flat(repo, parents[0]_git_oid, import_tree_max × 1024)` (the parent's GIT tree supplies unchanged-entry blob oids — `parents[0]` matches the import diff-base convention; merge-parents beyond `[0]` contribute no diff base) → cs manifest diff MIRRORED over `index.tree` pages with cap-during-accumulation (accumulated ≤ `import_tree_max`) → changed/new entries resolve VCS-blob→git-oid via seen-map then `objects.get(take(export_blob_max+1))` → `write_object`; unchanged entries reuse the base-flat oid → `mktree -z`/`commit-tree` via `write_commit` (idents: `ident_sidecar_read` hit ⇒ raw bytes; miss ⇒ WF5.3 fallback `author→both, committer.date=committed_at` unix UTC; message = cs message verbatim) → `link_insert` + `linked:` row → oid.
- `walk_export_set(repo_id, head) -> Result<Vec<ChangesetId>, InteropError>` — BFS `vcs_changeset_parents` from head, `linked:`-anchor existence-probed (gc'd ⇒ unlinked), `≤ export_chain_max`, parents-first output.
- `sanitize_ident(bytes) / refuse_ident(name, email)` — mirrored `compose_author` guards (changeset_store.rs:439-461 private — mirrored, not called).
- `mirror_validate_path` — NOT needed: `validate_path` (changeset_store.rs:241) is `pub(crate)` — called directly.
- `is_git_oid(&str) -> bool` — `len==40 && all-hex` (local — the store's `is_git_oid` IS `pub(crate)` and reachable, but accepts `40|64`-hex for sha256 probes (changeset_store.rs:204-208); the lazy path needs 40-only git oids — the length difference is the real justification, support-F7).

## LC6.5 — Consumed surfaces (no new surface)

Facade: `vcs_lookup_import_link(repo_id, git_oid) -> Result<Option<String>, String>` (vcs.rs:2550 — returns the cs id directly, NOT a row type), `vcs_insert_import_link_if_absent(repo_id, git_oid, changeset_id) -> Result<bool, String>` (:2540), `vcs_query_derived_index(repo_id, kind, index_key: Option, cursor, limit) -> Vec<VcsDerivedIndexRow>` (:2520), `vcs_upsert_derived_index_rows(repo_id, &[VcsDerivedIndexUpsert])` (:2511 — `covering_changeset: Option<&str>` post-0055), `vcs_get_changeset`, `vcs_changeset_parents`, `Database::is_skipped` (`vcs_list_file_changes` NOT consumed — per-node export deltas fold `index.tree`, not file_changes rows; entities.md:51) (`vcs_import_link_page` :2559 is NOT consumed — Q9 exclusion). Stores: `ChangesetStore::create` (+`CreateChangesetInput`/`FileChangeInput`/`ChangeKind`/`ChangesetError`), `ReadIndex::tree` (+`IndexError`), `ObjectStore::{put,get}` (+`ObjectStoreError`/`QuotaScope`). Crate-internal: `changeset_store::validate_path` (`pub(crate)`), `oxidean_core::protection_types::ProtectionBlockReasons`, `oxidean_core::vcs_types::{ChangesetId, VcsRefConflictData, VcsErrorCode}`. Backend: `GitBackend` TD6.2 surface + existing `is_ancestor`/`list_refs`/`cat_blob`. New deps: `oxidean-git`, `async-trait` — nothing else (verified Cargo.toml: `base64`/`serde`/`serde_json`/`sha2`/`hex`/`bytes`/`tokio`/`tracing`/`futures-util`/`chrono`/`oxidean-core`/`oxidean-db` are ALL already `server` deps; `sha2` mints the nonce).
- **Zero edits** to u1–u5 files outside the recorded deviations (vcs.rs read/write option + vcs_types.rs two-field amendment).

## LC6.6 — Wire contract surface (u7 consumes)

Variants → codes: `DbNotConfigured`→`db.not_configured`; `NotFound`→`vcs.not_found`; `InvalidInput(m)`→`vcs.invalid_input`+m; `ImportState(m)`→`vcs.import_state`+m (variant EXISTS — vcs_types.rs:331/354/1101, resolved); `Forbidden{verdicts}`→`vcs.forbidden`+`VcsForbiddenData{verdicts}`; `RefConflict{refname,actual}`→`vcs.ref_conflict`+`VcsRefConflictData{actual:Option<ChangesetId>}`; `TooLarge`→`vcs.too_large`; `QuotaExceeded`→`vcs.quota_exceeded`; `Internal`→`rpc.internal` (Q7's wire pin verbatim — `internal` shorthand corrected; matches app.rs:838). `ImportPage`/`ImportHead`/`ExportOutcome` map to the amended DTOs (TD6.5). Activity-row fields carried on `ExportOutcome` (`ref_name`,`before_oid`,`after_oid`,`commits_count`).

## LC6.7 — Superseded/added pins vs FD (codegen: read these over stale FD lines)

- FD signature table: `tree_flat(repo,sha)`/`commit_name_status(repo,base,sha)` → the TD6.2 full arities with `max_bytes` (review-F1).
- `rev_list`/`cat_commit` bound stdout INTERNALLY (`rev_list` derives `max × 41` from its own `max` param; `cat_commit` uses a private `CAT_COMMIT_MAX` const = `import_blob_max`-class bound) — no signature change; only `tree_flat`/`commit_name_status` carry caller `max_bytes` (their bound derives from the caller's `import_tree_max`).
- Cursor decode is the five-step SD6.2 pipeline (new pins: inbound byte cap, field revalidation, `skip>len`, decoded-tip oid-shape).
- `lazy_import` seeds `tips=[oid]` directly — `resolve_tips` is bypassed on the lazy path.
- Ident ordering: sanitize-then-refuse ONCE (BR6.13 order wins over BR6.23's earlier wording — the sanitized result is what `<`/`>`/empty check).
- `vcs.import_state` code EXISTS (vcs_types.rs:331/354/1101) — no amendment needed.
- `IndexError` bridge supersedes FD-Q8's `CursorMalformed→ImportState`: u5 parity — `CursorMalformed→Internal` (index cursors are interop-minted paging tokens, never caller input), `FrontierTooLarge→TooLarge`, `DbNotConfigured→DbNotConfigured`, rest→`Internal` (merge_engine.rs:126-131 convention).
- `ident-cs:` sidecar ABSENCE is the WF5.3 fallback path (`author→both`), NOT `Internal` — supersedes the earlier "missing ⇒ `Internal`" wording; `Internal` only for a present-but-malformed row.
- `run_git_stdout_bounded` overflow types `GitError::Process` with the fixed sentinel `oxidean:stdout-bound-exceeded` — the interop `GitError` arm maps sentinel-matching `Process` → `TooLarge`, all other `Process`/`Io` → `Internal` (a real bound trip must surface `TooLarge` per SD6.6/SR6.1).
