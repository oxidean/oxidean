# Logical Components — u3-vcs-object-store

## LC3.1 — `oxidean-vcs` crate layout

```
src/lib.rs            — existing doc header + `#[cfg(feature = "server")] pub mod object_store;`
src/object_store/
  mod.rs              — ObjectStore, PutOutcome, ObjectStoreError, VcsGcStats
  config.rs           — ObjectStoreConfig + from_env (SR3.9 ranges + SD3.9-R10 upper bounds)
  paths.rs            — validate_hash, shard_path, tmp naming (pid/counter/nanos)
  gc.rs               — gc_once, clear_stale_lock, LockGuard (release-on-drop + heartbeat)
Cargo.toml            — [features] default=["client"], server=[dep:...]; per-crate dep pins (TD3.2)
```

## LC3.2 — `ObjectStore`

- `new(db: Database, config: ObjectStoreConfig) -> Result<Self>` — ordered: dir-is-regular-file ⇒ `Config` → non-empty (exempting `.oxidean-vcs-objects`, `lost+found`) && no marker ⇒ `Config` → `create_dir_all(dir)` → write `.oxidean-vcs-objects` marker → `create_dir_all(tmp)`. Zero writes before the refusal; marker precedes `tmp/` so a crash can't leave a marker-less tmp-only dir that refuses forever.
- `put<S,E>(stream, repo_id) -> Result<PutOutcome>` where `S: Stream<Item = Result<Bytes, E>> + Unpin` and `E: Display` (the bound `Stream(String)` construction needs — LFS store.rs:43). SD3.3 ordering.
- `get(hash) -> Result<tokio::fs::File>` — validate → row check via `vcs_get_object` → open; io::NotFound ⇒ `NotFound`.
- `verify(hash) -> Result<()>` — stream + re-hash compare; mismatch ⇒ `Internal`.
- `gc_once() -> Result<VcsGcStats>` — SD3.4/SD3.5; stats `{orphans_removed, rowless_removed, tmp_removed, bytes_reclaimed, skipped: bool, errors: u32}`.
- `clear_stale_lock()` — unconditional lockfile remove (boot hook; single-node pin).

## LC3.3 — `ObjectStoreError` (closed enum)

`QuotaExceeded{Object|Repo}` / `NotFound` / `InvalidHash` / `Stream(String)` / `Config(String)` / `DbNotConfigured` / `Db(String)` / `Io(std::io::Error)` / `Internal(String)` — `Io` carries the source error for host diagnostics; `Internal` carries a short detail string (e.g. verify-mismatch note — no content, no paths).

- `Config` — construct/boot refusal (env parse, dir-is-file, foreign-dir marker gate, sibling-dir collision). Never wire-mapped: U7 surfaces it as a startup refusal, not an RPC error.
- `From<io::Error>` branches `ErrorKind::NotFound ⇒ NotFound`, else `Io`.
- `From<String> ⇒ Db` exists ONLY for the facade `Result<_,String>` surface; `Stream` is explicit-construction-only (a String helper must not auto-mislabel as `Db`).
- `From<VarError> ⇒ Config`.
- Wire mapping table lives in U7; the enum is the contract (TD3.3/TD3.5).

## LC3.4 — `oxidean-db` sanctioned delta (BR3.11)

- `vcs_get_object_link(repo_id, hash) -> Result<bool>` — `has_lfs_link` shape (lfs.rs:230) with the **vcs** column name: `SELECT 1 AS ok FROM vcs_object_links WHERE repo_id = ? AND hash = ?` + `fetch_optional` + `Ok(row.is_some())`.
- `vcs_delete_object_if_unlinked(hash) -> Result<bool>` — `DELETE FROM vcs_objects WHERE hash = ? AND NOT EXISTS(SELECT 1 FROM vcs_object_links WHERE hash = ?) AND NOT EXISTS(SELECT 1 FROM vcs_file_changes WHERE blob_hash = ?)` → `rows_affected() > 0`. The `vcs_file_changes` clause keeps `false` = "protected by manifest FK" distinct from `Err` = genuine failure (no per-pass warn spam on the U4-invariant edge). Dialect-portable: MySQL's same-table-subquery ban doesn't apply (subqueries target other tables); `idx_vcs_object_links_hash` + `idx_vcs_file_changes_blob` serve them.

## LC3.5 — Consumed facade surface (unchanged)

`vcs_get_object`, `vcs_insert_object_if_absent` (bool), `vcs_insert_object_link_if_absent` (bool), `vcs_repo_object_counts`, `vcs_list_orphaned_objects_before` — plus `vcs_create_changeset_tx`'s `object_links` param (the C6a mechanism U4 drives).

## LC3.6 — Build/test surfaces (TD3.7)

`Makefile` deltas (claimed in source-manifest; CI-lane counterparts land in this unit's infrastructure-design cicd-pipeline.md):

- `test` lane (Makefile:237-243): add `cargo nextest run -p oxidean-vcs --features server` to the nextest arm AND `cargo test -p oxidean-vcs --features server` to the `cargo test` fallback arm — non-nextest environments must not silently run zero U3 tests.
- `dead-code-check` (Makefile:405 — the workspace clippy lives here; there is no `lint` target): add `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings` — same `-D warnings` parity as the workspace invocation, or the gated module can warn-but-pass.
- `docs-check` (Makefile:410): add `RUSTDOCFLAGS="-D warnings" cargo doc -p oxidean-vcs --no-deps --features server` — workspace `cargo doc` is default-features and never sees the gated module.
- Doc tests: nextest never runs them and `cargo test --doc` is default-features — pin **no doc examples** in the server module rather than adding a doc-test lane.
- infrastructure-design note: `cargo nextest archive --workspace` (ci.yml:207) is also default-features — the api-rust shard needs a dedicated `-p oxidean-vcs --features server` run, not just a Makefile line. Also record the feature-unification nuance: these `-p` invocations are load-bearing only until U7 adds the `server` dep edge (resolver-2 unification then compiles the gated code workspace-wide anyway — keep them belt-and-suspenders).

Tests under `#[cfg(all(test, feature = "server"))]` — real sqlite `Database` + `tempfile` dirs (TD3.7 matrix).

## Dependency edges

- `object_store` depends on: `oxidean-db` (facade + Database), `sha2`, `tokio`, `futures-util`, `bytes`, `tracing`, `chrono`; dev-dep `tempfile`. **No `oxidean-core` dep in U3** — nothing consumed yet (error codes/wire mapping live at U7's boundary); declaring it unused would trip `cargo machete`. Later units add it when a real use lands.
- `lib.rs` never names `object_store` unconditionally — the `#[cfg(feature="server")]` gate keeps client builds dep-free.
- Downstream consumers (U4 links invariant, U7 routes/jobs/boot) see only `ObjectStore`'s pub API + `PutOutcome`/`VcsGcStats`.
