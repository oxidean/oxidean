# Self-hostable object storage — plan

Status: implemented — verified locally (helm lint + render matrix, kind
install → s3+objectstore upgrade → real `s3 ls`, selfhost compose `--profile s3`
smoke). Deviations from the plan: none; spec README is generic prose (no
per-spec index exists to update).

## Tasks

1. **Helm chart — bundled RustFS**
   - `values.yaml`: top-level `objectstore:` section (enabled, image repo/tag
     `rustfs/rustfs:1.0.4`, bucket default `oxidean`, persistence
     size/storageClass, resources, console toggle).
   - `_helpers.tpl`: `objectstoreServiceName`, `objectstorePvcName`,
     `s3Endpoint` (bundled default vs explicit), `s3Bucket` default chain,
     `s3Bundled` predicate.
   - `templates/deployment-objectstore.yaml` + `service-objectstore.yaml`:
     1 replica, Recreate, `RUSTFS_*` envs via secretKeyRef (chart Secret or
     `secrets.existingSecret` — same `OXIDEAN_S3_*` keys feed both pods),
     probes on the MinIO-compatible health endpoints, `/data` PVC.
   - `templates/job-bucket-init.yaml`: `post-install,post-upgrade` hook Job,
     `amazon/aws-cli` image, waits for the endpoint then head-bucket/mb —
     idempotent; `hook-delete-policy: before-hook-creation,hook-succeeded`.
   - `templates/secret.yaml`: emit generated `OXIDEAN_S3_ACCESS_KEY_ID` /
     `OXIDEAN_S3_SECRET_ACCESS_KEY` when bundled and no explicit creds
     (lookup-guarded, same one-time-generation pattern as other keys).
   - `templates/deployment-api.yaml`: s3 env block gains bundled defaults —
     endpoint/path-style/insecure-http/bucket fall back to the service when
     `objectstore.enabled`; explicit `storage.s3.*` still wins per field.
   - `templates/pvc.yaml`: objectstore PVC.
2. **Compose parity — `s3` profile**
   - `deploy/selfhost/docker-compose.yml`: `objectstore` + `objectstore-init`
     services under `profiles: ["s3"]`; api env emits the full
     `OXIDEAN_STORAGE_BACKEND`/`OXIDEAN_S3_*` contract with bundled defaults
     and shared credentials; `objectstore_data` volume; header + usage note.
   - Root `docker-compose.yml`: same profile for the dev stack (lax dev
     credential defaults matching the file's convention) so the s3 path is
     exercisable during the `oxidean-storage` consumer rollout.
3. **CI — extend the `helm` lane** (`scripts/ci-helm-smoke.sh`)
   - Render matrix: bundled defaults render, explicit-endpoint override wins,
     bundled+`existingSecret`, bucket-init Job assertions.
   - Kind lane: after the default install passes, `helm upgrade` to
     `storage.backend=s3` + `objectstore.enabled=true`; wait for the
     objectstore rollout, assert the init Job completes, verify a real
     `s3 ls` against the service.
4. **Documentation**
   - `docs/SELF-HOSTING.md`: object-storage section (fs default / `--profile s3`
     bundled / external S3 envs).
   - `docs/SELFHOST-KUBERNETES.md`: objectstore values + topology notes in the
     storage chapter.
   - `docs/CONFIGURATION.md`: `OXIDEAN_STORAGE_*` / `OXIDEAN_S3_*` rows (absent
     today — the contract lives only in `config.rs` doc comments).
   - `.env.example` + `deploy/selfhost/.env.example`: storage block.
   - `deploy/helm/oxidean/README.md` + `values.yaml` comments.
5. **Agent-facing docs**
   - `AGENTS.md`: short deployment-surface parity map (selfhost compose ↔ helm
     chart ↔ same `OXIDEAN_*` contract; bundled store = RustFS; authoritative
     env contract in `crates/oxidean-storage/src/config.rs`).
   - This spec record + `agent-os/specs/README.md` index + roadmap **OPS-07**.
6. **Local verification**
   - `helm lint` + full render matrix.
   - kind: default install → upgrade to bundled s3; pod/PVC/Job checks plus a
     real `aws s3 ls` from inside the cluster.
   - compose: `--profile s3 up -d` on the self-host file; objectstore healthy,
     bucket created, api env present, api `/health` green.

## Follow-ups (out of scope)

- TLS for the bundled endpoint (in-cluster http is the eval default).
- Bucket lifecycle/versioning policies beyond create-if-missing.
- RustFS console exposure knob (console stays off; toggle exists for debugging).
- Second-bucket/per-store-bucket topology — one bucket + prefixes by design.
- Consumer-side `BlobStore` migration status lives with the storage spec, not
  here.
