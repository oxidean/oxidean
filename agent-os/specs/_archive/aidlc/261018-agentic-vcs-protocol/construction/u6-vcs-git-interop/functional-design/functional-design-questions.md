# Functional Design Questions — u6-vcs-git-interop

Unit: `u6-vcs-git-interop` — `GitInterop` inside `oxidean-vcs`'s `server` feature. Consumes `Arc<dyn GitBackend>` + `Arc<dyn ProtectionChecker>` by injection (ADR-006 — u7 wires them), `Database` facade import-link + derived-index + changeset probes, `ChangesetStore::create` (single changeset write path), `ObjectStore::{put,get}`, `ReadIndex::tree` (export trees). Branch moves go only through `fast_forward_ref`/`force_update_ref` (hook-driven) — never bare `update_ref`. *(Revision 3 — architecture review pass-2 integrated: `ProtectionBlockReasons` replaces the invented verdict struct, intent is derived `(old,new)`-side not `force`-flagged, git-side ff fallback added, lazy pagination loop made explicit, cursor byte cap, complete consumed-enum bridge.)*

## Q1 — Boundary, constructor, and feature gating

**Decision:** `GitInterop` is a `server`-feature-only struct in a NEW file `crates/oxidean-vcs/src/git_interop.rs` (no u4/u5 file edits — same fingerprint discipline as u5):

```rust
pub struct GitInterop {
    db: Database,
    objects: ObjectStore,
    store: ChangesetStore,
    index: ReadIndex,
    backend: Arc<dyn GitBackend>,
    checker: Arc<dyn ProtectionChecker>,
    caps: InteropCaps,
}
```

- `repo_id` is the DB scope (facade calls); the bare repo `&Path` is a per-call argument — path derivation is the host's (u7 knows `repos_dir`), keeping `oxidean-vcs` free of host layout policy.
- New dependencies under `server` only: `oxidean-git` (trait + types — never `CliGitBackend` concretely) and `async-trait` (for `ProtectionChecker`). The `client` feature never sees them (ADR-011). `ObjectStore` isn't `Clone` — the `ReadIndex` it wraps needs a second instance; u7 constructs both.
- `ProtectionChecker` is defined IN-CRATE (u6 owns the trait port); `oxidean-api` supplies the real impl at dispatch. The verdict shape is the REAL structured payload `ProtectionBlockReasons` (`oxidean-core/protection_types.rs:265` — `reasons`, `required_approving_review_count`, `approving_review_count`, `missing_status_contexts`, `unsigned_commits`) — the only shape that can produce the wire's `forbidden{verdicts}` verbatim (review-BLOCKER: an invented `ProtectionVerdict{rule,outcome,detail}` cannot). **No `force` param** — `check_ref_update` (protection/mod.rs:842-856) derives intent from `(old,new)` itself: `new==zeros`→Delete, `old` absent/zeros→Push, else `git_is_fast_forward`→Push|ForcePush. Passing the caller's force flag would eval ForcePush on a move that is actually ff — backwards: `allow_force_pushes` should bite only when the move genuinely isn't (F2). `force` lives only at the interop ff gate:

```rust
#[async_trait]
pub trait ProtectionChecker: Send + Sync {
    /// Same verdict computation a `git push` of `{old → new}` onto `refname`
    /// would face — intent derived from (old,new), hook-parity.
    /// Empty `reasons` = pass.
    async fn check(&self, repo: &Path, refname: &str,
                   old: Option<&str>, new: &str,
                   actor_capability: &str) -> Result<ProtectionBlockReasons, String>;
}
```

- `actor_capability` threads from the wire (u7 supplies the session capability) so the verdict matches what a push by that actor would get — `OXIDEAN_ACTOR_CAPABILITY` is what the hook reads (cli.rs:253-278).

## Q2 — `GitBackend` surface extension (new methods authored by u6 inside `oxidean-git`)

**Decision:** the trait lacks raw commit reads, topo enumeration, flat-tree listings, refname validation, existence typing, and object-plumbing writes. u6 adds exactly these (trait + `CliGitBackend` impl + tests):

