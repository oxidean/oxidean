# Security Design — u6-vcs-git-interop

## SD6.1 — Trust boundaries

```
client ──U7 transport/auth──> GitInterop::{import_chain, lazy_import, export}
  untrusted inputs: repo_id, from (ref|oid|None), cursor (opaque b64url,
                    FORGEABLE), limit, branch, force, actor_capability,
                    InteropCaps (host-tuned)
interop ──facade──> oxidean-db (trusted SQL; String errors opaque)
interop ──subprocess──> oxidean-git cli (UNTRUSTED boundary: fixed argv only,
                    stdin for messages/mktree, bounded stdout reads,
                    stderr never propagated)
interop ──put──> ObjectStore (streamed import blobs)
interop ──get──> ObjectStore (capped export reads)
interop ──trait──> ProtectionChecker (host impl, verdict verbatim)
interop ──refmove──> fast_forward_ref/force_update_ref (hooked, CAS'd)
```

The subprocess boundary is the unit's primary attack surface: every argv position is either a literal flag or a pre-validated value (dash-led rejected `InvalidInput`/`ImportState` BEFORE argv construction — caller `from`/`rev`/`branch` AND every oid decoded from a caller-supplied `CursorState`). `InteropCaps::validate` at `new` pins `*_blob_max ≤ objects.max_object_bytes` and `*_max ≤ usize::MAX / 2048` (bounds the LARGEST derived-bound multiplier — a misconfigured cap near `usize::MAX` would otherwise wrap the `× 1024`/`× 2048`/`× 41` byte bounds instead of typing `InvalidInput`; support-F2) → `InvalidInput` on host misconfig.

## SD6.2 — Cursor decode pipeline (untrusted input, ordered)

`cursor` arrives as an opaque string a client can forge. Decode order — steps 1–3 run BEFORE any cursor-derived value reaches argv or indexing (steps 4–5 run post-`rev_list`; tip resolution and `rev_list` themselves precede decode in the SD6.3 pipeline — the rule is that nothing DECODED enters argv until validated):

1. `cursor.len() > cursor_payload_max_bytes` ⇒ `ImportState` (pre-parse byte bound — no unbounded b64/JSON decode; the emitted `encode_cursor` string is checked on its opaque b64url length for symmetric round-trip).
2. b64url → JSON → `CursorState` ⇒ `ImportState` on malformed (never a parse panic).
3. Field revalidation: `v == 1` else `ImportState`; `tips.len() ≤ cursor_tips_max` else `ImportState`; `tips[].git_oid` is 40-hex and non-dash-led else `ImportState`; `tips[].name` ≤ 256 chars, no `\0`, else `ImportState`; `skip` parses as `usize` (a forged negative/huge JSON number fails deserialization itself). The same field bounds apply at `encode_cursor` — a server-minted cursor must always re-decode cleanly (a >256-char refname refused at encode `TooLarge` rather than minting a cursor that fails its own resume; support-F6).
4. Post-`rev_list` bound: `skip > list.len()` ⇒ `ImportState` BEFORE `list[skip-1]`/`list[skip..]` indexing (devsecops-M1 — the index panic would otherwise be the only guard).
5. Continuity: `skip>0` ⇒ `list[skip-1] == last_oid` else `ImportState`; `last_oid` must itself be oid-shaped.

Only after all five does the window slice or any parent lookup run.

## SD6.3 — Claim + write-order discipline

```
import_chain: is_skipped → limit bounds → dash-led reject → claim
  (upsert active{nonce} → re-read → nonce-match else ImportState)
  → tips resolve → cursor decode (SD6.2) → rev_list → window loop
  → cursor upsert (nonce re-verify → byte-cap pre-check)
  → release (nonce re-verify — skip on foreign nonce)
  on ANY error: release still runs, nonce-verified
export: is_skipped → parse head → check_refname → chain BFS (capped)
  → ff gate (changeset-side) → WRITE FULL OBJECT CHAIN
  → ff gate (git-side fallback) → checker.check(real old,new)
  → exactly one hooked move → outcome (activity fields on ExportOutcome;
    u7 persists the row — interop writes none)
```

