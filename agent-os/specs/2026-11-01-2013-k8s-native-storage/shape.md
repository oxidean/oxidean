# Kubernetes-native storage alternatives — Shaping Notes

## Scope

Extend the `deploy/helm/oxidean` storage surface with options that are
Kubernetes-native rather than single-node defaults:

- **fs backend → CSI/shared filesystem.** Today `storage.backend=fs` renders
  one chart-created RWO PVC. Add `existingClaim`, `accessModes`, and
  `storageClass` knobs to the product volumes (data, s3-slim repos, SSH
  host-keys) so operators can bring cluster-native shared storage — EFS,
  GCP Filestore, Azure Files, CephFS (Rook), Longhorn, NFS provisioner —
  including `ReadWriteMany` where the driver supports it.
- **s3 backend → bundled k8s-native object store.** The bundled RustFS is
  deliberately eval-grade (single Deployment + PVC). Add **SeaweedFS** as a
  second bundled provider via its official Helm chart as a subchart
  dependency — a real multi-component topology (master + volume + filer +
  s3 gateway) that is Apache-2.0 licensed (repo is MIT; MinIO/Garage are
  AGPL and excluded as bundled defaults).

External endpoints (`storage.s3.endpoint`) remain the recommended
production path and are unchanged — this adds bundled alternatives, it does
not replace the external story.

## Decisions (confirmed — "pick all recommended approaches" applies)

- `storage.data.existingClaim` / `storage.s3.repos.existingClaim` /
  `ssh.hostKeys.existingClaim`: when set, the chart renders no PVC and mounts
  the named claim. `accessModes` lists default `["ReadWriteOnce"]` and pass
  through verbatim (RWX is the operator's choice; api stays single-replica
  until OPS-03 regardless).
- `seaweedfs:` top-level pass-through values block gated by
  `seaweedfs.enabled` (subchart `condition:` — a boolean path is required,
  so provider selection is two-key, not `objectstore.provider`).
- `objectstore.enabled` (RustFS) and `seaweedfs.enabled` are mutually
  exclusive — template `fail` when both render.
- Endpoint derivation stays one seam: `oxidean.s3Bundled` covers either
  bundled store when `storage.s3.endpoint` is empty; explicit endpoint
  always wins. SeaweedFS endpoint = `http://<release>-seaweedfs-s3:8333`.
- Bundled bucket default reuses `objectstore.bucket` for both providers
  (provider-neutral "the bundled store's bucket"); the same aws-cli init
  Job creates it.
- Credentials unchanged: the chart Secret's `OXIDEAN_S3_ACCESS_KEY_ID` /
  `OXIDEAN_S3_SECRET_ACCESS_KEY` pair feeds both the api env and the
  subchart via `s3.credentials.admin.existingSecret` + key names —
  single source, `lookup`-guarded generation as today.
- SeaweedFS persistence: upstream chart defaults `master.data`,
  `volume.dataDirs`, and `filer.data` to **hostPath** — our pass-through
  pins `persistentVolumeClaim` + sizes + `storageClass` pass-through.
- Compose surfaces unchanged: compose keeps the light RustFS `s3` profile
  (single-node eval); the heavyweight native store is a k8s-only option.
  The AGENTS.md parity map notes the intentional asymmetry.
- Chart dependency is built (`helm dependency build`) at dev/CI/package
  time; `Chart.lock` committed, `charts/*.tgz` gitignored.

## Context

- **Visuals:** none.
- **References:** `2026-11-01-1628-helm-k8s-deploy` (chart skeleton, OPS-06),
  `2026-11-01-1934-selfhost-object-store` (bundled RustFS + parity, OPS-07),
  `2026-11-01-1425-storage-blob-backend` (`OXIDEAN_S3_*` env contract).
- **Product alignment:** OPS-03 (HA guidance) is a neighbor — RWX support is
  the storage-side prerequisite for any future multi-replica api, recorded
  here as groundwork, not scope.

## Standards Applied

- way-of-working — spec-then-implement on a branch/PR; follow-ups recorded.
- commits-versioning — `feat(helm):`/`docs(spec):` subjects; `helm` kind.
- review-practices — no secrets in values; redacted docs examples.
- testing — every new values path gets a render-matrix assertion.

## Roadmap

OPS-08 in `agent-os/product/roadmap.md`.
