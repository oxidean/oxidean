# Unit Test Instructions — u5-vcs-merge-engine

All crate tests live in `crates/oxidean-vcs/src/merge_engine.rs` under `#[cfg(all(test, feature = "server"))]` — in-file module per u3/u4 precedent. Seed state through the stores (`ObjectStore::put`, `ChangesetStore::create`, `RefRegistry::cas` → facade `vcs_cas_ref_update`) against `testutil::test_db()` — file-backed sqlite `(TempDir, Database)`; the `TempDir` MUST stay alive for the test's scope (`let (_dir, db) = ...`). Zero mocks; `Database::skipped()` covers `DbNotConfigured`. MergeCaps are constructor-injected small for cap tests.

## Exact command (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --features server          # primary
cargo test -p oxidean-vcs --features server                 # fallback arm
```

## Coverage matrix — merge

| Test | Asserts |
|------|---------|
| `fast_forward_b` | `is_ancestor(a,b)` → `FastForward{head:b}`; zero new rows (`vcs_get_changeset`/tree probes) |
| `fast_forward_a` | `is_ancestor(b,a)` → `FastForward{head:a}`; zero new rows |
| `equal_heads` | `a == b` → `FastForward`; zero new rows |
| `clean_merge` | divergent clean sides → `Merged{id}`, `parents:[a,b]`, manifest relative to `a` |
| `merge_idempotent` | same inputs → same changeset id (content-derived canonical id) |
| `merge_message_default` | `message:None` → `"merge {b} into {a}"`; `Some` verbatim |
| `no_common_ancestor` | disjoint histories → `MergeError::NoCommonAncestor` |
| `conflict_content` | merge3 regions → `Conflict` entries kind `content` |
| `conflict_binary` | NUL-in-8KiB side diverging → `binary` |
| `conflict_mode` | exec-bit divergence → `mode` |
| `conflict_type` | file↔symlink → `type` |
| `conflict_delete` | delete-vs-modify → `delete` on the path |
| `conflict_rename_rename` | both sides rename `p` to different targets → `rename` on claimed targets; per-path re-emission suppressed |
| `conflict_rename_delete` | `old` renamed one side, absent other while `∈ b_t` → `rename` on renaming side's target |
| `conflict_rename_modify` | stays `delete` kind on `old`, `detail` names rename target |
| `conflict_order_and_cap` | entries path-sorted (`BTreeMap`); `merge_conflict_max`+1 collected → `truncated` flag |
| `detail_truncated_200` | path-bearing `detail` cut at 200 chars on char boundary |
| `zero_write_on_conflict` | conflict path: `vcs_get_changeset` no new row + `vcs_list_file_changes` empty + `vcs_get_object_link` on pass-1 mint absent |
| `zero_write_on_abort` | bad author/message (entry check) → `{no changeset, no file_changes}` (no mint exists — entry pre-check precedes pass-1) |

## Coverage matrix — provenance

| Test | Asserts |
|------|---------|
| `rename_chain_compose` | `A→B→C` transitive compose → `prov[A]=C` |
| `rename_multiwriter_drop` | two writers of same `old` → dropped (no provenance) |
| `rename_cycle_drop` | `A→B→A` cycle → dropped |
| `rename_tip_filter` | provenance kept only when final target ∈ side's tip tree |
| `rename_manifest_emission` | emitted only when `prov[old]==p` ∧ `old ∉ merged` ∧ `old ∈ first_tree` ∧ `(mode,blob)` equal |
| `rename_byte_first_old` | multi-source same-target → byte-first `old` wins |
| `rename_delete_skip` | emitted-rename `old` never also emits `delete{old}` (no phantom path_touch row) |

## Coverage matrix — rebase

| Test | Asserts |
|------|---------|
| `rebase_root` | `parents(cs)` empty → ∅ base, `full_tree(None)` without `tree()` call |
| `rebase_noop` | `new_parent == old_base` → `FastForward{cs}` |
| `rebase_descendant_cycle` | `is_ancestor(cs,new_parent)` → `InvalidInput` |
| `rebase_ancestor_rollback` | rebasing onto an ancestor is legal |
| `rebase_disjoint_target` | disjoint target legal (BR5.14) |
| `rebase_merge_node` | merge-node source replays vs `parents[0]` only; output `parents:[new_parent]` |
| `rebase_message_default` | `message:None` → source `cs.message` verbatim |
| `rebase_provenance_source_only` | provenance from `vcs_list_file_changes(cs)` — never a target-side walk |

## Coverage matrix — merge3 / paging / caps

| Test | Asserts |
|------|---------|
| `merge3_trailing_newline` | trailing-`\n`-only divergence → `content` conflict (last-line `(bytes,ends_nl)` pair) |
| `merge3_crlf` | `\r\n` preserved verbatim in emitted bytes |
| `merge3_terminators` | `line + b"\n"` iff `i < len-1 || ends_nl` — byte-faithful reconstruction |
| `merge3_line_cap` | `(n_base + n_side + 2) > LINE_CAP` → `TooLarge` (unterminated-buffer +2 bound) |
| `merge3_spawn_blocking_probe` | `#[tokio::test(flavor="current_thread")]` — oneshot probe spawned immediately before awaiting private `run_merge3` handle, `try_recv` after return; payload outlasts first-poll completion (BR5.10 detector) |
| `full_tree_paging` | >500-entry fixture exercises cursor loop via `.as_deref()` |
| `cap_blob` | blob > `merge_blob_max` → `TooLarge` |
| `cap_paths_walk` | `full_tree` mid-walk abort at `merge_paths_max` |
| `cap_paths_manifest` | emitted manifest > `merge_paths_max` → `TooLarge` |
| `cap_ancestor_probe` | mark BFS > `ancestor_probe_max` → `TooLarge` |
| `cap_side_walk` | side-exclusive walk > `side_walk_max` → `TooLarge` |
| `cap_rename_probe` | collected rename rows > `rename_probe_max` → `TooLarge` |
| `caps_validate_blob_vs_object` | `merge_blob_max > max_object_bytes` → `InvalidInput` at `new` |
| `caps_validate_u64_max` | `merge_blob_max == u64::MAX` → `InvalidInput` (`take(max+1)` overflow guard) |
| `caps_validate_paths_parity` | `merge_paths_max > 10_000` → `InvalidInput` (Q9 client-parity) |

## Coverage matrix — error bridge

| Test | Asserts |
|------|---------|
| `bridge_cursor_malformed` | `IndexError::CursorMalformed` → `Internal` |
| `bridge_frontier` | `IndexError::FrontierTooLarge` → `TooLarge` |
| `bridge_base_not_found` | `IndexError::BaseNotFound` → `NoCommonAncestor` |
| `bridge_parent_not_found` | `ChangesetError::{ParentNotFound,ChangesetNotFound}` → `NotFound` |
| `bridge_object_not_found` | `ChangesetError::ObjectNotFound` → `Internal` |
| `bridge_quota_scope` | `ObjectStoreError::QuotaExceeded(scope)` → `QuotaExceeded(scope)` — scope preserved (Field/Object/Repo arms) |
| `bridge_db_not_configured` | `is_skipped` database → `DbNotConfigured` on both entry points |
| `malformed_id_no_echo` | bad id → `InvalidInput("changeset id: malformed")` fixed text; raw id absent from message |
| `conflict_display_count_only` | `MergeError::Conflict(..)` Display == `"conflict: {n} paths"` — paths absent |