Write-before-check ordering is load-bearing: the verdict evaluates `(old, minted_tip)` — checking before the mint is unevaluable. A denied verdict leaves the written objects UNREFERENCED (gc reaps; re-export re-mints — named residual). The `active` marker is best-effort only — BR6.4 convergence (insert-if-absent + idempotent create) is the correctness backstop, so interleaving a raced import is cost, never corruption.

## SD6.4 — Subprocess hygiene design

- `run_git_stdout_bounded(args, stdin, env, max_bytes)` — the new shared reader: incremental stdout drain; past `max_bytes` it kills the child and returns `GitError::Process` carrying the fixed sentinel string `oxidean:stdout-bound-exceeded` (the distinguishable bound-kill signal — the interop `GitError` arm maps sentinel-bearing `Process` → `TooLarge`; a plain `Process` cannot be distinguished from a real spawn/exec failure, so the sentinel is the only honest channel). `tree_flat` receives the caller's `import_tree_max × 1024`; `commit_name_status` `import_tree_max × 2048` (caller-supplied `max_bytes` params — TD6.2 signatures); `rev_list` bounds internally at `max × 41` (40-hex + `\n` — exact); `cat_commit` bounds internally at a private `CAT_COMMIT_MAX` (commit objects can't exceed the blob-cap class).
- `-c` overrides are TOP-LEVEL argv — `-c diff.renameLimit=10000` / `-c commit.gpgSign=false` appear before the subcommand token.
- Commit messages and `mktree` rows ride stdin (argv `-m` spawn-fails on NUL).
- `verify_commit_oid` = `rev-parse --verify --quiet {rev}^{commit}` — peels annotated tags, `None` on non-commit/miss; dash-led `rev` is rejected before argv.
- `git check-ref-format` is the refname oracle (u1's `validate_treeish` accepts `a//b`, `x.lock`, `a@{b}` — dev 16; NOT reused).
- `is_ancestor` = EXISTING `GitBackend::is_ancestor` — zero new code for the git-side ff fallback.
- Raw `cat-file commit` parse: folded continuation headers skipped (`gpgsig`/`mergetag`/`encoding`), last-` <`/`>` ident split, byte-level fields — malformed ident ⇒ typed `InvalidInput`, never panic, never lossy.

## SD6.5 — Export gate + move design (AC7.2.2 parity)

```
ff gate (changeset-side, pre-mint):
  old = verify_commit_oid(refs/heads/{branch})
  None ⇒ create path (no gate)
  Some ⇒ link_lookup(old_oid): hit ∧ (old_cs ∈ set | is_ancestor_cs) ⇒ ff
         miss/hit-non-ancestor ⇒ DEFER (not yet conflict)
write chain (pre-check — verdict needs minted tip):
  topo-order export_one → link_insert + linked: row
  same-oid alias (insert false, stored≠ours) ⇒ still linked:{ours}
ff gate (git-side, post-mint — only when side-1 didn't pass):
  is_ancestor(old_oid, minted_tip) ⇒ ff
  else: UNLINKED old ⇒ RefConflict{actual:None} — even under force
        LINKED old ⇒ force ? proceed : RefConflict{actual:old_cs}
checker.check(repo, refname, old, minted_tip, actor):
  Err(_) ⇒ Internal opaque (logged); reasons non-empty ⇒
  Forbidden{verdicts} verbatim; empty ⇒ proceed
move (exactly one, classified):
  !force ⇒ fast_forward_ref(..., actor) | force ⇒ force_update_ref(..., actor)
  Denied ⇒ Forbidden{reasons:["push_hook"], ..Default} + fixed message
  Conflict ⇒ RefConflict{actual: re-read forward link — Option<ChangesetId>}
```

The checker carries NO force flag — intent derives from `(old,new)` exactly as `check_ref_update` does, so `allow_force_pushes` bites iff the real move is non-ff. `actor_capability` is threaded through the checker AND the move; `GitInterop` never invents a default.

## SD6.6 — Bounded traversal architecture

| Operation | Bound | Over → |
|-----------|-------|--------|
| `rev_list` emitted set | `rev_list_max` 250k | `TooLarge` |
| `rev_list` stdout | `rev_list_max × 41` bytes | `TooLarge` |
| page window / caller limit | `import_commit_cap` 1_000 | `InvalidInput` |
| resolved tips | `cursor_tips_max` 512 | `TooLarge` |
| serialized cursor (in + out) | `cursor_payload_max_bytes` 32_768 | `ImportState` / `TooLarge` |
| `tree_flat` leaves | `import_tree_max` 10_000 | `TooLarge` |
| `tree_flat`/`name_status` stdout | `import_tree_max × 1024/2048` | `TooLarge` |
| `cat_commit` stdout | `CAT_COMMIT_MAX` const (~`import_blob_max`-class) | `TooLarge` |
| `cat_blob`/`write_object` bytes | `import_blob_max` / `export_blob_max` | `TooLarge` |
| serialized `IdentSidecar` | `cursor_payload_max_bytes` | `InvalidInput` pre-create |
| export-set BFS | `export_chain_max` 10_000 | `TooLarge` |
| ff-gate changeset-side probe | `export_ancestor_probe_max` 250k | `TooLarge` |
| `linked:`/`ident`/`cursor`/`active` keyspaces | fixed 5 keys, no others | — |

Uncapped-but-named paths (SR6.10): per-move full-clone IO (inherited backend cost), no `run_git*` timeout, cold `ReadIndex::tree` materialize (u4-owned), `vcs_changeset_parents` raced-empty on mid-walk delete (converges `ParentNotFound`⇒`NotFound`), **`list_refs` unbounded stdout** (cli.rs:1896-1949 `Command::output()` — the `cursor_tips_max` 512 bound applies only post-materialization; a >512-tip repo materializes the full list before the cap trips — accepted: tip counts are bounded by real refs, not attacker-scalable bytes; support-F4).

## SD6.7 — Error channel design

```
From                                        → InteropError
ObjectStoreError::QuotaExceeded(scope)      → QuotaExceeded(scope)
ObjectStoreError::DbNotConfigured           → DbNotConfigured
ObjectStoreError::{Config,Db,Io,InvalidHash,NotFound,Stream,Internal} → Internal
ChangesetError::{ParentNotFound,ChangesetNotFound} → NotFound
ChangesetError::InvalidInput                → InvalidInput
ChangesetError::ObjectNotFound              → Internal
ChangesetError::DbNotConfigured             → DbNotConfigured
ChangesetError::Internal                    → Internal
IndexError::CursorMalformed                 → Internal (u5 parity — index cursors are interop-minted paging tokens, never caller input; malformed ⇒ internal corruption)
IndexError::FrontierTooLarge                → TooLarge (u5 parity — a real bound trip)
IndexError::DbNotConfigured                 → DbNotConfigured
IndexError::all others                      → Internal
GitError::Denied (from classify_push_error) → Forbidden{push_hook}
GitError::Conflict                          → RefConflict{re-read link}
GitError::NotFound                          → NotFound (raced cat_blob/verify miss)
GitError::InvalidArg                        → InvalidInput (validation-shaped calls)
GitError::Process containing the sentinel   → TooLarge (bounded-reader kill — see SD6.4)
  `oxidean:stdout-bound-exceeded`
GitError::{Process(other),Io}               → Internal (fixed message)
checker.check Err(String)                   → Internal opaque (logged, never echoed)
facade String                               → Internal opaque
io::Error                                   → Internal
```

Messages are `field: reason` — no stderr, no rejected values, no paths, no auth material. `ChangesetId::parse` text embeds the raw id — DISCARDED; fixed `"changeset id: malformed"` constructed. `InteropError` does NOT derive `PartialEq` (u5 convention; `ProtectionBlockReasons` inside `Forbidden` lacks it — recorded deviation, tests field-compare).

## SD6.8 — Observability design

`tracing::info_span!("vcs.git_interop", unit="vcs.git_interop", op, repo_id)` on each public op; `warn!` on gitlink skips + non-commit tip drops (counted → `skipped_tips`); `info!` on claim/release/page/export outcome; `debug!` on cursor continuity checks. Fields: repo_id, op, counts, oids, cs ids — NEVER blob contents, ident payloads, stderr text, ref values in errors, or auth material.
