# Tech-Stack Decisions — u3-vcs-object-store

## TD3.1 — `oxidean-vcs` cargo feature split (ADR-011)

First workspace use of `dep:`-gated features: `default = ["client"]`, `server = ["dep:oxidean-db", "dep:tokio", …]`. `ObjectStore` lands under `server`; `mod object_store` is `#[cfg(feature = "server")]`-gated so `cargo check -p oxidean-vcs` (default) links zero `oxidean-db`. `oxidean-core` is the shared dep (error codes); `oxidean-cli`/`oxidean-api` never become deps of this crate (host→crate direction, FR16.1). Enforcement pin: `cargo tree -p oxidean-vcs` (default features) shows no `oxidean-db` — `cargo check` alone can't prove non-linkage.

## TD3.2 — Server dep set (pinned, per-crate versions matching api's)

Workspace-level `workspace = true` covers only serde/serde_json/schemars/tokio — everything else pins per-crate at api's versions:

- `oxidean-db` (path dep, `dep:`-gated under server) — facade
- `sha2 = "0.11"` (api Cargo.toml:51 — lockfile already dual-versioned; `0.10` would add a third copy)
- `tokio` (workspace dep; features fs/io-util/macros/rt-multi-thread already declared)
- `futures-util = "0.3"` — `StreamExt` stream shape
- `bytes = "1"` — **new direct dep** (currently transitive-only via axum 0.8's body chunks; must stay the same `Bytes` type or the U7 route seam stops compiling)
- `tracing = "0.1"` — GC stats
- `chrono = "0.4.45"` (api pin — grace_ts ISO cutoff; `std::time` can't format row timestamps)
- `tempfile = "3"` — dev-dependency only (tests)
- local `bytes_to_hex` (api's is unreachable under the dependency direction; six private clones already exist in-tree)

`cargo machete` runs in lint — feature-gated deps can false-positive "unused"; the `[package.metadata.cargo-machete] ignored` escape hatch exists (api Cargo.toml:106) if needed.

## TD3.3 — API shapes (pinned)

- `put<S, E>(stream: S, repo_id) -> Result<PutOutcome>` where `S: StreamExt<Item = Result<Bytes, E>> + Unpin` — LFS `put_stream` precedent. `PutOutcome{hash, Stored|Dedup}` — **hash returned bare** (the `sha256:` prefix is added at the U7 wire boundary; FD WF1.4's prefixed-return wording is corrected here — double-prefixing would be a wire bug).
- `get(hash: &str) -> Result<tokio::fs::File>` — opened file; the U7 route wraps it (`ReaderStream` is the host's). Never `Vec<u8>` at 256 MiB objects.
- `verify(hash) -> Result<()>` — `Err(Internal)` on hash mismatch (corruption is a defect, not a boolean), `Err(NotFound)` on absence.
- `gc_once() -> Result<VcsGcStats>` + `clear_stale_lock()` — the U7 job/boot hooks; `gc_once` releases the lockfile on completion.
- `ObjectStoreError`: `QuotaExceeded{Object|Repo}`, `NotFound` (incl. `io::ErrorKind::NotFound`), `InvalidHash`, `Stream(String)` (chunk error → `vcs.invalid_input`), `DbNotConfigured` (eager `is_skipped()`), `Db(String)`, `Io`, `Internal`.

## TD3.4 — Facade reuse + sanctioned delta (BR3.11)

Existing methods consumed verbatim: `vcs_get_object` (also the row-less-sweep membership probe — per-file indexed lookup, O(files) per pass, accepted), `vcs_insert_object_if_absent` (bool → Stored|Dedup), `vcs_insert_object_link_if_absent`, `vcs_repo_object_counts` (`{object_count, storage_bytes}`), `vcs_list_orphaned_objects_before` (unbounded Vec — backlog spike accepted single-node; LIMIT-batching is a v2 option). Two additions inside `crates/oxidean-db/src/vcs.rs` under this unit's manifest: `vcs_get_object_link(repo_id, hash) -> Result<bool>` (SELECT EXISTS) and `vcs_delete_object_if_unlinked(hash) -> Result<bool>` (`DELETE … WHERE hash AND NOT EXISTS(link)` — bool gates the file unlink). Per-dialect SQL stays in `oxidean-db` — NFR6 parity holds with zero new migrations (reuses 0053 tables + `idx_vcs_object_links_hash` + `idx_vcs_objects_created_at`).

## TD3.5 — Error channel

Hand-rolled `ObjectStoreError` enum (TD3.3 variant list), `From` impls for `io::Error`/facade `String`/`std::env::VarError` (config). Maps onto the frozen wire taxonomy at the U7 boundary; the enum itself never stringifies into wire data.

## TD3.6 — Concurrency model

Single-node process-local: `create_new` lockfile for GC (release at pass end + mtime heartbeat during long passes — prevents both lock-starvation and mid-run staleness); fs atomicity via same-dir rename; DB atomicity via facade txs. Concurrent same-hash PUTs: unique tmp names + `insert_if_absent` → loser is a clean Dedup. Local POSIX fs assumed (NFS/overlayfs degrade `create_new`+rename semantics — U7 deploy-docs note).

## TD3.7 — Test posture (NFR10) — feature-gated, named commands

**The `server` feature compiles nothing under default invocations** — `cargo nextest run --workspace`/`--all-targets`/clippy/doc all run default features and would produce zero U3 test binaries. Pinned invocations (claimed in this unit's source manifest as a Makefile delta): `cargo nextest run -p oxidean-vcs --features server` plus `cargo clippy -p oxidean-vcs --all-targets --all-features`. Test module under `#[cfg(all(test, feature = "server"))]` (or `tests/` + `required-features`); the suite must appear in nextest output — zero-test runs are a failure, not a skip.

nextest on real sqlite `Database` (migrated, incl. `Database::skipped()` fail-close cases) + `tempfile` object dirs: put/get round-trip, dedup (same + cross-repo), both quota denials + eager cleanup, mid-stream abort (no file/row/tmp), empty PUT, orphan/row-less/stray-tmp sweeps (incl. `.gc-lock` survival + foreign-file safety via marker), lock acquire/skip/age-stale/release, absent-dir early return, hash-guard rejections, env parse/range errors, `create_changeset_tx` link insertion through the facade.
