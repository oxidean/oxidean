# Plan — S3-Compatible Blob Storage Backend

> Recorded 2026-11-01 alongside the foundation PR (#214). Scope defined on
> issue [#210](https://github.com/oxidean/oxidean/issues/210).

## Status: foundation in review (#214); consumer migration pending

Phased delivery — each phase ships independently behind `fs` defaults:

- [x] **Day-0 persistence gap** — dev `docker-compose.yml` now sets and
      mounts `OXIDEAN_ACTIONS_ARTIFACTS_DIR`, `OXIDEAN_ACTIONS_CACHE_DIR`,
      `OXIDEAN_PAGES_DIR` matching the actions-logs pattern (PR #214)
- [x] **`oxidean-storage` crate** — `BlobStore` trait (`put_stream`,
      `get`, `get_range`, `head`, `delete`, `exists`, `begin_upload` →
      `StagedUpload` commit/abort, `presign_get`/`presign_put` on s3),
      `fs` + `s3` backends, `BlobKey` validation, marker-verified roots
      (`check_store_isolation`), `StorageConfig` env surface
      (`OXIDEAN_STORAGE_BACKEND`, `OXIDEAN_S3_*`) (PR #214)
- [ ] **Consumer migration, store by store** — LFS, release assets,
      packages, template packs, VCS objects, actions
      artifacts/cache/logs, pages, uploads; each call site in
      `crates/oxidean-api-core/src/app.rs` (~10 fs roots) migrates behind
      the trait (follow-up PRs; `repos_dir` and SSH host keys stay POSIX)
- [ ] **Multipart mapping for Actions artifacts** — chunked uploads onto
      `StagedUpload`/S3 multipart
- [ ] **MinIO self-host overlay** — compose profile for local s3 testing
- [ ] **Migration tooling** — fs → bucket copy per store prefix

## Out of scope

- Gitaly-class git object storage (`repos_dir` stays POSIX; separate
  decision when repo HA is designed)
- Public buckets, unauthenticated delivery

## Config surface shipped in #214

`OXIDEAN_STORAGE_BACKEND` (`fs`|`s3`), `OXIDEAN_S3_BUCKET`,
`OXIDEAN_S3_REGION`, `OXIDEAN_S3_ENDPOINT`, `OXIDEAN_S3_ACCESS_KEY_ID` /
`OXIDEAN_S3_SECRET_ACCESS_KEY` (or ambient AWS chain),
`OXIDEAN_S3_PATH_STYLE`, `OXIDEAN_S3_PREFIX`, `OXIDEAN_S3_INSECURE_HTTP`,
`OXIDEAN_S3_PRESIGN_TTL_SECS` — documented in `docs/CONFIGURATION.md`.
