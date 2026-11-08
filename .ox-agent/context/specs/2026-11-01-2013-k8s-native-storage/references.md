# References for Kubernetes-native storage alternatives

## Similar Implementations

### Bundled RustFS objectstore (OPS-07)

- **Location:** `deploy/helm/oxidean/templates/deployment-objectstore.yaml`,
  `service-objectstore.yaml`, `job-bucket-init.yaml`,
  `templates/_helpers.tpl` (`oxidean.objectstore*`/`oxidean.s3*`),
  `values.yaml` `objectstore:` block
- **Relevance:** the bundled-provider pattern this spec extends —
  endpoint derivation, shared generated credentials, bucket-init Job,
  eval-vs-production posture comments.
- **Key patterns:** `oxidean.s3Bundled` predicate + `s3Endpoint` helper is
  the single seam where a second provider plugs in; the same aws-cli
  `mb` Job works against any path-style S3 endpoint.

### SeaweedFS upstream chart

- **Location:** `seaweedfs/seaweedfs` chart 4.51.x,
  `helm repo add seaweedfs https://seaweedfs.github.io/seaweedfs/helm`
- **Relevance:** the k8s-native bundled provider (OPS-08).
- **Key patterns:** multi-component topology — `master`/`volume`/`filer`
  StatefulSets + `s3` gateway Deployment; s3 service
  `<release>-seaweedfs-s3:8333`; `s3.enableAuth` +
  `s3.credentials.admin.existingSecret`/`accessKeyKey`/`secretKeyKey`
  (env-referenced identity config — our generated pair slots in without
  render-time secrets). **Gotcha:** `master.data`, `volume.dataDirs`, and
  `filer.data` default to `hostPath` — pass-through must pin
  `persistentVolumeClaim`. `fullnameOverride` not needed — the
  `<release>-seaweedfs-s3` name derives from `.Release.Name`.

### Storage env contract

- **Location:** `crates/oxidean-storage/src/config.rs`,
  `docs/CONFIGURATION.md` (`OXIDEAN_STORAGE_BACKEND`, `OXIDEAN_S3_*`)
- **Relevance:** authoritative env names, validation, fail-closed
  behavior — the chart only chooses which endpoint/credentials back it.
- **Key patterns:** custom endpoints default path-style + require
  `OXIDEAN_S3_INSECURE_HTTP` for http:// — both emitted for bundled stores.

### fs volume plan

- **Location:** `deploy/helm/oxidean/templates/pvc.yaml`,
  `templates/deployment-api.yaml` volumes, `_helpers.tpl`
  `dataPvcName`/`reposPvcName`/`sshPvcName`/`dataVolumeMounted`
- **Relevance:** existingClaim/accessModes slots in here — helpers resolve
  the claim name either way, so deployments need no new branching.

## Alternatives Considered

- **MinIO (Operator or chart):** AGPLv3 — excluded as a bundled default in
  an MIT-licensed repo; remains a perfectly good *external*
  `storage.s3.endpoint` (documented in `.env.example`/`CONFIGURATION.md`).
- **Garage:** AGPL — same exclusion; works as external endpoint.
- **Rook-Ceph / ceph-radosgw:** cluster-admin territory, not a chart-level
  dependency; CephFS is covered via the fs `existingClaim`/RWX path.
- **SeaweedFS all-in-one (`weed server -s3`) natively templated:** lighter
  but the same eval class as RustFS — misses the point of a k8s-native
  alternative; the subchart is the proper topology.
- **`objectstore.provider: rustfs|seaweedfs` single knob:** rejected —
  subchart `condition:` needs a concrete boolean values path, so provider
  selection is `seaweedfs.enabled` + a mutual-exclusion fail guard.
