# Entities — u6-vcs-git-interop

`GitInterop` owns **no new tables** — but does take one u2 amendment: migration 0055 makes `vcs_derived_index.covering_changeset` nullable (FK kept) so `active`/`cursor` marker rows can carry `NULL` before any changeset exists (review-BLOCKER — `""` violates `NOT NULL REFERENCES vcs_changesets(id)`); `VcsDerivedIndexUpsert.covering_changeset` becomes `Option<&str>` (u6-claimed files: 0055 ×3 dialects + `vcs.rs`). Its persisted footprint is otherwise: changeset rows written through `ChangesetStore::create` (u4 path, unchanged), `ObjectStore` blobs/links (u3), `vcs_import_links` rows (u2's entity, u6 behaviorally owns the insert/lookup protocol), and five derived-index keyspaces under `kind="vcs:import"` — including `linked:{cs}`, the reverse-map. New in-crate types live in `git_interop.rs` (NEW file, `server` feature); new backend-facing types live in `oxidean-git` where the trait is defined — u6 authors them there as part of the trait extension. *(Revision 2 — dev pass + architecture review pass-1 integrated.)*

## Owned in-crate types (`crates/oxidean-vcs/src/git_interop.rs`, `server` feature — NEW file only)

| Type | Shape | Notes |
|------|-------|-------|
| `GitInterop` | `{db: Database, objects: ObjectStore, store: ChangesetStore, index: ReadIndex, backend: Arc<dyn GitBackend>, checker: Arc<dyn ProtectionChecker>, caps: InteropCaps}` | Handles u7 already owns + two injected traits. `repo: &Path` per call — host derives paths. `ObjectStore` isn't `Clone`; the `ReadIndex` instance wraps a second one (dev 24). |
| `InteropCaps` | `{import_commit_cap: usize = 1000, rev_list_max: usize = 250_000, cursor_tips_max: usize = 512, import_tree_max: usize = 10_000, import_blob_max: u64, export_chain_max: usize = 10_000, export_blob_max: u64, export_ancestor_probe_max: usize = 250_000, import_lock_ttl_secs: i64 = 900}` | `*_blob_max` default `objects.max_object_bytes`; `validate` at `new` pins them ≤ that cap (u5 MergeCaps convention). `export_ancestor_probe_max` bounds the ff-gate BFS (dev 18b). |
| `ImportPage` | `{imported: u64, cursor: Option<String>, done: bool, heads: Vec<ImportHead>}` | `cursor` present iff `!done`; opaque b64url `CursorState`. |
| `ImportHead` | `{name: String, git_oid: String, changeset_id: Option<ChangesetId>}` | `git_oid` is the PEELED commit oid (tags peeled via `verify_commit_oid` — dev 4); non-commit peel targets drop+warn (BR6.28). `changeset_id` is `Option` — unresolved mid-import tips emit `null` (review-MAJOR 5; contract errata for u7). |
| `ExportOutcome` | `{branch: String, git_oid: String, commits_count: u64, before_oid: Option<String>}` | Carries u7's activity-row fields (`ref_name`, `before_oid`, `after_oid`, `commits_count`). |
| `InteropError` | `DbNotConfigured` \| `NotFound` \| `InvalidInput(String)` \| `ImportState(String)` \| `Forbidden{verdicts: Vec<ProtectionVerdict>}` \| `RefConflict{refname: String, actual: Option<String>}` \| `TooLarge` \| `QuotaExceeded(QuotaScope)` \| `Internal(String)` | Wire map per Q6. `field: reason` messages — never stderr/rejected-value echo. No `PartialEq` (u5 convention). `RefConflict.actual` `Option` covers the branch-create edge (matches `CasOutcome::Conflict` shape, dev 24). |
| `ProtectionChecker` (trait) | `async fn check(&self, repo: &Path, refname: &str, old: Option<&str>, new: &str, actor_capability: &str, force: bool) -> Result<Vec<ProtectionVerdict>, String>` | u6-owned port (ADR-006); needs `async-trait` in `server` deps (dev 22f). `force` selects the ForcePush rule branch — push-intent parity (review-MINOR 7). |
| `ProtectionVerdict` | `{rule: String, outcome: String, detail: String}` | `outcome` ∈ `{"pass","warn","deny"}`; any `deny` ⇒ `Forbidden`. Wire-verbatim into `data.verdicts`. |
| `CursorState` (internal) | `{v: u8 = 1, tips: Vec<TipSnapshot>, skip: u64, last_oid: Option<String>}` — `TipSnapshot{name,git_oid}` | Snapshot cursor — `last_oid` guards ordering drift on resume (dev 13); `tips` ≤ `cursor_tips_max` (dev 22d). |
| `IdentSidecar` (internal) | `{a_name_b64,a_email_b64,a_unix,a_tz, c_name_b64,c_email_b64,c_unix,c_tz}` | RAW ident bytes base64'd (dev 5 — `GitIdent` fields are `Vec<u8>`; sanitized/trimmed/non-UTF8 idents still re-export byte-exactly). Written AFTER `create` (dev 22e). |
| `ImportMarker` (internal) | `{state:"active"|"idle", nonce, started_at}` | Best-effort claim (BR6.6) — nonce-verified release; `covering_changeset = NULL` (post-0055 Option column). |
| `LinkedRow` (internal) | `{git_oid}` payload under `linked:{cs}` | Reverse map: changeset→git_oid via derived-index exact-key query — dev 1 resolution, no facade edit. |

## New backend-facing types + methods (authored by u6 inside `crates/oxidean-git` — trait extension)

| Type | Shape | Notes |
|------|-------|-------|
| `RawCommit` | `{tree: String, parents: Vec<String>, author: GitIdent, committer: GitIdent, message: Vec<u8>}` | `cat-file commit` parse — folded continuation headers skipped; last-` <`/`>` ident split; malformed ⇒ typed error (dev 19). |
| `GitIdent` | `{name: Vec<u8>, email: Vec<u8>, unix: i64, tz: String}` | Byte-tolerant (dev 5) — git permits arbitrary ident bytes; `write_commit` envs emit `<unix> <tz>` raw date form. |
| `NameStatusRow` | `{status: NameStatusKind, path: String, old_path: Option<String>}` | `NameStatusKind = Add|Modify|Delete|Rename|TypeChange`; `-z` R-rows consume two path records (dev 20); non-UTF8 path ⇒ typed parse error (dev 6). |
| `FlatTreeEntry` | `{mode: String, kind: TreeEntryKind, oid: String, path: String, size: u64}` | One `ls-tree -r -l -z --full-tree` per commit (dev 17) — mode+kind+size per leaf; non-UTF8 path ⇒ typed error. |
| `TreeWriteEntry` | `{mode: String (`{:06o}`), oid: String, path: String}` | `mktree -z` input — full snapshot semantics. |
| `RevList` | `{oids: Vec<String>, truncated: bool}` | `oids` = PARENTS-FIRST (backend reverses children-first `--topo-order` output in-crate — review-BLOCKER); `truncated` is data (dev 15). |
| Method signatures | `cat_commit(repo,sha)->RawCommit`; `commit_name_status(repo,base:Option<&str>,sha)->Vec<NameStatusRow>`; `tree_flat(repo,sha)->Vec<FlatTreeEntry>`; `rev_list(repo,tips:&[String],max)->RevList`; `verify_commit_oid(repo,rev)->Option<String>`; `check_refname(repo,refname)->bool`; `write_object(repo,bytes)->oid`; `write_commit(repo,entries,parents,author,committer,message)->oid` | See Q2 table for the plumbing argv per method — all pins config-independent (BR6.24). |
| Modified signatures | `fast_forward_ref(repo, refname, sha, actor_capability)`; `force_update_ref(repo, refname, sha, actor_capability)` | New `actor_capability` param (replaces hardcoded `"admin"`) + ported `classify_push_error` stderr remap — shared helper used by these AND `commit_files` (dev BLOCKER 3). Four production call sites pass `"admin"` (`mirror/engine.rs:536`,`:863`,`:991`, `repo/sync_fork.rs`). |

## Persisted keyspaces (derived-index `kind="vcs:import"`)

| `index_key` | Payload | `covering_changeset` | Lifecycle |
|-------------|---------|----------------------|-----------|
| `active` | `ImportMarker` JSON | `NULL` | Claim/release — best-effort mutex (BR6.6). |
| `cursor` | `CursorState` JSON | last imported cs or `NULL` | Resumable position + `last_oid` continuity. |
| `ident:{git_oid}` | `IdentSidecar` JSON | the imported cs | Forward lookup/diagnostics. |
| `ident-cs:{cs}` | `IdentSidecar` JSON | the imported cs | Export's O(1) byte-exact ident restore. |
| `linked:{cs}` | `{git_oid}` JSON | the linked cs | Reverse map — export anchors + ff gate (dev 1). |

## Consumed entities (NOT owned)

| Entity | Owner | Use |
|--------|-------|-----|
| `vcs_import_links` | u2 (DDL) | `insert_if_absent` + `lookup` — THE convergence mechanism; u6 owns the protocol, u2 the table. `idx_vcs_import_links_changeset` exists but stays unused (the derived `linked:` row IS the reverse map — a real reverse facade is a recorded v2 option). |
| `vcs_changesets` + `parents` | u2/u4 | `row_probe`, `vcs_changeset_parents` BFS (chain walk, ff gate, local `is_ancestor_cs` — u5's is private, dev 18d), `store.create` write path. |
| `vcs_file_changes` | u2 | `vcs_list_file_changes(cs)` — per-node delta (not used for export trees — `index.tree` per node is the correct fold, dev 2). |
| `vcs_derived_index` | u2/u4 | upsert/query only — the five keyspaces above; `covering_changeset` nullability amended by u6 (migration 0055, `Option<&str>` signature). |
| object blobs | u3 | `put` (import, streamed), `get` → `File` with `take(export_blob_max+1)` (export, dev 22b). |
| `ReadIndex::tree` | u4 | per-node export trees — u4's `materialize` does the correct first-parent fold (dev 2). |
| `GitBackend` (existing) | oxidean-git | `rev_parse`(avoid — see `verify_commit_oid`), `list_refs`, `cat_blob`, `fast_forward_ref`/`force_update_ref` (extended), `update_ref`/`commit_files`/`push_to_url*`(FORBIDDEN). |

## Rejected entity candidates

- **`vcs_lookup_import_link_by_changeset` facade** — rejected: the `linked:{cs}` derived row delivers the reverse map with zero `vcs.rs` churn (dev 1 resolution); the schema index stays available for a v2 promotion.
- **`vcs_import_state` table** — rejected: derived-index upsert+query+timestamp suffices (BR6.21).
- **`""` covering sentinel** — rejected: FK violation in all three dialects; the 0055 nullability amendment is the honest mechanism (review-BLOCKER).
- **Single-map fold-forward export tree** — rejected: provably wrong on DAGs (`tree(C)=tree(parents[0])+changes(C)`, dev 2); per-node `index.tree` is correct-by-construction.
- **`SubmodulePointer` entity** — rejected: no LEGAL_MODES slot; skipped+warned (BR6.11).
- **In-memory frontier / pending queue** — rejected: snapshot+skip cursor over `RevList` (BR6.7).
- **`ExportLink`/branch-tip state rows** — rejected: `linked:` + `verify_commit_oid` derive it.
