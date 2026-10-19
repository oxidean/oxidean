# Functional Design Questions — u6-vcs-git-interop

Unit: `u6-vcs-git-interop` — `GitInterop` inside `oxidean-vcs`'s `server` feature. Consumes `Arc<dyn GitBackend>` + `Arc<dyn ProtectionChecker>` by injection (ADR-006 — u7 wires them), `Database` facade import-link + derived-index + changeset probes, `ChangesetStore::create` (single changeset write path), `ObjectStore::{put,get}`, `ReadIndex::tree` (export trees). Branch moves go only through `fast_forward_ref`/`force_update_ref` (hook-driven) — never bare `update_ref`. *(Revision 2 — dev pass (24 findings) + architecture review pass-1 (3 blockers, 6 majors, 3 minors, 8 nits) integrated.)*

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
- `ProtectionChecker` is defined IN-CRATE (u6 owns the trait port); `oxidean-api` supplies the real impl at dispatch. Trait shape (review MINOR 7 — `force` param: the real evaluator splits Push vs ForcePush, `allow_force_pushes` bites only under ForcePush):

```rust
#[async_trait]
pub trait ProtectionChecker: Send + Sync {
    /// Same verdict computation a `git push` of `{old → new}` onto `refname`
    /// would face — `force` selects the ForcePush rule branch.
    async fn check(&self, repo: &Path, refname: &str,
                   old: Option<&str>, new: &str,
                   actor_capability: &str,
                   force: bool) -> Result<Vec<ProtectionVerdict>, String>;
}
pub struct ProtectionVerdict { pub rule: String, pub outcome: String, pub detail: String }
```

- `actor_capability` threads from the wire (u7 supplies the session capability) so the verdict matches what a push by that actor would get — `OXIDEAN_ACTOR_CAPABILITY` is what the hook reads (cli.rs:253-278).

## Q2 — `GitBackend` surface extension (new methods authored by u6 inside `oxidean-git`)

**Decision:** the trait lacks raw commit reads, topo enumeration, flat-tree listings, refname validation, existence typing, and object-plumbing writes. u6 adds exactly these (trait + `CliGitBackend` impl + tests):

| Method | Shape | Backing plumbing |
|--------|-------|------------------|
| `cat_commit` | `(repo, sha) -> RawCommit{tree, parents: Vec<String>, author: GitIdent, committer: GitIdent, message: Vec<u8>}` | `git cat-file commit <sha>` — raw header parse: `tree`, `parent`×n, `author`/`committer` parsed byte-level (last-` <`/`>` split, trailing `unix ±ZZZZ`); folded continuation headers (`gpgsig`, `mergetag`, `encoding` — space-led lines continue the previous header) skipped; malformed ident ⇒ typed parse error → `InvalidInput`, never panic (dev 19). `message` = bytes after first blank line, verbatim. |
| `commit_name_status` | `(repo, base: Option<&str>, sha) -> Vec<NameStatusRow>` | `git diff-tree -r --name-status -M -z -c diff.renameLimit=10000` — two-tree `<base> <sha>` for normal commits; `--root --no-commit-id` single-arg for roots (dev 10). `-M` default similarity; `diff.renameLimit` pinned config-independent (dev 8). `-z`: status records NUL-terminated; `R<score>` rows carry TWO path records consumed together. Paths byte-parsed → UTF-8 ⇒ else typed error → `InvalidInput` (dev 6). |
| `tree_flat` | `(repo, sha) -> Vec<FlatTreeEntry>` | `git ls-tree -r -l -z --full-tree <sha>` — one subprocess → every leaf `{mode,kind,oid,path,size}`; enables pre-read blob-cap checks (dev 17). |
| `rev_list` | `(repo, tips: &[String], max: usize) -> Result<RevList, GitError>` | `git rev-list --topo-order --max-count=max+1 <tips…>` — the emitted set (children-first) is REVERSED in-crate → `RevList.oids` is PARENTS-FIRST (dev BLOCKER 2; `--reverse`+`--max-count` would truncate the wrong end). `truncated` when `len > max`. `tips.is_empty()` ⇒ `Ok(RevList{[],false})` (dev 15). |
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

