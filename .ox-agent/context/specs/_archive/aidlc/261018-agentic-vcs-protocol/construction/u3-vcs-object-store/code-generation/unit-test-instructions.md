# Unit Test Instructions — u3-vcs-object-store

All tests live in `crates/oxidean-vcs/src/object_store/{mod,config,paths,gc}.rs` under `#[cfg(all(test, feature = "server"))]`, plus facade tests in `crates/oxidean-db/src/vcs.rs` `#[cfg(test)]`. Real sqlite `Database` (existing test fixture) + `tempfile` object dirs. Zero mocks — the facade contract is exercised against real schema.

## Exact command (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --features server          # primary
cargo test -p oxidean-vcs --features server                 # fallback arm
cargo nextest run -p oxidean-db -E 'test(vcs_)'             # facade additions
```

## Coverage matrix

| Test | Asserts |
|------|---------|
| `validate_hash_accepts_bare_hex` | 64 lowercase-hex ok; rejects len!=64, uppercase, non-hex, `../`, `sha256:`-prefixed — before any path join |
| `shard_path_layout` | `{dir}/{h0:2}/{h2:4}/{h}` shape; tmp under `{dir}/tmp/` |
| `new_refuses_regular_file` | objects_dir is a file → `Config` |
| `new_refuses_foreign_dir` | dir with a non-marker file + no marker → `Config`; marker added → `new` succeeds |
| `new_allows_fresh_and_lostfound` | absent dir creates marker+tmp; dir containing only `lost+found` treated fresh |
| `new_marker_before_tmp` | crash-ordering invariant: marker exists whenever tmp does |
| `put_roundtrip_dedup` | stream put → Stored + bare 64-hex hash; bytes land at shard path; second put same content → Dedup, same hash |
| `put_empty_object` | empty stream → sha256-of-empty `size=0` object Stored; `repo_quota_bytes=0` still admits it (carve-out) |
| `put_object_quota_aborts_tmp` | stream > max_object_bytes → `QuotaExceeded{Object}`, tmp file gone |
| `put_repo_quota_denies_link_keeps_object` | storage+size>repo_quota → `QuotaExceeded{Repo}`; new-object PUT runs eager cleanup (row+file gone when conditional delete returns true); object put by another repo already linked keeps the bytes |
| `put_rereference_no_quota_charge` | repo B links object already linked by repo A → no count_for_repo denial path |
| `get_streams_and_404s` | known hash returns readable bytes; unknown/vanished → `NotFound`; malformed hash → `InvalidHash` |
| `verify_detects_corruption` | verify ok on good bytes; byte-flipped file → `Internal` |
| `skipped_db_fails_closed` | put/get/gc_once on a skipped-DB `Database` → `DbNotConfigured` |
| `gc_absent_dir_zero_stats` | `gc_once` with absent objects_dir → `Ok` zero stats, `skipped=false` |
| `gc_foreign_dir_marker_gate` | non-empty markerless dir → `Config`, no files touched |
| `gc_lock_skip_and_stale` | live `.gc-lock` → `skipped=true` stats; mtime older than grace → stale lock replaced, pass runs |
| `gc_lock_guard_drops` | guard drop removes `.gc-lock`; heartbeat `set_modified` does not truncate content |
| `gc_orphans_row_then_file` | orphan rows older than grace: conditional delete true ⇒ file removed + counters; delete false (racing link/file_changes ref) ⇒ file kept, not an error |
| `gc_rowless_sweep` | shape-matching file, no row, mtime>grace → removed + `rowless_removed`; newer mtime kept; non-shape names (`.gc-lock`, marker, `lost+found`, dotfiles) untouched; symlink not followed |
| `gc_tmp_sweep` | `*.tmp` older than grace removed; probe `Err` on row-less candidate → file kept + `errors+=1` |
| `from_env_ranges` | grace `0` and `>i64::MAX/1000` → `Config`; interval `>i64::MAX` → `Config`; byte caps `>i64::MAX` → `Config`; sibling-dir equality with resolved `OXIDEAN_LFS_DIR`/`OXIDEAN_PACKAGES_DIR` → `Config` |
| `vcs_get_object_link_bool` | absent→false; link inserted→true |
| `vcs_delete_object_if_unlinked` | unlinked row → true + row gone; row with link → false + row kept; row referenced by `vcs_file_changes.blob_hash` → false; absent hash → false |

## Gaps carried forward

- pg/mysql facade paths probed only by `db-dialects` (house precedent — recorded debt).
- Concurrency races (R1-R3, R8) are residual-risk register items, not testable units.
- Rust coverage collected-not-gated for the server suite until U7 feature unification.
