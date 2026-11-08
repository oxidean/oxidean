# Unit Test Instructions — u7-vcs-api-surface

VCS-side tests live in `crates/oxidean-vcs/src/api.rs` / `activity.rs` under `#[cfg(all(test, feature = "server"))]` — in-file modules per u3–u6 precedent. Seed state through `testutil::test_db()` → `(TempDir, Database)` (file-backed sqlite; `TempDir` held alive; never `:memory:`) + real `ObjectStore`/`ChangesetStore`/`ReadIndex`/`RefRegistry`/`MergeEngine` instances + a mock `GitInterop` seam for lazy-trigger tests. Host-side tests live in-file in `crates/oxidean-api/src/vcs.rs`/`vcs_routes.rs`/`vcs_protection.rs` following the existing api test conventions (sqlite `test_db`, in-memory `RpcCtx`, axum `oneshot` route tests where existing route tests do). `Database::skipped()` covers `db.not_configured`.

## Exact commands (runnable before first test step)

```bash
cargo nextest run -p oxidean-vcs --features server          # primary — U7 api/activity suite
cargo nextest run -p oxidean-api                          # host dispatch/routes/activity/mcp
cargo test -p oxidean-vcs --features server               # fallback arm
cargo nextest run --workspace                             # full regression
```

## Coverage matrix — flag gate (`VcsApi::flag`)

| Test | Asserts |
|------|---------|
| `gate_absent_row_denies_status` | no flag row ⇒ `vcs.status` ⇒ `Disabled` (never-enabled denies ALL) |
| `gate_absent_row_denies_write` | no flag row ⇒ write-class ⇒ `Disabled` |
| `gate_disabled_denies_write` | `enabled:false` ⇒ `changeset_create`/`ref_update`/`git_export` ⇒ `Disabled` |
| `gate_disabled_serves_reads` | `enabled:false` ⇒ `status`/`log`/`tree`/objects-GET path ⇒ served |
| `gate_enabled_passes` | `enabled:true` ⇒ read + write proceed |
| `gate_write_vs_read_table` | per-procedure class check matches proc_classes tiers exactly (10 read / 6 write / 1 admin) |

## Coverage matrix — `repo_enable` / `status`

| Test | Asserts |
|------|---------|
| `enable_upserts_flag` | `repo_enable` ⇒ `vcs_repo_surface_flags.enabled=true` + `enabled_by` actor recorded |
| `enable_disable_roundtrip` | enable→disable→enable transitions persist; disabled state gates writes after |
| `status_counts` | `vcs.status` returns flag + `changeset_count` + `{object_count, storage_bytes}` accurate to seeded rows |
| `enable_activity_row` | successful enable writes `vcs.repo_enable` activity row with all-null push columns |

## Coverage matrix — read procedures (`log`/`tree`/`diff`/`blame`/`path_history`/`merge_base`/`file_get`/`changeset_get`/`ref_list`)

| Test | Asserts |
|------|---------|
| `read_log_chain` | seeded chain ⇒ `vcs.log` returns parents-first entries with ids/messages |
| `read_tree_entries` | `vcs.tree` returns seeded tree rows; `at` resolves head |
| `read_at_main_default` | omitted `at` ⇒ `main` ref tip resolution |
| `read_at_sole_head` | omitted `at` + exactly one non-main ref ⇒ sole-head resolution |
| `read_at_none_found` | omitted `at` + zero refs ⇒ `vcs.not_found` |
| `read_at_ambiguous` | omitted `at` + ≥2 refs w/o `main` ⇒ `vcs.invalid_input` |
| `read_at_explicit` | explicit `at` changeset id resolves that commit's view |
| `read_merge_base_none` | unrelated roots ⇒ `vcs.no_common_ancestor` (AC4.2.2) |
| `read_not_found_codes` | unknown changeset/path/ref ⇒ `vcs.not_found`, never `Internal` |
| `lazy_import_retry_once` | 40-hex `at` miss ⇒ one `lazy_import` call ⇒ re-resolve succeeds; mock records single call |
| `lazy_import_not_found` | `lazy_import` `NotFound` ⇒ `vcs.not_found`; non-hex `at` never triggers lazy import |
| `lazy_import_no_interop` | `interop:None` ⇒ read miss ⇒ `vcs.not_found` plain (no panic/Internal) |

## Coverage matrix — write procedures (`changeset_create`/`merge`/`rebase`/`ref_update`/`git_import`/`git_export`)