- **Migration 0055** `vcs_derived_index_nullable_covering.sql` ×3 dialects (filename parity): postgres `ALTER TABLE vcs_derived_index ALTER COLUMN covering_changeset DROP NOT NULL`; mysql `MODIFY covering_changeset CHAR(29) NULL`; sqlite table-rebuild (ALTER can't drop NOT NULL) preserving the FK + `ON DELETE CASCADE`.
- **`vcs.rs` signature change:** `VcsDerivedIndexUpsert.covering_changeset: Option<&str>` (was `&'a str`); existing callers (`read_index.rs` etc.) wrap `Some(...)`; `.bind(u.covering_changeset)` passes NULL for state rows.
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
7. **State** — upsert `key="cursor"` (nonce re-verified, `covering_changeset = last imported cs or None`); `release`; return `{imported, cursor: Some iff !done, done, heads}` — `heads[i].changeset_id = Option` from `link_lookup(tip)` (dev-major 5: unresolved tips emit `null` — contract errata noted for u7).
8. Errors ⇒ `release` still runs (nonce-verified); durable `skip` makes replay idempotent.

## Q5 — Per-commit conversion (`import_one` — the AC7.1.2 convergence pin)

1. `cat_commit` → `RawCommit`.
2. Parents → `link_lookup` each in stored order; miss ⇒ `Internal` (topo invariant — parents-first enumeration makes misses impossible on a healthy list).
3. `rows = commit_name_status(...)`; `flat = tree_flat(sha)` (`flat.len() > import_tree_max` ⇒ `TooLarge` — dev 10-minor); parent-side `tree_flat` lazily only when `D`/`R` rows exist.
4. Per row → `FileChange` (`A`⇒add, `M`/`T`⇒modify, `D`⇒delete{parent mode}, `R`⇒rename{new,old,mode,blob}); either-side gitlink ⇒ skip+warn; `size > import_blob_max` ⇒ `TooLarge` pre-read; `cat_blob` → `objects.put` → VCS sha256.
5. `message` → UTF-8 fail ⇒ `InvalidInput`; refusal inventory (BR6.23): `>64` parents, `>65535`-byte message, unrepresentable `rfc3339(a.unix)` (nit c — `from_timestamp` `None` ⇒ `InvalidInput`, no panic), `>512`-char paths, `.`-segments/control-byte paths (validate_path:241-266 full set), empty post-sanitize author name OR email (compose_author:441-455 — email unsanitized, `<`/`>`/control ⇒ refuse).
6. `store.create({parents, author_name: sanitize(a.name_utf8), author_email: a.email_utf8, message, committed_at: rfc3339(a.unix)})`.
7. `ident` sidecars AFTER `create` — `ident:{git_oid}` + `ident-cs:{cs}` raw-byte payloads (BR6.12).
8. Return id — `link_insert` + `linked:` are the caller's step.

## Q6 — Export algorithm (`vcs.git_export`)

**Decision:** `export(repo_id, repo, head, branch, force, actor) -> ExportOutcome` — ordering is **write-chain → check → move** (the checker needs the minted tip to evaluate the real `(old,new)` pair — review's reordering of my earlier pre-write check; denial leaves only unreferenced, gc-reaped objects).

1. `is_skipped`/`parse`/`row_probe`; `check_refname("refs/heads/{branch}")` ⇒ `InvalidInput`.
2. **Chain walk:** BFS over `vcs_changeset_parents` from `head`; `linked_lookup(cs)` hit ⇒ candidate anchor — verify the anchor object still exists (`verify_commit_oid(git_oid)`; gc'd ⇒ treat node as UNLINKED so re-mint heals — dev-major 6). Set ≤ `export_chain_max` ⇒ `TooLarge`; topo-order parents-first.
3. **Empty set fast-path (nit h):** `head` already anchored ⇒ if `old == anchored oid` return early `{git_oid: old, commits_count: 0}` (no writes, no move — honest "already exported"); else continue to the ff gate with the anchored oid as tip.
4. **FF gate:** `old = verify_commit_oid(refs/heads/{branch})` — `None` ⇒ create path; `Some` ⇒ `old_cs = link_lookup(old_oid)` (FORWARD direction — review BLOCKER 3); miss ⇒ git-only tip ⇒ `RefConflict{actual}` (never clobber); hit ⇒ `is_ancestor_cs(old_cs, head)` OR `old_cs ∈ set`; non-ff ⇒ `force ? continue : RefConflict{actual}`.
5. **Write chain:** per changeset topo-order → `export_one` → `link_insert` + `linked:` row. Same-oid alias (dev 11): `link_insert` false with stored≠ours is content-identical collision ⇒ still write `linked:{ours}→oid` and proceed (forward link stays with the first-minter — both directions stay consistent).
6. **Protection:** `checker.check(repo, refname, old, minted_tip, actor, force)` → `deny` ⇒ `Forbidden{verdicts}`.
7. **Move:** `!force` ⇒ `fast_forward_ref(…, actor)`; `force` ⇒ `force_update_ref(…, actor)` — classified stderr: `Denied` ⇒ `Forbidden{[push_hook]}` (signature/content verdicts on pushed commits land here); `Conflict` ⇒ `RefConflict{actual: re-read}`.
8. Return `{branch, git_oid: tip, commits_count, before_oid: old}` — fetch/log/show read normally (AC7.2.4). Concurrent `git gc --prune` can reap minted-unreferenced objects between write and move — same exposure `commit_files` already carries (nit g; a re-export heals via re-mint).

## Q7 — Error taxonomy (`InteropError`)

| `InteropError` | wire code | Notes |
|---|---|---|
| `DbNotConfigured` | `db.not_configured` | eager `is_skipped` on all three entries. |
| `NotFound` | `vcs.not_found` | absent head / unresolvable `from`/`git_oid`; `ParentNotFound`/`ChangesetNotFound` bridge here (dev-minor 8). |
| `InvalidInput(String)` | `vcs.invalid_input` | refname parity, limit range, non-UTF8 path/message/author, malformed ident, `is_git_oid` fail, dash-led `from`. `field: reason`, no echo. |
| `ImportState(String)` | `vcs.import_state` | malformed cursor, marker contention, `last_oid` ordering drift. |
| `Forbidden{verdicts}` | `vcs.forbidden` | checker verbatim; push `Denied` ⇒ `[{rule:"push_hook"}]`. |
| `RefConflict{refname, actual: Option}` | `vcs.ref_conflict` | non-ff / unlinked `old` / TOCTOU `Conflict`. |
| `TooLarge` | `vcs.too_large` | cap overflows. |
| `QuotaExceeded(QuotaScope)` | `vcs.quota_exceeded` | put quota — scope carried. |
| `Internal(String)` | `rpc.internal` | backend `Process`/`Io`, topo breaks, divergent links — fixed messages. |

**Consumed-enum bridges:** `ChangesetError`: `ParentNotFound`/`ChangesetNotFound`→`NotFound`, `InvalidInput`→`InvalidInput`, `ObjectNotFound`→`Internal` (blob raced GC = integrity loss), `DbNotConfigured`→`DbNotConfigured`, `Internal`→`Internal` — NO `QuotaExceeded` variant exists on the enum (dev-minor 8). `IndexError`: `CursorMalformed`→`Internal`, `DbNotConfigured`→`DbNotConfigured`, rest→`Internal`. `ObjectStoreError`: `QuotaExceeded`→`QuotaExceeded`, `InvalidHash`/`NotFound`/`Stream`→`Internal`, `DbNotConfigured`→`DbNotConfigured`. Facade `String`→`Internal`.

## Q8 — Caps (`InteropCaps`)

| Field | Default | Bounds |
|---|---|---|
| `import_commit_cap` | 1000 | contract ceiling. |
| `rev_list_max` | 250_000 | `RevList.truncated` ⇒ `TooLarge`; a >250k single cone cannot import (honest bound — no `^oid` exclusion expressible, nit e). |
| `cursor_tips_max` | 512 | tip-snapshot bound. |
| `import_tree_max` | 10_000 | name-status rows AND `tree_flat` leaf count AND export per-node trees. |
| `import_blob_max` | `objects.max_object_bytes` | pre-read via `tree_flat` sizes. |
| `export_chain_max` | 10_000 | unlinked-ancestor walk. |
| `export_blob_max` | `objects.max_object_bytes` | `objects.get`→`File.take(cap+1)` (dev 22b). |
| `export_ancestor_probe_max` | 250_000 | ff-gate BFS (dev 18b). |
| `import_lock_ttl_secs` | 900 | stale-marker reclaim. |

## Q9 — What u6 does NOT do

- **No wire DTOs/routes/authz, no activity rows, no lazy-trigger** — u7's; `ImportPage`/`ExportOutcome` carry the activity fields. `ImportHead.changeset_id` is `Option` — contract errata (`changeset_id|null` while a tip is unimported mid-batch, dev-major 5).
- **u2 file edits ARE in scope (Q3)** — the covering_changeset nullability amendment (migrations ×3 + facade signature) is u6-authored and lands in u6's manifest; everything else in `vcs.rs` untouched.
- **No mirror/automatic export; no refs moved on import; no submodule representation; no `vcs_import_link_page` consumer** (u7/diagnostics).
