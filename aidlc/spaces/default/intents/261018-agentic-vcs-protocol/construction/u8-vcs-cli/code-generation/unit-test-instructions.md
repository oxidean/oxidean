# Unit Test Instructions — u8-vcs-cli

Rust colocated tests. Client-side suites run under the `client` feature with a scripted fake `VcsTransport` (in-memory procedure→response map + object-store map + call recorder); `WorkingCopy` tests use `std::env::temp_dir()`-unique dirs and real filesystem materialization. `oxidean-cli` glue tests follow the existing `args.rs`/`commands.rs` `#[cfg(test)]` conventions.

## Exact commands (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --no-default-features --features client   # client-only feature compile + tests
cargo nextest run -p oxidean-vcs --features server                        # combined feature gate (server ⊃ client)
cargo nextest run -p oxidean-cli                                          # argv glue / parse / exit-code suites
cargo nextest run --workspace                                             # checkpoint command
cargo clippy -p oxidean-vcs --all-features -- -D warnings                   # lint both feature gates
cargo fmt --check
cargo check -p oxidean-cli                                                # thin-glue compile proof
```

## Coverage matrix — `ObjectCache` (`oxidean-vcs/src/client/object_cache.rs` tests)

| Test | Asserts |
|------|---------|
| `round_trip` | put→get returns bytes; second `get` counts a hit |
| `verify_on_read_corrupt_is_miss` | overwrite the cache file with garbage ⇒ `get` → miss + entry evicted (AC6.2.2) |
| `put_hash_mismatch_refuses` | `put(hash, wrong_bytes)` ⇒ `CliFault::Cache`, nothing written (AC3.3.3) |
| `bounded_eviction` | `max_bytes` cap set small ⇒ inserting past bound evicts oldest-mtime until ≤ bound; inserted newest survives |
| `shared_root` | two cache handles on one root see each other's entries (cross-repo sharing, AC6.2.1) |
| `env_override_dir` | `OXIDEAN_VCS_CACHE_DIR` respected |

## Coverage matrix — `WorkingCopy` (`oxidean-vcs/src/client/working_copy.rs` tests)

| Test | Asserts |
|------|---------|
| `full_checkout` | empty filter ⇒ all tree entries materialize; `files_written=N`, `skipped_outside_filter=0` |
| `sparse_selection_no_extra_fetch` | `--paths` filter ⇒ only matching entries written; `object_get` called only for selected blobs (call-count assertion, AC3.4.1); exact-path AND dir-prefix forms match |
| `paged_tree` | multi-page `vcs.tree` cursor loop accumulates all entries |
| `second_run_zero_fetch` | same changeset + warm cache ⇒ `blobs_fetched=0`, `cache_hits=N` (AC6.2.1) |
| `corrupt_cache_refetches` | corrupt one entry ⇒ that blob refetched, rest cached (AC6.2.2) |
| `fetch_hash_mismatch_fault` | server serves bytes whose sha256 ≠ wanted hash ⇒ `CliFault`, file not materialized (AC3.3.3/AC6.1.2) |
| `non_empty_target_refused` | pre-filled dir without marker ⇒ `CliFault::Fs`, nothing written (AC6.1.3) |
| `recheckout_into_marked_dir` | marker present ⇒ files overwritten, `files_written` counts again |
| `exec_bit_and_symlink` | unix: `executable` ⇒ `0o755` + x bit; `symlink` entry ⇒ real symlink w/ blob-decoded target (AC6.3.1 note: Windows degradation documented) |
| `temp_write_rename` | no `.tmp` litter on success; crashed-writes can't be observed mid-file (verify only complete files land — rename used, not create+write) |
| `checkout_result_counters` | mixed run ⇒ all four counters correct |

## Coverage matrix — `CliSurface` (`oxidean-vcs/src/client/cli.rs` tests)

| Test | Asserts |
|------|---------|
| `enable_disable` | `--disable` ⇒ `{enabled:false}`; default ⇒ `{enabled:true}` (AC8.4.1) |
| `request_shaping_per_command` | every command emits exactly the C1 wire input (`{owner,name,…}` first) — table-driven over all 16 |
| `error_envelope_passthrough` | `{ok:false,code:"vcs.disabled"}` ⇒ envelope returned verbatim in `CliOutcome`, not rewritten |
| `commit_inline_threshold` | ≤262144 B file ⇒ `content_b64` in `changes[]`; 262145 B ⇒ `object_put` called + `blob:` ref emitted |
| `commit_empty_changes_usage` | zero `--add/--delete/--rename` ⇒ `CliFault::Usage` |
| `commit_parents_default_root` | no `--parent` ⇒ `parents:[]` |
| `ref_set_cas_mapping` | `--create` ⇒ `expected_old:null`; `--delete` ⇒ `new:null`; `--expect X --to Y` ⇒ `{expected_old:X,new:Y}`; bare `ref set` ⇒ Usage |
| `cat_too_large_object_fallback` | `file_get` ⇒ `{too_large:true}` ⇒ `object_get(blob)` path taken, bytes to stdout channel |
| `conflict_pretty_channel` | `vcs.conflict` `{data:{conflicts:[…]}}` ⇒ pretty/stderr lists `{path,kind}` entries |
| `log_from_defaults_main` | no `--from` ⇒ `{from:"main"}` |
| `import_cursor_passthrough` | `--cursor` ⇒ forwarded verbatim |

## Coverage matrix — `oxidean-cli` glue

| Test | Asserts |
|------|---------|
| `args_parse_all_subs` | `ox vcs <sub>` × 16 parse to the right variants incl. `ref list`/`ref set`, `--expect`/`--to`/`--delete`/`--create`, `--paths` repeatable |
| `required_procedures_rows` | each sub ⇒ its `vcs.*` procedure; `ref set` ⇒ `vcs.ref_update` |
| `cli_procs_vcs_complete` | `CLI_PROCS` covers every called `vcs.*`; fail-closed test at commands.rs:3048 stays green |
| `error_exit_code_vcs_auth` | `vcs.unauthenticated`, `vcs.forbidden` ⇒ 3; `vcs.disabled` stays 1 |
| `usage_rows` | `USAGE` + `help_topic("vcs", _)` rows exist per sub |

## Coverage matrix — feature discipline (structural)

| Test | Asserts |
|------|---------|
| `client_feature_slim` | `cargo check -p oxidean-vcs --no-default-features --features client` compiles; `cargo tree` shows no `sqlx`/`oxidean-git`/`oxidean-db` edge into `oxidean-cli` (manual gate — documented in code-summary) |
