# Self-hosting Oxidean on Kubernetes

Run a complete Oxidean forge — web UI, API, database, Git-over-SSH — on any
Kubernetes cluster with the official Helm chart. This is the third self-host
surface next to [Docker Compose](SELF-HOSTING.md) and Railway, and it keeps
the same topology: `web` is the only HTTP origin and reverse-proxies every
API path internally; `api` is a single replica owning POSIX Git storage.

The chart lives in [`deploy/helm/oxidean/`](../deploy/helm/oxidean/) and is
published as an OCI artifact on every `oxidean-chart-v*` release:

```
oci://ghcr.io/oxidean/charts/oxidean
```

Related docs: [SELF-HOSTING.md](SELF-HOSTING.md) (Compose bundle),
[SELFHOST-MIGRATE-TO-K8S.md](SELFHOST-MIGRATE-TO-K8S.md) (moving an
existing Compose install here — DB, repos, SSH keys, secrets),
[CONFIGURATION.md](CONFIGURATION.md) (every env var),
[DEPLOYMENT.md](DEPLOYMENT.md) (all deployment targets),
[database.md](database.md) (dialects),
[chart README](../deploy/helm/oxidean/README.md) (full values reference).

## Architecture

| Component | Kind | Replicas | Notes |
|-----------|------|----------|-------|
| `web` | `Deployment` | 1+ (scalable) | The single HTTP origin: serves the UI and reverse-proxies RPC, `/{owner}/{repo}.git` Smart HTTP + LFS, `/v2` `/npm` `/generic` registries, badges, Pages, OAuth, and `/api/rpc/ws` websockets to `api`. |
| `api` | `Deployment` | **1** | Forge API + git backend. `strategy: Recreate`, RWO volumes — loose objects, ref locks, and `receive-pack` need real POSIX semantics. |
| `postgres` | `Deployment` | 1 | Bundled `postgres:18-alpine`, evaluation-grade. Disable for an external `DATABASE_URL`. |
| `runner` | `Deployment` | opt-in | Actions executor with a privileged Docker-in-Docker sidecar. Disabled by default. |

Why one API replica: Git's on-disk layout assumes a single writer, and
database migrations run at startup (`OXIDEAN_AUTO_MIGRATE`). Scale the API
by scaling the node, not the replica count — the chart hard-pins replicas
to 1. `web` is stateless and scales freely.

## Exposed endpoints

| Path | Resource | Default |
|------|----------|---------|
| HTTP/HTTPS | `Ingress` → `web:3000` | `ingress.enabled=true` |
| Git over SSH | `Service` `LoadBalancer` → `api:2222` | external port 22, configurable |

Ingress is deliberately simple: one hostname routes UI **and** API because
`web` owns the proxying. TLS terminates at your ingress controller — set
`ingress.host` (and `ingress.tls` / `ingress.className`), then point
`publicOrigin` at the `https://` URL. `OXIDEAN_WEB_BEHIND_PROXY=1` is set
automatically when the ingress is enabled.

