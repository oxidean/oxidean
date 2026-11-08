# Code Summary — u3-vcs-object-store

## Landed

| File | Change |
|------|--------|
| `crates/oxidean-vcs/Cargo.toml` | `[features] default = ["client"]`, `client = []`, `server = [dep:bytes, dep:chrono, dep:futures-util, dep:oxidean-db, dep:sha2, dep:tokio, dep:tracing]`; dev-deps `oxidean-core` (repo-seeding `Role`) + `tempfile` |
| `crates/oxidean-vcs/src/lib.rs` | `#[cfg(feature = "server")] pub mod object_store;` — client builds link zero `oxidean-db`/fs machinery |
| `crates/oxidean-vcs/src/object_store/paths.rs` | `validate_hash` (bare 64-lowerhex before ANY join), `shard_path` `{dir}/{h0:2}/{h2:4}/{h}`, `is_hex_shaped` sweep predicate, `tmp_path` `{pid}-{counter}-{nanos}.tmp`, `MARKER_FILE`/`GC_LOCK_FILE`/`TMP_DIR`/`LOST_FOUND` consts |
| `crates/oxidean-vcs/src/object_store/config.rs` | `ObjectStoreConfig` + `from_env` — 5 `OXIDEAN_VCS_*` vars, malformed ⇒ `Config` (never silent default), grace `[1, i64::MAX/1000]`, interval `[0, i64::MAX]`, byte caps `≤ i64::MAX`, resolved-dir equality refusal vs `OXIDEAN_LFS_DIR`/`OXIDEAN_PACKAGES_DIR` |
| `crates/oxidean-vcs/src/object_store/mod.rs` | `ObjectStore::{new, put, get, verify}`, `PutOutcome{bare hash, PutKind::Stored|Dedup}`, closed `ObjectStoreError` (QuotaExceeded{Object|Repo}/NotFound/InvalidHash/Stream/Config/DbNotConfigured/Db/Io/Internal), `VcsGcStats`, unix `0o600`/`0o700` permission pins |
| `crates/oxidean-vcs/src/object_store/gc.rs` | `gc_once` (absent-dir ⇒ zero stats → marker gate → `.gc-lock` → orphan-row pass → row-less sweep → tmp sweep), `clear_stale_lock`, `LockGuard` (sync remove on Drop), create-new lock + mtime staleness + `set_modified` heartbeat (no truncate) |
| `crates/oxidean-db/src/vcs.rs` | `vcs_get_object_link` (`SELECT 1 AS ok` membership probe → `bool`) + `vcs_delete_object_if_unlinked` (conditional DELETE, `NOT EXISTS` on links AND `file_changes.blob_hash`, `rows_affected>0`) — the two sanctioned additions only; zero migrations |
| `.github/workflows/ci.yml` | `cargo nextest archive --workspace --features server`; new `vcs-server` leg (`-E 'package(oxidean-vcs)'` on the shared archive); `vcs-server` added to `rust-artifacts-cleanup` needs; `dead-code` job gains the `--all-features` clippy + `--features server` rustdoc lines |
| `Makefile` | `test` nextest arm + `cargo test` fallback arm; `dead-code-check` clippy line; `docs-check` rustdoc line |
| `docs/TESTING.md` | `vcs-server` row + `rust-build` row updated for `--features server` |

## Pinned decisions implemented

