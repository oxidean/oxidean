# oxidean — Helm chart

Self-hosted Oxidean forge on Kubernetes: `web` (UI + internal API proxy),
`api` (single-replica forge + git backend on RWO storage), optional bundled
`postgres` and `objectstore` (RustFS), a raw-TCP SSH `LoadBalancer`, and an
opt-in Actions `runner`.

Published as an OCI artifact:

```
oci://ghcr.io/oxidean/charts/oxidean
```

Operator guide: [`docs/SELFHOST-KUBERNETES.md`](../../../docs/SELFHOST-KUBERNETES.md).

## Installing

```bash
helm install ox oci://ghcr.io/oxidean/charts/oxidean \
  --namespace oxidean --create-namespace -f my-values.yaml
```

`helm show values oci://ghcr.io/oxidean/charts/oxidean` prints the full
annotated defaults.

## Values overview

| Key | Default | Purpose |
|-----|---------|---------|
| `api.image.repository` / `.tag` | `ghcr.io/oxidean/oxidean-api` / appVersion | api image; released charts pin the tag the release was cut against |
| `web.image.*`, `web.replicaCount` | `ghcr.io/oxidean/oxidean-web`, `1` | stateless web tier — scale replicas freely |
| `postgres.enabled` | `true` | bundled `postgres:18-alpine` — evaluation-grade |
| `database.existingSecret` / `.urlKey` | `""` / `DATABASE_URL` | external `DATABASE_URL` holder when postgres is disabled |
| `ingress.enabled` / `.host` / `.tls` | `true` / `""` / `[]` | single-hostname HTTP edge → `web:3000` |
| `ssh.enabled` / `.service.type` / `.port` | `true` / `LoadBalancer` / `22` | raw-TCP git-over-SSH → api container port 2222 |
| `storage.backend` | `fs` | `fs` = one `/var` PVC; `s3` = slim repos+ssh PVCs + `OXIDEAN_S3_*` |
| `storage.s3.*` | — | bucket/prefix/endpoint/region/pathStyle/insecureHttp/static keys — bundled defaults apply when `objectstore.enabled` |
| `objectstore.enabled` | `false` | bundled RustFS (S3-compatible) Deployment+Service+PVC + bucket-init Job — eval-grade, external `storage.s3.*` for production |
| `storage.s3.retainDataVolume` | `true` | keep a `/var` PVC in s3 mode while OPS-02 consumer migration lands |
| `secrets.existingSecret` | `""` | operator-managed app secrets (`envFrom`) — no chart Secret rendered |
| `runner.enabled` | `false` | Actions runner + privileged DinD sidecar — see security note |
| `api.env.*` | — | extra non-secret `OXIDEAN_*` env (`RUST_LOG`, mail, SSO ids, …). Values override chart-rendered env names but **not** the matching resources — e.g. `api.env.OXIDEAN_SSH_ENABLED=false` leaves the SSH Service/PVC up; use `ssh.enabled`, `allowSignup`, etc. for structural switches |

See `values.yaml` for the complete annotated surface — every value carries
a comment.

## Versioning

`version` is the chart's own SemVer; `appVersion` tracks `oxidean-api`.
Released charts also stamp `values.yaml` image tags, so installing
`oci://ghcr.io/oxidean/charts/oxidean --version X.Y.Z` always resolves the
exact image set that release was cut against. Cut with
`type(scope): …` commits per `docs/VERSIONING.md`.