| Method | Shape | Backing plumbing |
|--------|-------|------------------|
| `cat_commit` | `(repo, sha) -> RawCommit{tree, parents: Vec<String>, author: GitIdent, committer: GitIdent, message: Vec<u8>}` | `git cat-file commit <sha>` — raw header parse: `tree`, `parent`×n, `author`/`committer` parsed byte-level (last-` <`/`>` split, trailing `unix ±ZZZZ`); folded continuation headers (`gpgsig`, `mergetag`, `encoding` — space-led lines continue the previous header) skipped; malformed ident ⇒ typed parse error → `InvalidInput`, never panic (dev 19). `message` = bytes after first blank line, verbatim. |
| `commit_name_status` | `(repo, base: Option<&str>, sha) -> Vec<NameStatusRow>` | `git -c diff.renameLimit=10000 diff-tree -r --name-status -M -z` — `-c` is a TOP-LEVEL git option positioned before the subcommand (after it, git treats it as a diff-tree flag and dies; F-nit). Two-tree `<base> <sha>` for normal commits; `--root --no-commit-id` single-arg for roots (dev 10). `-M` default similarity; `diff.renameLimit` pinned config-independent (dev 8). `-z`: status records NUL-terminated; `R<score>` rows carry TWO path records consumed together. Paths byte-parsed → UTF-8 ⇒ else typed error → `InvalidInput` (dev 6). |
| `tree_flat` | `(repo, sha) -> Vec<FlatTreeEntry>` | `git ls-tree -r -l -z --full-tree <sha>` — one subprocess → every leaf `{mode,kind,oid,path,size}`; enables pre-read blob-cap checks (dev 17). |
| `rev_list` | `(repo, tips: &[String], max: usize) -> Result<RevList, GitError>` | `git rev-list --topo-order --max-count=max+1 <tips…>` — the emitted set (children-first) is REVERSED in-crate → `RevList.oids` is PARENTS-FIRST (dev BLOCKER 2; `--reverse`+`--max-count` would truncate the wrong end). `truncated` when `len > max`. `tips.is_empty()` ⇒ `Ok(RevList{[],false})` (dev 15). |
| ~~`is_ancestor_oid`~~ — REUSE | `is_ancestor(repo, maybe_ancestor, tip) -> Result<bool, GitError>` — ALREADY EXISTS | `backend.rs:735-740` declares it; `cli.rs:3332-3368` already runs `git merge-base --is-ancestor` with the identical `0`⇒true/`1`⇒false/else⇒`Process` exit typing — the invented duplicate is dropped; u6 consumes the existing method as the ff gate's git-side fallback (review3-F1). Resolves twin-alias holders and unlinked-but-reachable tips the changeset-side test can't (F7). |
| `verify_commit_oid` | `(repo, rev) -> Result<Option<String>, GitError>` | `git rev-parse --verify --quiet {rev}^{commit}` — resolves refs/oids/annotated tags to a peeled COMMIT oid or `None` on miss (dev 4+11: bare `rev_parse` never types NotFound and returns tag-object oids). Used for `from`, tip peel, `old` branch tip, lazy oid, AND anchor existence probes (dev 6-major). Dash-led `rev` rejected caller-side as `InvalidInput` (nit f). |
| `check_refname` | `(repo, refname) -> Result<bool, GitError>` | `git check-ref-format <refname>` — exact parity (`validate_treeish` accepts `a//b`, `x.lock`, `a@{b}`, `a[b`, `foo.` — dev 16). |
| `write_object` | `(repo, bytes: &[u8]) -> Result<String, GitError>` | `git hash-object -w --stdin`. |
| `write_commit` | `(repo, entries: &[TreeWriteEntry], parents: &[String], author: &GitIdent, committer: &GitIdent, message: &[u8]) -> Result<String, GitError>` | `git -c commit.gpgSign=false mktree -z` recursion (`mode SP blob SP oid TAB name`, `{:06o}` modes, raw names — non-`-z` C-unquotes `"`/`\` paths, dev 9; mktree self-sorts) then `git -c commit.gpgSign=false commit-tree <tree> -p …` with all six `GIT_AUTHOR_*`/`GIT_COMMITTER_*` envs (date `<unix> <±ZZZZ>`) and message via STDIN (`run_git_stdout_env_stdin`, cli.rs:337 — argv `-m` spawn-fails on NUL, dev 7). Deterministic mint. |
| `GitIdent` (type) | `{name: Vec<u8>, email: Vec<u8>, unix: i64, tz: String}` | Byte-tolerant — git permits arbitrary ident bytes (dev 5). |
| `FlatTreeEntry` (type) | `{mode: String, kind: TreeEntryKind, oid: String, path: String, size: u64}` | Byte-parsed path → UTF-8 validated. |
| `RevList` (type) | `{oids: Vec<String>, truncated: bool}` | `oids` = parents-first order. |

- `fast_forward_ref`/`force_update_ref` gain `actor_capability: &str` AND a ported push-stderr classifier — today both return raw `GitError::Process` for non-ff rejections AND hook declines (the `Process→Conflict`/`Denied` remap exists only inside `commit_files`, cli.rs:1735–1748 — dev BLOCKER 3). u6 extracts it into a shared `classify_push_error(msg)` in cli.rs used by all three: `non-fast-forward`/`fetch first`/`stale info` ⇒ `Conflict`; `hook declined`/`protected` ⇒ `Denied(msg)`; else `Process`. The FOUR production call sites (`mirror/engine.rs:536`, `:863`, `:991`, `repo/sync_fork.rs`) pass `"admin"` — system ops, behavior identical (dev 22a).
- Nothing else touches `oxidean-git`: `update_ref` stays un-called by interop.

## Q3 — u2 file amendment (review BLOCKER 1 — `covering_changeset` is FK'd NOT NULL)

**Decision:** `vcs_derived_index.covering_changeset` is `NOT NULL REFERENCES vcs_changesets(id)` in all three dialects — `""`/placeholder values violate the FK, and the `active` marker must exist before any changeset on a fresh repo. The sanction-free store alternatives all fail (`vcs_refs` pollutes `ref_list`; `vcs_import_links` keys on git_oid shape; `RepoSurfaceFlag` has no payload column). So u6 takes a small u2 amendment — same pattern as u4's bigint deviation:

- **Migration 0055** `vcs_derived_index_nullable_covering.sql` ×3 dialects (filename parity): postgres `ALTER TABLE vcs_derived_index ALTER COLUMN covering_changeset DROP NOT NULL`; mysql `MODIFY covering_changeset VARCHAR(32) NULL` (the column is `VARCHAR(32)` in mysql/0053:78 — `CHAR(29)` is postgres's type; F-nit); sqlite table-rebuild (ALTER can't drop NOT NULL) preserving the FK + `ON DELETE CASCADE`.
- **`vcs.rs` signature change:** `VcsDerivedIndexUpsert.covering_changeset: Option<&str>` (was `&'a str`); existing callers (`read_index.rs` etc.) wrap `Some(...)`; `.bind(u.covering_changeset)` passes NULL for state rows.
- **`vcs.rs` READ-side change (review3-F2):** `VcsDerivedIndexRow.covering_changeset: String` → `Option<String>` (vcs.rs:100) and the `map_index_row!` `try_get` (vcs.rs:281-283) decodes `Option` — today a NULL-covering `active`/`cursor` row would fail every `vcs_query_derived_index` decode, including the nonce re-read and cursor-resume paths this design mandates. All current row consumers unwrap-or-default.
- u2-claimed files land in u6's manifest — u2's CG binding re-verifies after U6 CG (planned cost, same as u4's `vcs.rs` cast deviation).
- `linked:`/`ident:` rows still pass `Some(cs_id)` — only `active`/`cursor` marker rows pass `None`.

## Q4 — Import algorithm (explicit `vcs.git_import` — the shared core)

**Decision:** `import_chain(repo_id, repo, from, cursor, limit) -> ImportPage`; `lazy_import` is a specialization, not a parallel implementation.

1. `db.is_skipped()` → `DbNotConfigured` first; `limit` → `1..=import_commit_cap` (1000; over ⇒ `InvalidInput`, absent ⇒ 1000). `from`+`cursor` co-supplied ⇒ cursor is authoritative, `from` ignored (nit a — documented).
2. **Claim** — derived `key="active"` `{state,nonce,started_at}` `covering_changeset=None` (post-0055): fresh `active` ⇒ `ImportState`; else upsert + RE-READ nonce-verify ⇒ `ImportState` on mismatch. Release = `{idle}` upsert ONLY after nonce re-verify (skip when marker holds another nonce — dev 9-minor). BEST-EFFORT mutex (no CAS — dev 12); BR6.4 convergence is the correctness backstop.
3. **Tips** — `from` ⇒ head-preferred (dev 21): bare short name ⇒ `verify_commit_oid("refs/heads/{s}")` → `refs/tags/{s}` → raw rev; all `None` ⇒ `NotFound`; dash-led `from` ⇒ `InvalidInput` (nit f). Absent ⇒ `list_refs` (heads+tags only, cli.rs:1907) each peeled via `verify_commit_oid` — peel returning `None` (tag→tree/blob) ⇒ drop + `tracing::warn` (dev 10). `tips.len() > cursor_tips_max` (512) ⇒ `TooLarge`.
4. **Cursor** — `{v:1, tips:[{name,git_oid}], skip, last_oid}`: malformed ⇒ `ImportState`. `TipSnapshot.name` = resolved ref shortname or the literal `from` (nit b).
5. **Enumerate** — `rev_list(tips, rev_list_max)` → `truncated` ⇒ `TooLarge` — honest bound: a >250k single ancestor-cone cannot import (nit e — `rev_list` can't express `^oid` exclusions); multi-cone repos import per-ref via `from`. `skip>0` ⇒ continuity `list[skip-1] == last_oid` else `ImportState` (dev 13).
6. **Process** `list[skip .. skip+window]` in parents-first order — `window = min(limit, remaining)`: per oid `link_lookup` EARLY-SKIP (dev 14); unlinked ⇒ `import_one` + `link_insert` + `linked:` row. Counters: `skip += consumed window`; `last_oid = list[new_skip-1]`; `out.imported` = FRESH conversions only (dev-major 4).
7. **State** — upsert `key="cursor"` (nonce re-verified; `covering_changeset` = CUMULATIVE last-imported cs — stays `None` on a zero-import page, not overwritten to `None` from a stale in-memory value; serialized `CursorState` bytes > `cursor_payload_max_bytes` ⇒ `TooLarge`, never write an unresumable cursor — mysql `payload TEXT` = 64KiB, mysql/0053:77); `release`; return `{imported, cursor: Some iff !done, done, skipped_tips, heads}` — `skipped_tips` counts non-commit-peel drops — surfacing it on the wire requires the `VcsGitImportResponse.skipped_tips` DTO amendment (vcs_types.rs:901-908 has no such field — review3-F3, recorded in Q9); `heads[i].changeset_id = Option` from `link_lookup(tip)` (dev-major 5: unresolved tips emit `null` — requires widening `VcsGitImportHead.changeset_id` to `Option<ChangesetId>`, vcs_types.rs:884 — the second recorded u1/u7 DTO amendment).
8. Errors ⇒ `release` still runs (nonce-verified); durable `skip` makes replay idempotent.

**`lazy_import` is an explicit bounded LOOP** (F4 — the earlier prose said "exits when the target oid is linked" without saying how): entry `is_git_oid`-validates + `link_lookup(oid)` early-`{done:true}`; `verify_commit_oid` ⇒ `None` ⇒ `NotFound`; then `import_chain(tips=[oid], cursor=carry, limit=cap)` — the lazy path seeds `tips` with the verified oid DIRECTLY, bypassing the heads→tags→raw `from` resolution (a branch literally named a 40-hex oid would otherwise shadow the target and import the wrong cone — review3-F7) — after each page `link_lookup(oid)` hit ⇒ break `{done:true,cursor:None}`; `page.done` ∧ target still unlinked ⇒ `Internal` (enumeration drift — target inside the tip cone but never converted); `page.cursor` carries forward. A target deeper than one `import_commit_cap` window can never silently report done.

## Q5 — Per-commit conversion (`import_one` — the AC7.1.2 convergence pin)

1. `cat_commit` → `RawCommit`.
2. Parents → `link_lookup` each in stored order; miss ⇒ `Internal` (topo invariant — parents-first enumeration makes misses impossible on a healthy list).
3. `rows = commit_name_status(...)`; `flat = tree_flat(sha)` (`flat.len() > import_tree_max` ⇒ `TooLarge` — dev 10-minor); parent-side `tree_flat` lazily only when `D`/`R` rows exist.
4. Per row → `FileChange` (`A`⇒add, `M`/`T`⇒modify, `D`⇒delete{parent mode}, `R`⇒rename{new,old,mode,blob}); either-side gitlink ⇒ skip+warn; `size > import_blob_max` ⇒ `TooLarge` pre-read; `cat_blob` → `objects.put` → VCS sha256.
5. `message` → UTF-8 fail ⇒ `InvalidInput`; refusal inventory (BR6.23): `>64` parents, `>65535`-byte message, unrepresentable `rfc3339(a.unix)` (nit c — `from_timestamp` `None` ⇒ `InvalidInput`, no panic), `>512`-char paths, `.`-segments/control-byte paths (`validate_path`:241-266 — `pub(crate)`, called directly), and the IDENTITY ordering pinned once (review3-F4): author name/email sanitize {control,0x7f}→`'?'` FIRST, then refuse `<`/`>` in the sanitized part OR empty post-sanitize, AND composed `Name <email>` > `MAX_AUTHOR_LEN` (255) — `compose_author`:439-461 is the reference set but is a PRIVATE fn — import mirrors the same guards in-crate pre-`create` (no u4 edit; review3-F6) so a store-side refusal can never surface mid-batch as an untyped error (F-major).
6. `store.create({parents, author_name: sanitize(a.name_utf8), author_email: a.email_utf8, message, committed_at: rfc3339(a.unix)})`.
7. `ident` sidecars AFTER `create` — `ident:{git_oid}` + `ident-cs:{cs}` raw-byte payloads (BR6.12).
8. Return id — `link_insert` + `linked:` are the caller's step.

## Q6 — Export algorithm (`vcs.git_export`)

**Decision:** `export(repo_id, repo, head, branch, force, actor) -> ExportOutcome` — ordering is **write-chain → check → move** (the checker needs the minted tip to evaluate the real `(old,new)` pair — review's reordering of my earlier pre-write check; denial leaves only unreferenced, gc-reaped objects).

1. `is_skipped`/`parse`/`row_probe`; `check_refname("refs/heads/{branch}")` ⇒ `InvalidInput`.
2. **Chain walk:** BFS over `vcs_changeset_parents` from `head`; `linked_lookup(cs)` hit ⇒ candidate anchor — verify the anchor object still exists (`verify_commit_oid(git_oid)`; gc'd ⇒ treat node as UNLINKED so re-mint heals — dev-major 6). Set ≤ `export_chain_max` ⇒ `TooLarge`; topo-order parents-first.
3. **Empty set fast-path (nit h):** `head` already anchored ⇒ if `old == anchored oid` return early `{git_oid: old, commits_count: 0}` (no writes, no move — honest "already exported"); else continue to the ff gate with the anchored oid as tip.
4. **FF gate (changeset-side, pre-mint):** `old = verify_commit_oid(refs/heads/{branch})` — `None` ⇒ create path; `Some` ⇒ `old_cs = link_lookup(old_oid)` (FORWARD direction — review BLOCKER 3); hit ∧ (`old_cs ∈ set` OR `is_ancestor_cs(old_cs, head)` — a LOCAL in-crate helper over the same `vcs_changeset_parents` BFS, mirroring u5's private `is_ancestor` semantics without a u5 edit — dev 18d) ⇒ ff; hit-but-not-ancestor OR **miss** ⇒ defer to the git-side test at step 6 — a miss is NOT yet `RefConflict` (F7: an old tip minted by an earlier export whose link row died, or held by an alias twin, is still a legal ff; only a genuinely unreachable `old` is a clobber).
5. **Write chain:** per changeset topo-order → `export_one` → `link_insert` + `linked:` row. Same-oid alias (dev 11): `link_insert` false with stored≠ours is content-identical collision ⇒ still write `linked:{ours}→oid` and proceed (forward link stays with the first-minter — both directions stay consistent).
6. **FF gate (git-side, post-mint — only when step 4 didn't pass):** `is_ancestor(old_oid, minted_tip)` (the EXISTING `GitBackend::is_ancestor`, backend.rs:735 — review3-F1) ⇒ ff — identical object ⇒ trivially ancestor (twin-alias), unlinked-but-reachable tips covered too. Else the move is non-ff and splits on old's provenance: **unlinked `old` ⇒ `RefConflict{actual: None}` even under `force`** — a genuine git-only tip is never clobbered (the stated requirement; force overrides ff-ness, not provenance-blindness — git-side pushes that bypassed import are protected work, not force-rewrite targets). **Linked `old` ⇒ `force ? continue : RefConflict{actual: old_cs}`** — `actual` carries the changeset behind old's FORWARD link (`None` when unlinked) — the wire field is `Option<ChangesetId>` (`VcsRefConflictData.actual`, vcs_types.rs:414), never a git oid (F8).
7. **Protection:** `checker.check(repo, refname, old, minted_tip, actor)` — NO force flag (F2): the impl derives intent from `(old,new)` like `check_ref_update` (old absent/zeros⇒Push, else ff-probe⇒Push|ForcePush) so `allow_force_pushes` bites exactly when the real move isn't ff. `reasons` non-empty ⇒ `Forbidden{verdicts: ProtectionBlockReasons}` — wire-verbatim (u1's `forbidden(message, verdicts)` constructor, vcs_types.rs:394).
8. **Move:** `!force` ⇒ `fast_forward_ref(…, actor)`; `force` ⇒ `force_update_ref(…, actor)` — classified stderr: `Denied` ⇒ `Forbidden{verdicts: {reasons:["push_hook"], ..}}` + fixed detail (never raw stderr — it can carry rejected values/paths); `Conflict` ⇒ `RefConflict{actual: re-read tip's forward-link cs}`.
9. Return `{branch, git_oid: tip, commits_count, before_oid: old}` — fetch/log/show read normally (AC7.2.4). Concurrent `git gc --prune` can reap minted-unreferenced objects between write and move — same exposure `commit_files` already carries (nit g; a re-export heals via re-mint). `export_chain_max` is an honest bound: deeper chains export by staging through an ancestor head (export an intermediate head first — it becomes the anchor — then re-export).

## Q7 — Error taxonomy (`InteropError`)

| `InteropError` | wire code | Notes |
|---|---|---|
| `DbNotConfigured` | `db.not_configured` | eager `is_skipped` on all three entries. |
| `NotFound` | `vcs.not_found` | absent head / unresolvable `from`/`git_oid`; `ParentNotFound`/`ChangesetNotFound` bridge here (dev-minor 8). |
| `InvalidInput(String)` | `vcs.invalid_input` | refname parity, limit range, non-UTF8 path/message/author, malformed ident, `is_git_oid` fail, dash-led `from`. `field: reason`, no echo. |
| `ImportState(String)` | `vcs.import_state` | malformed cursor, marker contention, `last_oid` ordering drift. |
| `Forbidden{verdicts: ProtectionBlockReasons}` | `vcs.forbidden` | checker verbatim; push `Denied` ⇒ `{reasons:["push_hook"], ..Default}` + fixed detail — never stderr (it can carry rejected values/paths). |
| `RefConflict{refname, actual: Option<ChangesetId>}` | `vcs.ref_conflict` | non-ff / unlinked `old` / TOCTOU `Conflict`; `actual` = old's forward-link changeset (`None` when unlinked) — never a git oid. |
| `TooLarge` | `vcs.too_large` | cap overflows. |
| `QuotaExceeded(QuotaScope)` | `vcs.quota_exceeded` | put quota — scope carried. |
| `Internal(String)` | `rpc.internal` | backend `Process`/`Io`, topo breaks, divergent links — fixed messages. |

**Consumed-enum bridges (total — every variant of every consumed enum has a named arm):** `ChangesetError`: `ParentNotFound`/`ChangesetNotFound`→`NotFound`, `InvalidInput`→`InvalidInput`, `ObjectNotFound`→`Internal` (blob raced GC = integrity loss), `DbNotConfigured`→`DbNotConfigured`, `Internal`→`Internal` — NO `QuotaExceeded` variant exists on the enum (dev-minor 8). `IndexError`: `CursorMalformed`→`ImportState` (it IS a cursor-shape error on this surface), `DbNotConfigured`→`DbNotConfigured`, rest→`Internal`. `ObjectStoreError`: `QuotaExceeded`→`QuotaExceeded`, `DbNotConfigured`→`DbNotConfigured`, `Config`/`Db`/`Io`/`InvalidHash`/`NotFound`/`Stream`/`Internal`→`Internal` — explicit enumeration, no silent catch-all (F3). Facade `String`→`Internal`.

## Q8 — Caps (`InteropCaps`)

| Field | Default | Bounds |
|---|---|---|
| `import_commit_cap` | 1000 | contract ceiling. |
| `rev_list_max` | 250_000 | `RevList.truncated` ⇒ `TooLarge`; a >250k single cone cannot import (honest bound — no `^oid` exclusion expressible, nit e). |
| `cursor_tips_max` | 512 | tip-snapshot bound. |
| `cursor_payload_max_bytes` | 32_768 | serialized `CursorState` bound — mysql `payload TEXT` = 64KiB hard ceiling (mysql/0053:77); over ⇒ `TooLarge`, never write an unresumable cursor (F6). |
| `import_tree_max` | 10_000 | name-status rows AND `tree_flat` leaf count AND export per-node trees (enforced DURING accumulation — a per-page check never trips; F10). |
| `import_blob_max` | `objects.max_object_bytes` | pre-read via `tree_flat` sizes. |
| `export_chain_max` | 10_000 | unlinked-ancestor walk. |
| `export_blob_max` | `objects.max_object_bytes` | `objects.get`→`File.take(cap+1)` (dev 22b). |
| `export_ancestor_probe_max` | 250_000 | ff-gate BFS (dev 18b). |
| `import_lock_ttl_secs` | 900 | stale-marker reclaim. |

## Q9 — What u6 does NOT do

- **No wire DTOs/routes/authz, no activity rows, no lazy-trigger** — u7's; `ImportPage`/`ExportOutcome` carry the activity fields. `ImportHead.changeset_id` is `Option` — **recorded wire-DTO amendments for u7's `vcs.git_import` handler (review3-F3):** widen `VcsGitImportHead.changeset_id` to `Option<ChangesetId>` (vcs_types.rs:884) so a mid-batch unimported tip can emit `null`, and add `skipped_tips: u64` to `VcsGitImportResponse` (vcs_types.rs:901-908) so a dropped non-commit ref surfaces on the wire — both land as u6 deviations on the u1-claimed `vcs_types.rs` (u6 produces the values; u7 maps them), each recorded in the manifest.
- **u2 file edits ARE in scope (Q3)** — the covering_changeset nullability amendment (migrations ×3 + facade signature) is u6-authored and lands in u6's manifest; everything else in `vcs.rs` untouched.
- **No mirror/automatic export; no refs moved on import; no submodule representation; no `vcs_import_link_page` consumer** (u7/diagnostics).
