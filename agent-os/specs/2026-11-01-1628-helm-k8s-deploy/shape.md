# Helm/Kubernetes Deployment — Shaping Notes

> Recorded 2026-11-01 as the follow-on to the storage backend spec
> ([`2026-11-01-1425-storage-blob-backend`](../2026-11-01-1425-storage-blob-backend/shape.md)),
> which landed the `BlobStore` abstraction explicitly to unblock this work
> (issue [#210](https://github.com/oxidean/oxidean/issues/210): "this is the
> storage-side unblock for a future Helm chart").

## Scope

A Helm chart that installs the self-hosted Oxidean topology on Kubernetes:
`postgres` + `api` + `web`, matching what `deploy/selfhost/docker-compose.yml`
assembles today, plus the two deployment modes the platform now supports.

Target topology (inherited from the storage spec):

- `api` — single replica, `strategy: Recreate`. Raw git repos stay on a
  POSIX RWO volume because `CliGitBackend` needs loose objects, ref
  transaction locks, and `receive-pack` on local paths; repo HA via
  sharding/Gitaly-style split is a separate decision. Until then `api`
  cannot scale past one replica.
- `web` — stateless, scales freely.
- `postgres` — in-chart dependency by default so `helm install` matches
  compose's zero-config install; production operators point
  `database.*` at an external `DATABASE_URL` instead.
- Every non-git byte goes to object storage when `storage.backend=s3`
  (`crates/oxidean-storage`, PR #214): LFS, release assets, packages,
  template packs, VCS objects, actions logs/artifacts/cache, pages,
  uploads. One bucket, per-store key prefixes.
- SSH host keys live under `/var/ssh` — the app generates them on first
  boot, so this must be a writable dir.

## Decisions

Confirmed 2026-11-01:

1. **Chart at `deploy/helm/oxidean/`**, published as an OCI artifact to
   `oci://ghcr.io/oxidean/charts/oxidean` on the existing release train —
   same registry and tag cadence as the `oxidean-{api,web,runner}` images.

2. **`api` is a `Deployment` with `strategy: Recreate` and a RWO PVC for
   repos** — not a `StatefulSet`. Recreate guarantees single-writer
   semantics during rollout; StatefulSet adds pod identity we cannot use
   until git sharding exists.

3. **Two storage modes via `storage.backend`:** `fs` mounts one RWO PVC
   at `/var` (parity with compose's `oxidean_data` and Railway's
   `forge-data`); `s3` mounts PVCs only for `/var/repos` and `/var/ssh`
   and emits `OXIDEAN_STORAGE_BACKEND` + `OXIDEAN_S3_*` envs from a
   Secret. Self-host zero-infra default stays `fs`.

4. **SSH host keys get a small PVC, not a Secret.** Mounted Secrets are
   read-only; the app writes host keys on first boot. A mounted Secret
   would either crash the writer or freeze keys that must rotate with
   the install. (PVC or nothing — `emptyDir` would churn keys every
   restart and break client trust.)

5. **SSH transport is a dedicated `Service` `type: LoadBalancer`**
   (default port 22 → container 2222), separate from HTTP. Ingress
   controllers don't carry raw SSH; an ingress-nginx `tcp-services`
   ConfigMap mapping is documented as the single-LB alternative.

6. **HTTP via `Ingress` → `web:3000`.** `web` already reverse-proxies the
   api (`/api/*`, `/{o}/{r}.git`, badges, `/pages`, `/.well-known/oauth*`,
   WS/SSE pass-through), so one hostname suffices. TLS through ingress
   annotations/cert-manager; the compose Caddy edge is not templated —
   the ingress replaces it.

7. **Probes use the real health endpoints:** api `GET /health :8080`;
   web `GET /health :3000` with `httpHeaders: Oxidean-Health-Probe: 1`
   (header-gated by design — see `crates/oxidean-web/src/main.rs`).

8. **Secrets: chart-managed Secret with `lookup`-guarded one-time
   generation** for `OXIDEAN_ACTIONS_SECRETS_KEY`, `OXIDEAN_SESSION_SECRET`,
   admin password, and `POSTGRES_PASSWORD`; every value also accepts
   `existingSecret` for operator-managed secrets. Never regenerate on
   upgrade — `lookup` reads the live Secret first.

9. **Migrations ride `OXIDEAN_AUTO_MIGRATE` (default true).** Single api
   replica means no migration race; a pre-install/upgrade hook Job is a
   later option, not v1.

10. **Postgres: `postgres.enabled=true` by default** (Bitnami postgresql
    OCI subchart) so `helm install` gives the same single-command eval
    as compose; production operators set `postgres.enabled=false` and
    supply an external `DATABASE_URL` via `database.*`. Docs mark the
    bundled database as evaluation-grade.

11. **Runner (`actions` profile in compose) is `runner.enabled=false`
    by default** — it needs a privileged dind sidecar or host docker
    socket, a real security tradeoff. Ship the template but gate it;
    hardening is follow-up.

12. **Chart registered in `versioning.toml` with real `kind = "helm"`
    tooling support** — the release tooling stamps `Chart.yaml`
    `version`/`appVersion` in lockstep with the release train instead
    of drifting on a workflow side-channel. Adding a kind is the same
    shape as the existing `contract`/`bun` kinds.

13. **CI gets a `helm` job** — `helm lint`, `helm template` golden
    render, and a kind-cluster install smoke alongside `selfhost-example`
    and `compose-smoke`. Per repo rule, adding a job requires updating
    the Main ruleset required-status-checks list in the same change.

## Alternatives considered

- **Plain manifests / Kustomize** — rejected: Helm is the packaged form
  operators ask for and values templating maps cleanly onto the existing
  `OXIDEAN_*` env contract; Kustomize could be generated later from the
  same values.
- **StatefulSet for `api`** — rejected for v1: no stable-identity need
  until repo sharding; Deployment+Recreate is simpler and equally safe
  for a single writer.
- **Secret for SSH host keys** — impossible: read-only mounts.
- **Caddy inside the chart** — rejected: duplicates the ingress layer.

## Context

- **Visuals:** none.
- **References:** see `references.md`.
- **Product alignment:** mission is one forge, cloud + self-host, one
  Docker-based release train — a Helm chart is the third deploy target
  after Compose (`deploy/selfhost/`) and Railway IaC (`.railway/`).
  Roadmap: OPS-02 (S3 storage) is the landed unblock; this adds the
  missing `OPS-06` line item, and OPS-03 (HA guidance) becomes actionable
  for `web`.

## Standards applied

- `way-of-working` — trunk-based, PR-only, squash-merge; this spec lands
  by PR like the storage spec did.
- `commits-versioning` — commit grammar; chart registration in
  `versioning.toml`.
- `review-practices` — no secrets in repo or default `values.yaml`; CI
  image pinning rules apply to chart dependencies and the kind job.
- `testing` — test-after posture; the `helm` CI job is the chart's gate.
