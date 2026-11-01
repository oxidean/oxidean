# Self-hostable object storage — shape

## Context

`storage.backend: s3` shipped in the Helm chart (spec `2026-11-01-1628-helm-k8s-deploy`)
and `crates/oxidean-storage` ships `fs`/`s3` backends (spec
`2026-11-01-1425-storage-blob-backend`). Both currently assume an **external** S3 —
AWS or a third-party endpoint. A self-hoster who wants the object-storage topology
today must stand up their own S3-compatible server out of band. That breaks the
chart/compose promise that `helm install` / `docker compose up` yields a complete,
batteries-included forge (the same role bundled Postgres plays for the database).

It also means "self-hostable S3" has no first-class answer on either deploy surface:
the self-host compose file has no object-store service at all, and the chart only
renders env vars for an endpoint the operator supplies.

## Decision

Ship a bundled, eval-grade S3-compatible object store on **both** deploy surfaces:

- **Bundled engine: RustFS** (`rustfs/rustfs`, Apache-2.0) — single binary, single
  volume, env-var credentials (`RUSTFS_ACCESS_KEY` / `RUSTFS_SECRET_KEY`), path-style
  addressing by default, MinIO-compatible health endpoints and client ecosystem.
  Evaluation-grade like bundled Postgres: one replica, one PVC, no HA claims.
- **Helm**: top-level `objectstore:` section (mirrors `postgres:`) — Deployment +
  ClusterIP Service + PVC + generated credentials in the chart Secret + a
  `post-install,post-upgrade` bucket-init Job (`amazon/aws-cli`). When
  `storage.backend=s3` and `objectstore.enabled=true`, the api's `OXIDEAN_S3_*`
  envs default to the bundled service (`http://<release>-objectstore:9000`,
  path-style, http) and to shared generated credentials; explicit
  `storage.s3.*` values keep overriding per field for external endpoints.
- **Compose (self-host + dev)**: `s3` profile adding `objectstore` + one-shot
  `objectstore-init` bucket-creation services; api env block emits the full
  `OXIDEAN_STORAGE_BACKEND` / `OXIDEAN_S3_*` contract with bundled defaults —
  inert while the backend stays `fs`, live when set to `s3`. One credential pair
  (`OXIDEAN_S3_ACCESS_KEY_ID` / `OXIDEAN_S3_SECRET_ACCESS_KEY`) feeds both the
  store and the api, so no second secret surface appears.
- **Docs**: developer-facing coverage in `docs/SELF-HOSTING.md`,
  `docs/SELFHOST-KUBERNETES.md`, `docs/CONFIGURATION.md`, both `.env.example`
  files, and the chart README; agent-facing coverage in `AGENTS.md`
  (deployment-surface parity map + authoritative env contract pointer) and this
  spec record.
- **CI**: extend the existing `helm` lane — render matrix gains bundled-objectstore
  cases and the kind smoke upgrades the running release to
  `storage.backend=s3` + bundled store, asserting the bucket-init Job completes
  and the S3 API answers a real `s3 ls`.

## Alternatives considered

- **MinIO** — the traditional default, now disqualified: image distribution
  stopped Oct 2025 (source-only releases; last official image carries an
  unpatched CVE), the repo entered maintenance mode Dec 2025, and the community
  console was removed earlier in the year. Cannot be bundled responsibly.
- **SeaweedFS** — mature Apache-2.0 with a large deployment base, but credential
  provisioning needs an IAM identity file or `s3.configure` shell calls, and the
  master/volume/filer/S3 multi-port model is more machinery than an eval-grade
  bundled component warrants. Remains a supported external endpoint.
- **Garage** — AGPLv3 and built for geo-distribution; single-node works but needs
  layout assignment via its own CLI (node ID + capacity + key + bucket steps) —
  heavy init ceremony inside Helm hooks.
- **OpenMaxIO / Silo** — MinIO forks keeping the drop-in surface alive; both are
  young with small maintainer bases versus RustFS's momentum as the community
  successor.
- **Do nothing** — leaves self-hosters without an object-storage path and makes
  the `s3` backend effectively cloud-only.

## Consequences

- `helm install` with `--set storage.backend=s3 --set objectstore.enabled=true`
  yields a working object-storage topology with zero external dependencies.
- `docker compose --profile s3 up -d` does the same on a single host.
- Production guidance is unchanged: bundled is for evaluation and small installs;
  real deployments point `storage.s3.*` / `OXIDEAN_S3_*` at external object
  storage (managed S3, R2, on-prem SeaweedFS cluster, …) — the chart docs say so.
- The bundled PVC is the durability story — no replication, mirroring bundled
  Postgres's documented posture.
- RustFS is pinned (`1.0.4`) and `objectstore.image.*` is overridable; engines
  with different env contracts still work as external `storage.s3` endpoints.
- Consumer honesty: `oxidean-api` consumes `BlobStore` per the storage spec's
  tracked rollout — until a store migrates, the emitted `OXIDEAN_S3_*` envs are
  inert. Docs state this rather than implying live object storage.
