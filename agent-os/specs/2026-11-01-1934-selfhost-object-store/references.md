# Self-hostable object storage — references

## Authoritative contracts

- `crates/oxidean-storage/src/config.rs` — `OXIDEAN_STORAGE_BACKEND`,
  `OXIDEAN_S3_*` env names, validation, fail-closed behavior. The deploy
  surfaces forward this contract verbatim; they never invent new env names.
- `docs/CONFIGURATION.md` — operator-facing env reference (updated here).

## Surfaces

- `deploy/selfhost/docker-compose.yml` — single-docker production-parity stack
  (`s3` profile added here).
- `docker-compose.yml` — dev stack (`s3` profile added here).
- `deploy/helm/oxidean/` — chart; `objectstore` section added here.
- `deploy/selfhost/.env.example`, `.env.example` — operator env templates.
- `scripts/ci-helm-smoke.sh` — chart CI lane (render matrix + kind smoke).

## Upstream engine

- RustFS `rustfs/rustfs:1.0.4` (Apache-2.0) — `RUSTFS_ACCESS_KEY`,
  `RUSTFS_SECRET_KEY`, `RUSTFS_VOLUMES`, `RUSTFS_ADDRESS`, `RUSTFS_CONSOLE_*`,
  `RUSTFS_CHECK_UPDATE`; S3 API `:9000`, path-style default, MinIO-compatible
  health endpoints. Docs: https://docs.rustfs.com/
- Bucket init: `amazon/aws-cli` image (`aws s3api head-bucket` /
  `aws s3 mb --endpoint-url`).

## Linked specs and issues

- `agent-os/specs/2026-11-01-1425-storage-blob-backend/` — `BlobStore` trait +
  `fs`/`s3` backends; consumer rollout (OPS-02 `[~]`).
- `agent-os/specs/2026-11-01-1628-helm-k8s-deploy/` — helm chart this extends
  (OPS-06 `[x]`).
- Issue #210 (S3-compatible storage) — the K8s/Helm unlock chain this completes
  on the bundled side.
- MinIO status grounding: community edition went source-only (no published
  images) Oct 2025 and maintenance-mode Dec 2025 — why RustFS is bundled
  instead.
