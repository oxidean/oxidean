# References for Helm/Kubernetes Deployment

## Prior decisions

### Storage backend spec (the direct unlock)

- **Location:** `agent-os/specs/2026-11-01-1425-storage-blob-backend/`
- **Relevance:** defines the K8s target topology — `api` single-replica
  with `/var/repos` RWO + `/var/ssh`, all other stores in object storage;
  `web` scales freely. The chart's `storage.backend` values mirror its
  `fs`/`s3` split.
- **Key patterns:** per-store key prefixes under one bucket; presign
  per-store, not per-backend; POSIX stayers (`repos`, ssh keys, sqlite,
  cli dist).

### Issue #210 — S3 storage backend

- **Location:** <https://github.com/oxidean/oxidean/issues/210>
- **Relevance:** its "Kubernetes/Helm" section enumerates the unlock;
  store table is the volume/env inventory for `values.yaml`.

## Deploy surfaces to mirror

### Self-host Compose

- **Location:** `deploy/selfhost/docker-compose.yml`, `deploy/selfhost/Caddyfile`
- **Relevance:** canonical env surface and dependency order
  (`postgres` healthy → `api` healthy → `web`); single `oxidean_data:/var`
  volume; `api` exposes SSH on 2222 + HTTP on 8080; `web` on 3000; health
  probes: api `curl /health`, web `curl -H Oxidean-Health-Probe:1 /health`.
- **Key patterns:** profiles gate optional services (`tls` → caddy,
  `actions` → runner) — the chart's `*.enabled` toggles map the same way.

### Railway IaC

- **Location:** `.railway/railway.ts`, `.railway/README.md`
- **Relevance:** the other file-defined topology; D-CLOUD-01..07
  decisions (single forge volume, managed postgres, channel tags) are the
  same calls the chart makes in K8s terms.

### Image contracts

- **Location:** `crates/oxidean-api/Dockerfile` (EXPOSE 8080, entrypoint
  script), `apps/web/Dockerfile` (EXPOSE 3000, `PORT` env, header-gated
  `/health` note)
- **Relevance:** probe definitions and port/env names come from here,
  not from convention.

### Storage crate config surface

- **Location:** `crates/oxidean-storage/src/config.rs`
- **Relevance:** authoritative `OXIDEAN_STORAGE_BACKEND`/`OXIDEAN_S3_*`
  env names, fail-closed behavior, redacted-credentials rules — the
  chart's s3 Secret values must match exactly.

## Process references

- `agent-os/specs/_archive/gsd/milestones/v1.0-phases/22-compose-ci-deploy/` —
  prior deploy work (compose + CI smoke) whose patterns the `helm` CI job
  extends.
- `.github/workflows/ci.yml` — `selfhost-example`, `compose-smoke` jobs
  show the existing deploy-validation lane shape.
- `docs/CONFIGURATION.md` — env-var documentation the chart's values
  comments should cross-reference rather than duplicate.
- `docs/TESTING.md#required-status-checks-merge-gate` — ruleset update
  procedure required when adding the CI job.
- `versioning.toml` + `docs/VERSIONING.md` — component registry and
  release flow the chart plugs into.
