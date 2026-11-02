# Kubernetes-native storage alternatives — Plan

Status: implemented

## Task 1: Spec record ✓

This folder + OPS-08 in `agent-os/product/roadmap.md`.

## Task 2: fs shared-storage knobs (CSI/RWX)

- `values.yaml`: `storage.data.{existingClaim,accessModes}`,
  `storage.s3.repos.{existingClaim,accessModes}`,
  `ssh.hostKeys.existingClaim` (+ comments naming EFS/Filestore/Azure
  Files/CephFS/Longhorn/NFS provisioner as targets).
- `templates/pvc.yaml`: skip the PVC manifest when `existingClaim` set;
  render `accessModes` from values.
- `templates/_helpers.tpl`: `dataPvcName`/`reposPvcName`/`sshPvcName`
  resolve to `existingClaim` when set (mount side needs no other change —
  deployments already reference the name helpers).
- Extended past the original scope for parity with the k8s docs' claim:
  `postgres.persistence`, `objectstore.persistence`, and
  `runner.persistence` carry the same `{existingClaim,accessModes}` pair;
  their name helpers substitute the claim and the PVC manifests (postgres
  and runner render inline in their Deployments) gate on it.

## Task 3: SeaweedFS bundled provider

- `Chart.yaml`: `seaweedfs` dependency (chart 4.51.x,
  `https://seaweedfs.github.io/seaweedfs/helm`),
  `condition: seaweedfs.enabled`. `Chart.lock` committed;
  `charts/*.tgz` gitignored.
- `values.yaml`: `seaweedfs:` pass-through block with our pinned defaults —
  `master.replicas: 1` + `data.type: persistentVolumeClaim` (size 1Gi),
  `volume.replicas: 1` + `dataDirs: [persistentVolumeClaim]` (size 10Gi),
  `filer.replicas: 1` + `data.type: persistentVolumeClaim` (size 1Gi),
  `s3.enabled: true`, `s3.enableAuth: true`,
  `s3.credentials.admin.existingSecret` + key names pointing at the chart
  Secret's `OXIDEAN_S3_*` keys (or `secrets.existingSecret` when set).
  All upstream `hostPath` defaults overridden.
- `_helpers.tpl`: `oxidean.s3Bundled` covers `seaweedfs.enabled`;
  `oxidean.s3Endpoint` → `http://<release>-seaweedfs-s3:8333` for that
  provider; `s3Bucket` bundled default unchanged; `s3CredentialsSecretName`
  unchanged; mutual-exclusion `fail` for `objectstore.enabled &&
  seaweedfs.enabled`.
- `templates/secret.yaml`: generated `OXIDEAN_S3_*` pair predicate extends
  to seaweedfs-bundled mode.
- `templates/deployment-api.yaml`: env block unchanged (endpoint/bucket/
  creds helpers do the branching).
- `templates/job-bucket-init.yaml`: unchanged (runs against the derived
  endpoint).

## Task 4: CI lane + Makefile

- `scripts/ci-helm-smoke.sh`: `helm dependency build` before lint; new
  render assertions — seaweedfs wiring (s3 svc endpoint in api env, secret
  ref, no rustfs deploy), mutual-exclusion fail, `existingClaim` suppresses
  PVC + mounts claim, RWX `accessModes` render.
- `Makefile` helm targets: dep-build step.
- `.github/workflows/` helm job + `release.yml`/`ox-cli.yml` chart-publish
  lane: `helm dependency build` before lint/package (OCI push needs the
  vendored subchart).

## Task 5: Docs

- `docs/SELFHOST-KUBERNETES.md`: shared-filesystem section (driver table:
  EFS/Filestore/Azure Files/CephFS/Longhorn/NFS provisioner + RWX note),
  SeaweedFS provider section (what it deploys, sizing, prod guidance:
  replicas + external filer metadata store), update storage table.
- `docs/CONFIGURATION.md`/`docs/SELF-HOSTING.md`: touch-ups only if the
  env contract changes (it should not — same `OXIDEAN_S3_*` envs).
- `AGENTS.md` parity map: note the intentional bundled-store asymmetry
  (compose RustFS ↔ helm RustFS-or-SeaweedFS).

## Task 6: Verify

- `helm dependency build`, `helm lint`, full render matrix locally.
- `make helm-smoke` on kind: install fs → upgrade to s3+seaweedfs → rollouts
  green → `s3 ls` the bucket through the s3 service.
- Push; watch CI `helm` + `selfhost-example` lanes.

## Follow-ups (not this spec)

- Multi-replica api on RWX (OPS-03) — HA story beyond storage.
- SeaweedFS HA sizing presets / external filer metadata store (etcd/mysql).
- MinIO/Garage remain external-endpoint options (AGPL — no bundling).
