# References — S3-Compatible Blob Storage Backend

## Code surfaces

- `crates/oxidean-storage/` — the crate: `lib.rs` (trait + types),
  `fs.rs` (POSIX backend), `s3.rs` (`object_store::AmazonS3` backend),
  `key.rs` (`BlobKey` validation + reserved prefixes), `config.rs` (env
  surface + `check_store_isolation`), `store.rs`, `error.rs`
- `crates/oxidean-api-core/src/app.rs` — `AppState` fs roots to migrate
  (LFS, release assets, packages, template packs, VCS objects, actions
  artifacts/cache/logs, pages, uploads)
- `crates/oxidean-api-core/src/vcs/` — `ObjectStoreConfig` marker-verified
  root, the precedent for the `BlobStore` marker gate
- `docker-compose.yml`, `deploy/selfhost/docker-compose.yml` — day-0
  volume mounts for artifacts/cache/pages
- `docs/CONFIGURATION.md` — `OXIDEAN_STORAGE_BACKEND` + `OXIDEAN_S3_*`
  operator docs

## Prior art studied

- VCS object store (`OXIDEAN_VCS_OBJECTS_DIR`) — content-addressed store
  with DB-refcount GC and marker-verified root; its cross-store isolation
  invariant (`D-PKG-07`, `D-ACT-13`) carries into object storage via the
  marker gate
- GitLab's decoupled hybrid (Gitaly + object storage) — the architecture
  shape adopted here

## Provenance

- Issue: [#210](https://github.com/oxidean/oxidean/issues/210) — full
  store table, env surface, trait shape
- Foundation PR: [#214](https://github.com/oxidean/oxidean/pull/214)