| Test | Asserts |
|------|---------|
| `create_idempotent` | byte-identical resubmit ⇒ same changeset id (AC2.2.x) |
| `create_too_large` | >10_000 changes / >256 KiB inline ⇒ `vcs.too_large` |
| `merge_conflict_envelope` | conflicting merge ⇒ `vcs.conflict` + `data:{conflicts:[{path,kind,detail?}]}`, NEVER `ok:true` |
| `merge_conflict_cap` | conflicts >500 ⇒ truncated to `VCS_CONFLICT_DATA_MAX_ENTRIES` |
| `rebase_descendant_denied` | descendant/cycle target ⇒ `vcs.invalid_input` (AC2.4.3) |
| `ref_update_create` | `expected_old:null` ⇒ create-only; existing ref ⇒ `vcs.ref_conflict` |
| `ref_update_nonidempotent` | retried landed call ⇒ `vcs.ref_conflict{data:{actual}}` with stored value (AC5.1.3) |
| `ref_update_delete` | `new:null` ⇒ delete; missing ref ⇒ `vcs.not_found`/`ref_conflict` per spec |
| `git_import_passthrough` | args mapped to `interop.import_chain`; `ImportState` ⇒ `vcs.import_state`; cursor echoed |
| `git_export_forbidden` | protection denial ⇒ `vcs.forbidden` + `data:{verdicts}` verbatim (AC7.2.2) |
| `git_export_refconflict` | unlinked tip ⇒ `vcs.ref_conflict{data:{actual:null}}` |
| `write_activity_rows` | each mutating op writes exactly ONE activity row with the C4 column mapping |
| `write_no_activity_on_failure` | failed op ⇒ zero activity rows |

## Coverage matrix — error bridge (`VcsApiError` → `AppError`/`VcsErrorCode`)

| Test | Asserts |
|------|---------|
| `bridge_disabled` | `Disabled` ⇒ `vcs.disabled` |
| `bridge_refconflict_data` | `RefConflict{actual}` ⇒ `vcs.ref_conflict` + `data.actual` — `Option<ChangesetId>`, never a git oid |
| `bridge_conflict_cap` | conflicts payload capped at 500 entries |
| `bridge_quota_vs_toolarge` | `QuotaExceeded` ⇒ `vcs.quota_exceeded`; `TooLarge` ⇒ `vcs.too_large` — distinct codes |
| `bridge_import_state` | `ImportState` ⇒ `vcs.import_state` |
| `bridge_forbidden_verdicts` | `Forbidden{verdicts}` ⇒ `vcs.forbidden` + `VcsForbiddenData` verbatim |
| `bridge_db_not_configured` | `DbNotConfigured` ⇒ `db.not_configured` (shared code, NOT `vcs.*`) |
| `bridge_internal_opaque` | `Internal` ⇒ `rpc.internal` fixed message — no internals leaked |
| `bridge_interop_total` | every `InteropError` variant maps per the LC6.6 table verbatim |

## Coverage matrix — object routes (`vcs_routes.rs`)

| Test | Asserts |
|------|---------|
| `route_get_octet` | seeded object ⇒ `200 application/octet-stream` + `Cache-Control: immutable` + body bytes |
| `route_get_not_found` | unknown hash ⇒ `404 vcs.not_found` JSON envelope |
| `route_get_disabled_false_ok` | `enabled:false` repo ⇒ GET still serves (AC1.2.1) |
| `route_get_never_enabled` | no flag row ⇒ `403 vcs.disabled` |
| `route_post_returns_hash` | POST body ⇒ `{ok:true,data:{hash:"sha256:…"}}` matching content sha256 |
| `route_post_dedup` | re-POST same bytes ⇒ success no-op, one stored object |
| `route_post_quota` | body > `OXIDEAN_VCS_MAX_OBJECT_BYTES` ⇒ `413 vcs.quota_exceeded` typed (not Axum's untyped 413) |
| `route_cookie_denied` | cookie-only session ⇒ `401 vcs.unauthenticated` (AC8.5.1) |
| `route_pat_ok` | PAT Bearer w/ sufficient class ⇒ pass; insufficient ⇒ `403 vcs.forbidden` |
| `route_alias_not_in_procedures` | `vcs.object_get`/`vcs.object_put` classify but never appear in `PROCEDURES` |

## Coverage matrix — host wiring (`rpc.rs`/`activity.rs`/`app_state`)

| Test | Asserts |
|------|---------|
| `dispatch_real_arms` | each of the 17 `vcs.*` arms calls its handler (no stub `vcs.disabled` remains for enabled repos) |
| `dispatch_arm_parity` | `procedures_list_matches_dispatch` still green |
| `archived_write_denied` | archived repo + write-class ⇒ `repo.archived` BEFORE dispatch; reads pass |
| `valid_push_types_ext` | `VCS_ACTIVITY_KINDS` all accepted by `VALID_PUSH_TYPES`; git push types unchanged |
| `appstate_env_defaults` | unset `OXIDEAN_VCS_*` ⇒ frozen defaults; set vars honored |
| `appstate_bad_dir` | invalid objects dir ⇒ `vcs:None` + handlers degrade typed, no panic |
| `gc_job_spawn` | configured ⇒ job registered; `ObjectStore::gc` invoked with grace secs |
| `factory_reset_sweep` | objects dir appears in the sweep path |
| `mcp_class_gate` | `vcs.*` tool class via proc_classes; `SessionOnly` ⇒ denial |

## Edge / hygiene

| Test | Asserts |
|------|---------|
| `envelope_shape` | success `{ok:true,data}` / failure `{ok:false,error:{code,message,data?}}` on both RPC + routes |
| `messages_no_leak` | error messages carry field/reason only — no paths, blob bytes, auth material |
| `skipped_db` | `Database::skipped()` ⇒ `db.not_configured` before any store call on every public method |
