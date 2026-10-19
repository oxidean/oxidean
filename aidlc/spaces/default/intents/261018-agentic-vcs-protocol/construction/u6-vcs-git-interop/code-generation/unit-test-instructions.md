# Unit Test Instructions — u6-vcs-git-interop

VCS-side tests live in `crates/oxidean-vcs/src/git_interop.rs` under `#[cfg(all(test, feature = "server"))]` — in-file module per u3/u4/u5 precedent. Seed state through `testutil::test_db()` → `(TempDir, Database)` (file-backed sqlite; `TempDir` held alive via `let (_dir, db) = ...`; never `:memory:`) + `ObjectStore`/`ChangesetStore`/`ReadIndex` instances per the store's own constructors. Git fixtures are REAL temporary repos (`git init` under `tempfile::tempdir()`) driven through `CliGitBackend` — the cli.rs:3686-suite precedent; `git` must be on PATH (docker-resident git per the project's docker-first posture). `ProtectionChecker` uses a crate-local mock recording call order + returning canned `ProtectionBlockReasons`. `InteropCaps` constructor-injected small for cap tests. `Database::skipped()` covers `DbNotConfigured`.

## Exact commands (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --features server          # primary — U6 suite
cargo test -p oxidean-vcs --features server                 # fallback arm
cargo nextest run -p oxidean-git                          # backend extension suite
cargo nextest run -p oxidean-db --test dialect_vcs        # NULL-covering sqlite arm
```

## Coverage matrix — cursor codec (private `encode_cursor`/`decode_cursor`)

| Test | Asserts |
|------|---------|
| `cursor_round_trip` | encode→decode round-trip preserves `{v,tips,skip,last_oid}` |
| `cursor_inbound_byte_cap` | opaque `cursor.len() > cursor_payload_max_bytes` ⇒ `ImportState` pre-parse |
| `cursor_bad_b64` / `cursor_bad_json` | malformed opaque ⇒ `ImportState`, never panic |
| `cursor_wrong_version` | `v != 1` ⇒ `ImportState` |
| `cursor_tips_over_max` | `tips.len > cursor_tips_max` ⇒ `ImportState` |
| `cursor_tip_oid_shape` | non-40-hex or dash-led tip oid ⇒ `ImportState` (pre-argv guard) |
| `cursor_tip_name_bounds` | >256-char or NUL-bearing name ⇒ `ImportState` |
| `cursor_skip_gt_len` | forged `skip > rev_list.len()` ⇒ `ImportState` BEFORE indexing (devsecops-M1) |
| `cursor_last_oid_continuity` | `skip>0` + `list[skip-1] != last_oid` ⇒ `ImportState`; match ⇒ resume |
| `cursor_emit_byte_cap` | emitted opaque b64url `> cursor_payload_max_bytes` ⇒ `TooLarge` (emit-side cap on the opaque string, not inner JSON) |
| `cursor_emit_name_bound` | >256-char refname ⇒ `TooLarge` at encode — never mint a self-failing cursor |

## Coverage matrix — claim/release (`active` marker protocol)

| Test | Asserts |
|------|---------|
| `claim_foreign_active` | second claim while `active` held ⇒ `ImportState` |
| `claim_ttl_reclaim` | `started_at` older than `import_lock_ttl_secs` ⇒ stale reclaim succeeds |
| `release_nonce_match` | matching nonce ⇒ `active` upserted to terminal state |
| `release_nonce_mismatch` | foreign nonce ⇒ silent skip, never error, `active` row untouched |
| `release_terminal_states` | `ok:true` ⇒ `idle`; `ok:false` ⇒ `failed` — both release |
| `claim_covering_null` | `active`/`cursor` rows persist `covering_changeset = NULL` post-0055 |

## Coverage matrix — import (`import_chain` / `lazy_import` / `import_one`)

| Test | Asserts |
|------|---------|
| `import_linear_chain` | seeded repo → one changeset per commit, parents mapped via links, `ident:`/`ident-cs:` sidecars + `linked:` row + import-link each |
| `import_merge_commit` | ≥2-parent commit keeps all parents in stored order; changes vs `parents[0]` |
| `import_rename_rows` | name-status R-rows ⇒ `rename` `FileChangeInput` (src→dst order) |
| `import_gitlink_skip` | either-side gitlink mode ⇒ skipped + `warn!`, never imported |
| `import_blob_cap_pre_read` | `size > import_blob_max` ⇒ `TooLarge` BEFORE `cat_blob` |
| `import_idempotent_rerun` | re-import ⇒ early-skip via existing link; `imported` counts fresh only |
| `import_divergent_link` | `insert_if_absent` false + stored cs ≠ ours ⇒ `Internal` (BR6.4 provenance guard) |
| `import_tip_peel_drop` | non-commit peel target (tag→tree) ⇒ dropped + `skipped_tips` counted on page |
| `import_from_dash_led` | dash-led `from` ⇒ `InvalidInput` pre-argv |
| `import_limit_bounds` | `limit` outside `1..=import_commit_cap` ⇒ `InvalidInput` (the ONE InvalidInput bound) |
| `import_resume_window` | second page resumes at cursor `skip`; parents-first ordering preserved |
| `lazy_direct_tip_seed` | branch literally named the 40-hex target CANNOT shadow it — `tips=[oid]` bypass |
| `lazy_multi_page_loop` | target deeper than one `import_commit_cap` window ⇒ loop via cursor carry |
| `lazy_done_unlinked` | `page.done ∧ link_lookup(oid) miss` ⇒ `Internal` (enumeration drift) |
| `lazy_early_linked` | already-linked oid ⇒ early `{done:true}` no-op |
| `lazy_oid_shape` | non-40-hex / 64-hex ⇒ `InvalidInput`; `verify_commit_oid None` ⇒ `NotFound` |
| `import_ident_sidecar_bytes` | sanitized/non-UTF8 ident bytes round-trip byte-exact via b64 fields |

## Coverage matrix — export (`export` / `export_one` / `walk_export_set`)

| Test | Asserts |
|------|---------|
| `export_empty_set` | head already anchored ⇒ no writes, no checker call, no move; `commits_count:0` |
| `export_write_before_check` | mock checker records call order — objects written BEFORE `check`, exactly ONE move after |
| `export_ff_proceed` | linked ancestor old ⇒ `fast_forward_ref` move; `ExportOutcome{branch,git_oid,before_oid}` |
| `export_git_side_ff` | changeset-side miss + `is_ancestor(old,minted)` ⇒ ff (dead-link/alias-twin heal) |
| `export_unlinked_never_clobbered` | unlinked non-ancestor old ⇒ `RefConflict{actual:None}` EVEN under `force` |
| `export_linked_nonff_refused` | linked non-ancestor old + `!force` ⇒ `RefConflict{actual:Some(old_cs)}` |
| `export_linked_force` | linked non-ancestor + `force` ⇒ `force_update_ref` proceeds |
| `export_hook_denied` | move `Denied` ⇒ `Forbidden{verdicts:{reasons:["push_hook"]}}`; stderr absent |
| `export_move_conflict` | move `Conflict` ⇒ `RefConflict{actual: re-read forward link Option<ChangesetId>}` |
| `export_checker_verdicts` | non-empty `ProtectionBlockReasons` ⇒ `Forbidden{verdicts}` verbatim; `Err` ⇒ `Internal` |
| `export_actor_capability` | `actor_capability` observed at BOTH `checker.check` AND the ref-move argv (mock records) |
| `export_gc_heal` | `linked:` anchor whose oid fails `verify_commit_oid` ⇒ treated unlinked, re-minted |
| `export_ident_restore` | imported changeset exports with RAW sidecar idents; native ⇒ `author→both` + `committed_at` UTC |
| `export_tree_base` | unchanged entries reuse parents[0] git-tree oids; merge-parents contribute no diff base |
| `export_refname_oracle` | `refs/heads/x.lock`-style bad refname ⇒ `InvalidInput` via `check-ref-format` |
| `export_chain_cap` | export-set BFS > `export_chain_max` ⇒ `TooLarge` |

## Coverage matrix — refusal inventory (SR6.7 — one arm each, all assert ZERO changeset rows)

| Test | Asserts |
|------|---------|
| `refuse_parents_64` | >64-parent commit ⇒ `InvalidInput` |
| `refuse_message_len` | >65_535-byte message ⇒ `InvalidInput` |
| `refuse_timestamp` | unrepresentable/out-of-range `rfc3339(unix)` ⇒ `InvalidInput` |
| `refuse_path_len` | >512-char path ⇒ `InvalidInput` |
| `refuse_path_dotdot` | `..` path ⇒ `InvalidInput` |
| `refuse_path_control` | control-byte path ⇒ `InvalidInput` |
| `refuse_author_angles` | sanitized author containing `<`/`>` ⇒ `InvalidInput` |
| `refuse_author_empty` | empty post-sanitize author ⇒ `InvalidInput` |
| `refuse_author_composed` | composed `Name <email>` >255 chars ⇒ `InvalidInput` |
| `sanitize_order` | `{control,0x7f}→'?'` runs BEFORE the refuse check (a `<` hidden behind a control byte still refuses) |

## Coverage matrix — `is_skipped` entry gates (SR6.9, fail-closed)

| Test | Asserts |
|------|---------|
| `skipped_import_chain` | `import_chain` on `Database::skipped()` ⇒ `DbNotConfigured` before argv/claim/facades |
| `skipped_lazy_import` | `lazy_import` same ⇒ `DbNotConfigured` |
| `skipped_export` | `export` same ⇒ `DbNotConfigured` |

## Coverage matrix — error bridge (one arm per consumed enum)

| Test | Asserts |
|------|---------|
| `bridge_git_denied` | `GitError::Denied` ⇒ `Forbidden{push_hook}` |
| `bridge_git_conflict` | `GitError::Conflict` ⇒ `RefConflict` |
| `bridge_git_notfound` | `GitError::NotFound` ⇒ `NotFound` |
| `bridge_git_invalidarg` | `GitError::InvalidArg` ⇒ `InvalidInput` |
| `bridge_git_sentinel` | `Process("oxidean:stdout-bound-exceeded")` ⇒ `TooLarge` |
| `bridge_git_process_other` | `Process(other)` ⇒ `Internal` fixed message |
| `bridge_git_io` | `Io` ⇒ `Internal` |
| `bridge_index_cursor` | `IndexError::CursorMalformed` ⇒ `Internal` (NOT `ImportState` — u5 parity) |
| `bridge_index_frontier` | `IndexError::FrontierTooLarge` ⇒ `TooLarge` |
| `bridge_index_db` | `IndexError::DbNotConfigured` ⇒ `DbNotConfigured` |
| `bridge_store_quota` | `ObjectStoreError::QuotaExceeded(scope)` ⇒ scope preserved |
| `bridge_store_db` / `bridge_store_others` | `DbNotConfigured` ⇒ same; other 7 ⇒ `Internal` |
| `bridge_changeset_*` | `ParentNotFound|ChangesetNotFound`⇒`NotFound`; `InvalidInput`; `ObjectNotFound|Internal`⇒`Internal`; `DbNotConfigured` |
| `bridge_checker_err` | checker `Err(String)` ⇒ `Internal` opaque |
| `bridge_facade_string` | facade `String` ⇒ `Internal` opaque |
| `malformed_id_no_echo` | raw id absent from `"changeset id: malformed"` |

## Coverage matrix — caps (constructor-injected small `InteropCaps`)

| Test | Asserts |
|------|---------|
| `cap_validate_blob_vs_object` | `*_blob_max > objects.config.max_object_bytes` ⇒ `InvalidInput` at `new` |
| `cap_validate_usize_max` | `*_max > usize::MAX / 2048` ⇒ `InvalidInput` (derived-bound multiplier can't wrap) |
| `cap_validate_zero` | zero cap ⇒ `InvalidInput` |
| `cap_rev_list` | `rev_list` emitted set > `rev_list_max` ⇒ `TooLarge` |
| `cap_tree_flat` | `tree_flat` leaves > `import_tree_max` ⇒ `TooLarge` |
| `cap_chain` | export BFS > `export_chain_max` ⇒ `TooLarge` |
| `cap_probe` | `is_ancestor_cs` visited > `export_ancestor_probe_max` ⇒ `TooLarge` |
| `cap_export_blob` | `objects.get` past `export_blob_max+1` ⇒ `TooLarge` |

## Coverage matrix — `oxidean-git` cli.rs additions (same-crate in-file)

| Test | Asserts |
|------|---------|
| `bounded_reader_kill` | stdout past `max_bytes` ⇒ child killed + `Process("oxidean:stdout-bound-exceeded")` |
| `name_status_z_rename` | `-z` R-row emits `R<score>\0src\0dst\0` — src-first order asserted |
| `tree_flat_max` / `name_status_max` | `max_bytes` honored → kill path |
| `write_object_commit_roundtrip` | `write_object` + `write_commit` (`mktree -z`, `{:06o}` modes, stdin message) → `cat_commit` verifies |
| `check_refname_oracle` | `a//b`, `x.lock`, `a@{b}` ⇒ `false`; normal ⇒ `true` |
| `verify_commit_oid_peel` | annotated tag → commit oid; tag→tree ⇒ `None` |
| `rev_list_parents_first` | children-first `--topo-order` reversed in-crate ⇒ parents-first `oids`; `truncated` data |
| `cat_commit_folded` | `gpgsig`/`mergetag` continuation headers skipped; last-`<`/`>` ident split; malformed ⇒ typed error |
| `move_actor_capability` | `fast_forward_ref`/`force_update_ref` pass `actor_capability` through (existing callers literal `"admin"`) |
| `classify_shared` | `Denied`/`Conflict` typed on BOTH move methods AND `commit_files` |

## Coverage matrix — `oxidean-db` + `oxidean-core` deltas

| Test | Asserts |
|------|---------|
| `dialect_null_covering` | `Some()`-wrap writes + `None` writes round-trip `Option::None` decode (sqlite arm in `dialect_vcs`; pg/mysql parity via docker) |
| `migration_0055_parity` | filename-set parity across the three dialect dirs; sqlite rebuild keeps `idx_vcs_derived_index_covering` |
| `dto_head_option` | `VcsGitImportHead.changeset_id` absent-key ⇒ `None` serde |
| `dto_skipped_tips` | `VcsGitImportResponse.skipped_tips` serde round-trip |
