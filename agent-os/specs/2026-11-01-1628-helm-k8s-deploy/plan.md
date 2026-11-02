# Helm/Kubernetes Deployment — Plan

Shaping: [`shape.md`](shape.md). Standards: [`standards.md`](standards.md).
References: [`references.md`](references.md).

## Status

Implemented. All shaping decisions settled 2026-11-01; implementation
landed on the spec branch. Recorded deviations from this plan:

- **Bundled Postgres is native chart templates, not a Bitnami OCI
  subchart.** A `postgres:18-alpine` `Deployment`+`Service`+PVC in-repo
  keeps parity with the Compose bundle's exact image, removes an external
  dependency fetch from CI/offline installs, and exposes the identical
  operator surface (`postgres.enabled`, `postgres.size`,
  `database.existingSecret`/`urlKey`).
- **`storage.backend: s3` ships slim PVCs + env wiring, plus
  `storage.s3.retainDataVolume: true` by default.** `oxidean-storage`
  exists but its `BlobStore` is not yet consumed by the api, so s3 mode
  keeps a `/var` PVC unless the operator opts out — documented honestly in
  `docs/SELFHOST-KUBERNETES.md` and `values.yaml`.
- **The bundled `DATABASE_URL` lives in the api Deployment env block, not
  the Secret.** Kubernetes expands `$(VAR)` only in literal `value:`
  fields, never in Secret data — `POSTGRES_*` are `secretKeyRef` entries
  and `DATABASE_URL` expands them in-pod.

## Task 1 — Save spec documentation

This folder: `shape.md`, `plan.md`, `standards.md`, `references.md`.
Lands with the spec PR itself.

## Task 2 — Chart skeleton (fs backend, bundled postgres)

`deploy/helm/oxidean/`:

- `Chart.yaml` (`apiVersion: v2`, `version`/`appVersion` placeholder for
  release stamping), `.helmignore`
- `values.yaml` — image refs (`ghcr.io/oxidean/oxidean-{api,web}`, tag
  default `.Chart.AppVersion`), `storage.backend: fs`, single `data` PVC
  values (size/storageClass), `ingress` block, `ssh.service` block,
  `postgres.enabled: true` + `database.external`/`existingSecret`
  override, `env`/`extraEnv` passthrough, `resources`,
  `podSecurityContext`/`securityContext`
- `templates/_helpers.tpl` (names, labels, env renderer)
- `templates/deployment-api.yaml` — replicas 1, `strategy: Recreate`,
  `/var` PVC mount, `/health:8080` probes, full `OXIDEAN_*` env surface
  mapped from values (same env contract as compose — no new names)
- `templates/service-api.yaml` — ClusterIP :8080 for `web`'s
  `OXIDEAN_API_ORIGIN`
- `templates/deployment-web.yaml` — N replicas, `/health:3000` probe with
  `Oxidean-Health-Probe: 1` httpHeader
- `templates/service-web.yaml`, `templates/ingress.yaml`
- `templates/pvc.yaml` — `/var` claim
- `templates/secret.yaml` — `lookup`-guarded generation +
  `existingSecret` passthrough
- `templates/NOTES.txt` — URL, SSH endpoint, admin bootstrap hints

## Task 3 — `storage.backend: s3` mode

- PVCs only for `/var/repos` and `/var/ssh`; `data` PVC skipped
- `OXIDEAN_STORAGE_BACKEND=s3` + `OXIDEAN_S3_*` envs from Secret
  (`endpoint`, `region`, `bucket`, `prefix`, `pathStyle`, credentials)
- values comments cross-referencing `crates/oxidean-storage` config and
  `docs/CONFIGURATION.md`

## Task 4 — SSH service

`templates/service-ssh.yaml` — `type: LoadBalancer` (values:
`ssh.service.type`, `port`, `annotations` for LB flavor, `externalIP`
option). Docs note: ingress-nginx `tcp-services` alternative.

## Task 5 — Postgres dependency

- `postgres.enabled=true` default → Bitnami postgresql OCI subchart for
  eval installs; renders `DATABASE_URL` into the api Secret
- `postgres.enabled=false` + `database.existingSecret` carrying
  `DATABASE_URL` is the production path; docs mark the bundled database
  as evaluation-grade

## Task 6 — Optional runner

`runner.enabled=false` default; `templates/deployment-runner.yaml` with
privileged dind sidecar, registration-token Secret, labels. Values
comments spell out the security tradeoff.

## Task 7 — CI gate

New `helm` job in `ci.yml`: `helm lint`, `helm template` render check
(golden diff or `kubeconform`/dry-run apply), `kind` install smoke
(bring up api+web+postgres, curl `/health`). Same change updates the
`Main` ruleset required-status-checks list — procedure in
`docs/TESTING.md#required-status-checks-merge-gate`.

## Task 8 — Release integration

- `versioning.toml` component for the chart plus real `kind = "helm"`
  support in the versioning tooling (same shape as `contract`/`bun`) so
  `Chart.yaml` `version`/`appVersion` are stamped by the release train
- Release workflow: `helm package` → `helm push` to
  `oci://ghcr.io/oxidean/charts/oxidean`, version = release version

## Task 9 — Docs

- `docs/SELFHOST-KUBERNETES.md` — values tour, fs→s3 migration note,
  SSH exposure, secrets management, upgrade path
- `deploy/helm/oxidean/README.md` — quickstart
- README deploy pointer + `docs/CONFIGURATION.md` cross-ref
- `agent-os/product/roadmap.md` — `OPS-06` line item (lands with this
  spec PR); OPS-02 status updated to reflect the merged foundation

## Follow-ups (explicitly out of scope for v1)

- Consumer migration of stores behind `BlobStore` (per #210 follow-ups;
  the chart works either way — fs mode needs no migration)
- Git repo sharding / Gitaly-style split → `api` multi-replica
- Gateway API `HTTPRoute`/`TCPRoute` values
- PDB/HPA for `web`, network policies, helm test hooks
- Runner hardening (rootless dind, Firecracker-class isolation)