- **`new()` ordering** (SD3.8): regular-file ⇒ `Config` → markerless non-empty (exempting marker + `lost+found`) ⇒ `Config` → `create_dir_all` → marker write → `create_dir_all(tmp)`. Zero writes before refusal; marker before `tmp/`.
- **`gc_once` ordering**: skipped-db fail-close → absent dir ⇒ `Ok` zero stats (`skipped=false`) → marker gate → lock acquire → passes.
- **PUT ordering** (SD3.3): stream→hash+count (`> max_object_bytes` ⇒ tmp removed + `QuotaExceeded{Object}`) → rename atomic → `insert_object_if_absent` → `get_object_link` probe (re-reference = no quota charge) → `repo_object_counts` gate → on `QuotaExceeded{Repo}`+`Stored` the conditional delete runs and the file unlinks ONLY when it returns true → `insert_object_link_if_absent`. Bare hash out.
- **Fail-close** (SD3.7): `db.is_skipped()` ⇒ `DbNotConfigured` at put/get/gc entry; `From<io::Error>` maps NotFound⇒`NotFound` else `Io`; `From<String>`⇒`Db` only for facade results (stream chunk errors construct `Stream` explicitly); `From<VarError>`⇒`Config`.
- **GC reclaim** (SD3.5): conditional row delete BEFORE file unlink; `false` = protected silent-skip; row-less probe `Err` keeps file + `errors+=1`; all unlinks NotFound-tolerant; logs `%hash`/`%error` only.
- **Lock** (SD3.4): `create_new(.gc-lock)`; live lock ⇒ `skipped=true` stats; stale ⇒ remove + retry once; heartbeat is `OpenOptions::write` + `set_modified` — never `File::create`.
- **Quota casts** (SD3.6): `u64::try_from(i64)` on `storage_bytes`; `size` bind guarded by `max_object_bytes ≤ i64::MAX` + `try_from` belt; `repo_quota_bytes=0` admits size-0 objects.
- **Row-less sweep** (SD3.2.3): `{2hex}/{2hex}/{64hex}` name-shape filter before stat; canonicalize + `starts_with(canonical_root)`; symlinks skipped no-follow; `*.tmp` at any swept level rides the tmp predicate.
- **U4 invariant** (WF3/C6a): no standalone `insert_links_tx` shipped — `vcs_create_changeset_tx`'s `object_links` param is the mechanism; mod.rs doc header records that U4 must pass EVERY manifest `blob_hash` (not just new blobs) or objects look orphaned to pass 1 while `file_changes` keeps the conditional delete false forever.
- **Carried doc nits** (infra iter-2) folded here, not re-edited upstream: the contributions-file INF-11 line, the `jq select` pin (:909 — not needed; `vcs-server` produces no second artifact), and the questions dep-kind wording.

## Deviations

- `oxidean-core` appears as a **dev-dependency** (not in the plan's dep list — only `tempfile` was named). Repo seeding goes through the real `Database::create_user` facade which takes `oxidean_core::Role`; dev-deps never link into client or server production builds, so the dep posture is unchanged.
- `QuotaScope` / `PutKind` are the enum names for the plan's `ObjectQuotaScope` / `Stored|Dedup` discriminants — same closed semantics.
- One extra test beyond the 24-row matrix: `gc_full_pass_via_public_api` (put → repo-delete cascade → orphan → gc reclaims), covering the public-API end-to-end path.

## Verification

- `cargo check -p oxidean-vcs` — clean (client feature: zero server deps linked)
- `cargo check -p oxidean-vcs --features server` — clean
- `cargo test --no-run -p oxidean-vcs --features server` — clean
- `cargo nextest run -p oxidean-vcs --features server` — **23 passed**
- `cargo test -p oxidean-vcs --features server` — **23 passed** (fallback parity)
- `cargo nextest run -p oxidean-db -E 'test(vcs_)'` — **2 passed** (`vcs_get_object_link_bool`, `vcs_delete_object_if_unlinked`)
- `cargo nextest run -p oxidean-db` — **71 passed, 0 skipped** (facade additions, no regression)
- `cargo clippy -p oxidean-vcs --all-targets --all-features -- -D warnings` — clean
- `cargo test -p oxidean-vcs --doc` — 0 doc tests (none authored — pinned)
- `cargo fmt --check` — clean
- `make dead-code-check` — knip + machete + workspace clippy + `--all-features` clippy: clean
- `make docs-check` — workspace rustdoc + `-p oxidean-vcs --features server` rustdoc: clean
- `make rpc-sync-check` — ok, zero diff

## Known carry-forwards

- pg/mysql facade paths for the two new methods probed only by `db-dialects` (house precedent — recorded debt).
- Concurrency races (R1–R3, R8) are residual-risk register items — LockGuard Drop + create-new + heartbeat cover the single-node pin; multi-node is out of scope.
- Rust coverage stays collected-not-gated; `cargo llvm-cov nextest --workspace` is default-features so this suite contributes nothing until U7's dep edge unifies features (flag for U7 planning).
- U7 wiring remains future work: routes, auth, RPC transport, host config (`ObjectStoreConfig::from_env` call site), and the GC-interval job loop (`gc_interval_secs` is parsed/pinned but no spawner ships in u3).
