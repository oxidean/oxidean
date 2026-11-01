# S3-Compatible Blob Storage Backend — Shaping Notes

> Recorded 2026-11-01 alongside the foundation PR (#214). Work was shaped
> on issue [#210](https://github.com/oxidean/oxidean/issues/210); this spec
> is the durable decision record it produced.

## Scope

One codebase for Oxidean Cloud and self-hosted installs, and today every
byte of forge state lives on the local filesystem — `AppState` carries
roughly a dozen filesystem roots
(`crates/oxidean-api-core/src/app.rs`), each bind-mounted per deployment
topology:

| Store | Env var | Content model | Needs POSIX? |
|-------|---------|---------------|--------------|
| Bare git repos | `OXIDEAN_REPOS_DIR` | real git (loose objects, refs, packed refs, ref-transaction locks) | yes — system `git` requires a local fs and single-writer semantics |
| Git LFS objects | `OXIDEAN_LFS_DIR` | content-addressed `{aa}/{bb}/{oid}`, sha256-verified on write | no |
| Release assets | `OXIDEAN_RELEASE_ASSETS_DIR` | keyed blobs | no |
| Package blobs | `OXIDEAN_PACKAGES_DIR` | content-addressed | no |
| Template packs | `OXIDEAN_TEMPLATE_PACKS_DIR` | content-addressed zips | no |
| Agentic VCS objects | `OXIDEAN_VCS_OBJECTS_DIR` | content-addressed + DB refcount GC | no |
| Uploads / avatars | `var/uploads` | keyed blobs | no |
| Actions logs | `OXIDEAN_ACTIONS_LOG_DIR` | append-then-freeze files | append only — rest no |
| Actions artifacts | `OXIDEAN_ACTIONS_ARTIFACTS_DIR` | keyed tarballs | no |
| Actions cache | `OXIDEAN_ACTIONS_CACHE_DIR` | keyed tarballs | no |
| Pages sites | `OXIDEAN_PAGES_DIR` | materialized trees | read-many |
| SSH host keys | `OXIDEAN_SSH_HOST_KEY_DIR` | tiny secret files | K8s Secret / small PVC |
| CLI dist | `OXIDEAN_CLI_DIST_DIR` | operator-managed binaries | fs (ephemeral cache) |

This blocks the platform goals: the `api` container cannot scale past one
replica safely (RWO volume semantics for every store), a Helm chart would
need a large RWO volume per install, and cloud installs pay volume prices
for cold blob data that object storage serves natively and cheaper.

Constraints that shaped the decision:

- `git` (the CLI, via `CliGitBackend`) does loose-object writes, ref
  transaction locks, `gc`, and `receive-pack` on local paths. There is no
  object-storage git backend short of building a Gitaly-class service;
  that is out of scope here.
- Self-host single-node Compose must keep working with **zero new
  infrastructure** — the `fs` behavior today is the product's default.
- Object storage is the one API every target platform already provides:
  AWS S3, MinIO, Garage, SeaweedFS, Ceph RGW, Cloudflare R2.

## Decisions

Adopt the **decoupled hybrid** — the same architecture GitLab runs
(Gitaly for git, object storage for everything else):

1. **Raw git repositories stay on a POSIX single-node volume.**
   `OXIDEAN_REPOS_DIR` remains a real filesystem (Compose bind mount /
   RWO PVC). Repo HA via sharding or a Gitaly-style service split is a
   *separate* decision; `api` stays single-replica until then, while
   `web` scales freely.

2. **Every other persistent store moves behind a `BlobStore`
   abstraction** (`crates/oxidean-storage`) with two backends:
   - `fs` — exactly today's semantics (temp-write → verify → atomic
     rename). It remains the default; Compose and `make dev` are
     unchanged.
   - `s3` — a generic S3-compatible client (via the `object_store`
     crate) with configurable endpoint, region, path-style toggle,
     static credentials, and the ambient AWS credential chain
     (env / web identity / ECS / instance metadata).

3. **One bucket, per-store key prefixes** (`{global_prefix}/{store}/…`),
   never bucket-per-store. The existing `OXIDEAN_*_DIR` vars stay
   authoritative in `fs` mode; in `s3` mode the per-store prefix takes
   their place.

4. **Delivery mode is decided per store, not per backend.** Presigned
   URLs are minted only where (a) content is public, (b) auth is already
   a bearer token, or (c) the ACL check immediately precedes the mint —
   presigning changes *who serves* the bytes, never *who decides*. Where
   a response needs security headers object storage cannot express
   (Pages `text/html` needing `sandbox allow-scripts` CSP, avatars
   needing server-side re-encode), the API keeps proxy-streaming.

### Security invariants (binding)

- **No public buckets.** Presigned URLs are single-object and
  short-TTL, minted only after the existing authorization check runs.
- **Keys derive only from validated identifiers** — `validate_oid`,
  `storage_rel`, DB ids — never raw client input.
- **Credentials come from env / K8s Secrets only**, are never logged,
  and the ambient AWS credential chain is supported so AWS installs
  need no static keys.
- **Fail-closed boot.** Each store prefix carries an ownership marker
  (`{prefix}/.oxidean-{store}-marker`), mirroring the VCS object
  store's marker-verified root (`ObjectStoreConfig`): a non-marker
  object without a marker means a foreign prefix and is refused, and
  two stores may never resolve to the same `(bucket, prefix)`.
  Misconfigured or unreachable backends refuse boot or disable the
  affected surface — never silently fall back to `fs`.
- **Integrity is enforced at commit.** `fs` keeps inline sha256
  verification; `s3` binds `x-amz-checksum-sha256` and `Content-Length`
  into presigned PUTs where supported and verifies size via `head`
  before an object is marked committed.
- **Stage → scan → commit seam.** The write path exposes a staged
  upload that becomes visible only at commit, so content scanning
  (e.g. ClamAV on package/release uploads) can slot in later without
  an API change.
- **Endpoint trust.** The S3 endpoint is operator configuration — same
  trust level as `DATABASE_URL` — never settable from user input and
  exempt from the user-driven egress SSRF rules; `http://` endpoints
  require an explicit `OXIDEAN_S3_INSECURE_HTTP` opt-in.

## Alternatives considered

- **A. All-in on object storage (including git).** Mount FUSE/s3fs or a
  git-object gateway in front of `repos_dir`. Rejected: git's
  loose-object + lock-file + `receive-pack` semantics do not survive an
  eventually-consistent, non-POSIX store; correctness and data safety
  come first. This is the path that leads to building Gitaly
  prematurely.
- **B. Stay filesystem-only, add shared storage for HA later.**
  NFS/EFS for everything. Rejected: it does not decouple the app from
  one vendor, pays performance taxes on small-file workloads, and does
  not give presigned-URL offload — the single biggest bandwidth win for
  LFS and release assets.
- **C. Bucket-per-store topology.** A bucket each for LFS, packages,
  release assets, etc. Rejected as the default: more operator surface
  (N buckets × policies × lifecycle rules) for no isolation gain — the
  app already enforces per-store ACLs. Prefix-level isolation with the
  marker gate carries the same cross-store invariant (`D-PKG-07`,
  `D-ACT-13`) into object storage. Operators can still give stores
  distinct `(bucket, prefix)` pairs later if they need it — the
  collision check then becomes load-bearing.
- **D. Full `aws-sdk-s3` / cloud-SDK clients.** Rejected: the AWS SDK
  pulls a large service graph for what is a seven-method store, and it
  still needs shims for MinIO/Garage quirks. `object_store` gives
  `AmazonS3` + `LocalFileSystem` impls, presigning, endpoint +
  path-style support, and multipart upload behind one trait with a much
  smaller dep tree.

## Consequences

- `api` converges on a single required RWO volume (`/var/repos`) plus a
  tiny `/var/ssh`; every other byte can live in object storage, which
  unblocks a Helm chart and multi-replica `web`.
- `fs` remains the default everywhere (Compose, `make dev`, self-host
  single volume) — `OXIDEAN_STORAGE_BACKEND` unset is byte-identical to
  today.
- Actions logs lose POSIX append in `s3` mode: live-tail runs stage
  locally and `put` once at terminal state — in-flight log bytes then
  die with the pod, matching today's recreate semantics.
- The `BlobStore` marker gate means a pointed-at-wrong-bucket error
  refuses boot instead of GC-sweeping somebody else's objects.
- Presigned delivery shifts upload integrity partially to the client:
  mitigated by signed checksum/size constraints plus post-upload `head`
  verification; the residual trust shift is documented at the signing
  call sites as consumers migrate.
- Follow-up phases (consumer migration store-by-store, multipart
  mapping for Actions artifacts, a MinIO self-host overlay, and
  migration tooling) are tracked on issue #210 and ship independently;
  this spec is the contract they implement.

## Context

- **Origin:** issue [#210](https://github.com/oxidean/oxidean/issues/210)
  — store table, configuration surface, and trait shape defined there.
- **Product alignment:** platform-parity track on
  `agent-os/product/roadmap.md` — cloud-native storage unblocks
  multi-replica API and Helm.
- **Precedent:** VCS object store's marker-verified root
  (`ObjectStoreConfig`) — the marker gate reuses that invariant.