Raw TCP can't ride an HTTP ingress, so SSH gets its own `LoadBalancer`
service. On clouds that issue one address per LoadBalancer this is a
second IP/DNS name — advertise it via `ssh.host` + `ssh.port` (the chart
falls back to `ingress.host` + `22`). On a single-IP cluster (or bare
metal) either give `LoadBalancer` a provider (MetalLB, kube-vip, or the
distribution's built-in LB) or map port 22 through your ingress
controller's TCP services (e.g. ingress-nginx `tcp-services` ConfigMap),
set `ssh.service.type: ClusterIP`, and keep `ssh.host` == `ingress.host`.

## Prerequisites

- Kubernetes 1.27+ (`helm template` works everywhere; the smoke lane uses kind)
- Helm 3.18+
- A default `StorageClass` that provisions `ReadWriteOnce` volumes
- For the ingress: any `IngressController` (nginx, Traefik, …)

## Quick start — kind evaluation

```bash
helm install ox oci://ghcr.io/oxidean/charts/oxidean \
  --namespace oxidean --create-namespace \
  --set ingress.enabled=false \
  --set ssh.service.type=ClusterIP \
  --set publicOrigin=http://localhost:3000

kubectl -n oxidean rollout status deploy/ox-oxidean-api --timeout=10m
kubectl -n oxidean port-forward svc/ox-oxidean-web 3000:3000
# → http://localhost:3000
```

On first boot the api runs migrations (`OXIDEAN_AUTO_MIGRATE=1`), generates
its SSH host keys into `/var/ssh`, and provisions the web-flow signing key
pair there via an init container. The generated app/admin secrets live in
the release Secret (`<release>-oxidean-secrets`); `helm status` and
`NOTES` print how to read the admin password.

For a real cluster, prefer `helm show values` → write a `my-values.yaml`:

```bash
helm show values oci://ghcr.io/oxidean/charts/oxidean > my-values.yaml
helm upgrade --install ox oci://ghcr.io/oxidean/charts/oxidean \
  -n oxidean --create-namespace -f my-values.yaml
```

## Storage modes

`storage.backend` picks how non-Git state is stored:

### `fs` (default)

One `PersistentVolumeClaim` (`<release>-oxidean-data`) mounts at `/var` and
carries every store: bare repos, LFS objects, packages, release assets,
uploads, template packs, VCS objects, action logs/artifacts/cache. A second
small PVC (`<release>-oxidean-ssh-keys`) keeps `/var/ssh` (host keys + the
web-flow signing key) separate so a rotating host key never shares fate
with repo data — and because it stays a **PVC**, key material never lands
in etcd or an `emptyDir`.

### `s3`

Point `storage.s3.bucket` (+ `prefix`, `endpoint`, `region`, `pathStyle`,
`insecureHttp`) at an S3-compatible object store. Only the POSIX-mandatory
volumes remain: `/var/repos` (Git) and `/var/ssh` (keys); everything else is
intended to resolve through `crates/oxidean-storage`'s `s3` backend.

**Bundled store (`objectstore`)** — `objectstore.enabled=true` deploys a
single-node RustFS (S3-compatible, Apache-2.0) on its own PVC
(`<release>-oxidean-objectstore`) behind a ClusterIP Service. With
`storage.backend=s3` and no explicit `storage.s3.endpoint`, the api's
`OXIDEAN_S3_*` envs default to `http://<release>-oxidean-objectstore:9000`
(path-style addressing, `OXIDEAN_S3_INSECURE_HTTP=1` for the in-cluster
http endpoint) and `storage.s3.bucket` falls back to `objectstore.bucket`
(default `oxidean`). A `post-install,post-upgrade` hook Job creates the
bucket. Credentials are one shared pair in the chart Secret (or your
`secrets.existingSecret`'s `OXIDEAN_S3_ACCESS_KEY_ID` /
`OXIDEAN_S3_SECRET_ACCESS_KEY` keys): the store reads them as `RUSTFS_*`
root keys and the api signs with them. Explicit `storage.s3.accessKeyId` /
`secretAccessKey` override the generated pair — and then double as the
store's root keys. Eval-grade like bundled postgres — production:
`objectstore.enabled=false` + external `storage.s3.*`. Every explicit
`storage.s3.*` value overrides its bundled default field-by-field, so you
can also keep the bundled store while pointing the api elsewhere.

**Bundled store (`seaweedfs`)** — `seaweedfs.enabled=true` deploys
[SeaweedFS](https://seaweedfs.com) via its upstream subchart: master /
volume / filer StatefulSets on `persistentVolumeClaim` storage (never the
upstream `hostPath` default) plus an S3 gateway on
`<release>-seaweedfs-s3:8333`. This is the production-shaped bundled
option — a real distributed-store topology (master / volume / filer
StatefulSets) vs RustFS's single-node eval profile. The default sizing is
one replica per component with replication off, so it is still not HA out
of the box — quorum and volume replication need explicit replica and
placement values (see the subchart's `master.replicas`, `volume.replicas`,
and replication settings). With `storage.backend=s3` and no explicit `storage.s3.endpoint`,
the api's `OXIDEAN_S3_*` envs default to `http://<release>-seaweedfs-s3:8333`
and the same hook Job creates the bucket. Zero-config: the subchart mints
the admin pair once into `<release>-seaweedfs-s3-secret`
(`admin_access_key_id` / `admin_secret_access_key`) and the api keyrefs
those keys directly, so installs and upgrades never read credentials out
of the cluster. `objectstore.enabled` and `seaweedfs.enabled` are mutually
exclusive — templating fails closed if both are set; external
`storage.s3.endpoint` still overrides either bundled provider.

**Your own claims / shared storage** — every chart-managed PVC accepts
`existingClaim` and a configurable `accessModes` list: `storage.data`,
`storage.s3.repos`, `ssh.hostKeys`, `postgres.persistence`,
`objectstore.persistence`, and `runner.persistence`. Point a key at an
existing claim and the chart skips its own PVC and mounts yours — an
EFS/CephFS/Longhorn `ReadWriteMany` claim is the shared-storage story.
Two caveats: `ReadWriteMany` does **not** make the api multi-replica (it
stays pinned to 1 — shared storage shortens failover reattach, it does
not give active/active), and chart PVCs carry
`helm.sh/resource-policy: keep`, so switching a key to `existingClaim`
keeps the previously managed claim instead of deleting it — migrate the
data yourself, then delete the old claim explicitly. The bundled
SeaweedFS subchart's own PVCs follow its values, not these keys.

> **Honesty note (OPS-02 in-flight):** the `BlobStore` abstraction exists
> but API consumer migration is still landing — today `s3` mode mounts the
> slim PVCs and exports `OXIDEAN_S3_*`, while non-Git stores still write to
> the api container's filesystem. Keep `storage.s3.retainDataVolume: true`
> (the default) so `/var` also gets a PVC until the stores move behind the
> abstraction; set it `false` only once OPS-02 consumer migration has
> shipped. Credentials: `accessKeyId`/`secretAccessKey` must be set
> together (static keys) or neither (ambient AWS credentials / IRSA).
> Templating fails closed on a half-pair.

## Database

`postgres.enabled: true` (default) installs a bundled `postgres:18-alpine`
with its own PVC — **evaluation-grade**, mirroring what the Compose bundle
ships for local bring-up. The api's `DATABASE_URL` is assembled in-cluster
from the generated `POSTGRES_PASSWORD` secret.

For anything real, run Postgres outside the chart:

```yaml
postgres:
  enabled: false
database:
  existingSecret: my-db-secret   # supplies the connection URL
  urlKey: url                    # key inside that Secret (default: DATABASE_URL)
```

or leave `database.existingSecret` empty and put `DATABASE_URL` in
`secrets.existingSecret` (it is `envFrom`-merged — with postgres disabled
the api also reads `DATABASE_URL` from that Secret explicitly). The only
custom key-name support is `database.urlKey` for `database.existingSecret`
— there is no rename knob for the `secrets.existingSecret` key.

## Secrets

On first install the chart generates and persists:

- `OXIDEAN_ACTIONS_SECRETS_KEY` — Actions secrets encryption
- `OXIDEAN_SESSION_SECRET` — session signing
- `OXIDEAN_ADMIN_PASSWORD` — bootstrap admin password, only when
  `admin.email` or `secrets.adminPassword` is set (the default `/setup`
  wizard flow mints no password key)
- `POSTGRES_PASSWORD` — only when `postgres.enabled`

Generation is `lookup`-guarded: `helm upgrade` reuses whatever is already
in the cluster and never rotates your secrets. To bring your own, create
the Secret yourself and set:

```yaml
secrets:
  existingSecret: my-oxidean-secrets   # envFrom'd into the api pod
database:
  existingSecret: my-database-secret   # optional separate DATABASE_URL holder
```

Any keys not present in your Secret fall back to the chart-generated ones
when `existingSecret` is unset; when it is set the chart renders **no**
Secret at all — your Secret is authoritative.

## Actions runner (opt-in)

```yaml
runner:
  enabled: true
```

Ships `ghcr.io/oxidean/oxidean-runner` plus a `docker:28-dind` sidecar so
`docker://` jobs run. The chart bypasses the stock dind entrypoint and
invokes `dockerd` with a loopback-only listener
(`tcp://127.0.0.1:2375` + the container-local socket), so the daemon API
never leaves the pod's network namespace — the runner container reaches
it via the shared `DOCKER_HOST`, nothing else can.

**Security note:** the dind container still runs `privileged: true` — it
can reach the host kernel. That's the documented tradeoff for
containerized CI on shared nodes; pin runner pods to dedicated nodes with
`runner.nodeSelector` / `runner.affinity` / `runner.tolerations` (they fall
back to the global scheduling keys) in multi-tenant clusters. Rootless
dind / Firecracker isolation is out of scope for this chart (roadmap
follow-up).

## Health and upgrades

Probes are the real endpoints: api `GET /health` on 8080, web `GET /health`
on 3000 carrying the `Oxidean-Health-Probe: 1` header — the same checks the
Compose stack uses.

Upgrading:

```bash
helm upgrade ox oci://ghcr.io/oxidean/charts/oxidean -n oxidean
```

- Secrets persist (lookup-guarded) — nothing rotates.
- `strategy: Recreate` on the api frees the RWO volume before the new pod
  attaches; expect a short write outage during the swap.
- `OXIDEAN_AUTO_MIGRATE=1` (default) applies pending migrations on boot.

## What the chart does not do (yet)

- Multi-replica api / Gitaly-style repo sharding — out of scope, single
  writer by design.
- Gateway API `HTTPRoute`/`TCPRoute` — ingress-only for v1.
- `PodDisruptionBudget`/`HorizontalPodAutoscaler` for web, NetworkPolicies,
  Helm test hooks — tracked follow-ups.
- Consumer migration of every non-Git store behind `BlobStore` — OPS-02
  follow-up; see the `s3` honesty note above.
