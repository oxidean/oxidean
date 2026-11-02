# Migrating a Compose deployment to Kubernetes

Move an existing `deploy/selfhost/docker-compose.yml` install onto the Helm
chart (`deploy/helm/oxidean/`) without losing repositories, accounts, or SSH
trust. Read [SELFHOST-KUBERNETES.md](SELFHOST-KUBERNETES.md) first — this
doc assumes you know the chart's value surface.

The short version: the Compose stack and the chart mount the same logical
state — a Postgres database, one `/var` data volume holding git repos and
all other stores, SSH host keys, and the `OXIDEAN_*` secrets. Migration is
"copy those four things into cluster equivalents, then repoint DNS."

> **Order matters.** Stop writes on the source before copying — a
> `git push` or a DB write between the dump and the cutover is silently
> lost.

## What maps to what

| Compose | Helm | Notes |
|---------|------|-------|
| `postgres` service / `postgres_data` volume | external DB via `database.*` (recommended) or `postgres.enabled` bundled PVC | External is the production path; bundled is eval-grade |
| `oxidean_data` volume (`/var`) | `storage.data` PVC (fs mode) | Same layout — repos, LFS, releases, packages, pages, actions data |
| `ssh_host_keys` volume | `ssh.hostKeys` PVC | Copy keys verbatim — clients pin fingerprints |
| `objectstore_data` volume (`s3` profile) | external `storage.s3.*` endpoint (objects don't move) or copy into `objectstore.persistence` / SeaweedFS | Simplest: keep the same endpoint |
| `runner_data` volume | `runner.persistence` PVC | Re-registration also works — cheap to skip |
| `.env` `OXIDEAN_*` secrets | `secrets.existingSecret` keys | Same key names, 1:1 |
| Traefik/Caddy edge | `Ingress` + `ssh` LoadBalancer Service | TLS moves to the ingress controller |

Chart PVCs carry `helm.sh/resource-policy: keep`, so an `existingClaim`
switch or an uninstall never garbage-collects them — the migration claims
you create below survive the chart lifecycle until you delete them
yourself.

## Step 1 — Freeze the source

```bash
cd deploy/selfhost
docker compose stop api web   # postgres + objectstore keep running for the copy
```

No new pushes, issues, or logins from this point on.

## Step 2 — Dump Postgres

```bash
docker compose exec -T postgres \
  sh -c 'pg_dump -U "$POSTGRES_USER" --format=custom --no-owner "$POSTGRES_DB"' \
  > oxidean-db.dump
```

`-T` disables the TTY `docker compose exec` allocates by default (it
corrupts a binary dump), and the `sh -c` wrapper expands
`POSTGRES_USER`/`POSTGRES_DB` **inside** the container where the Compose
file defines them — the host shell does not have them.

If the Compose stack ran MySQL or SQLite instead, migrate the engine first
(export/import) or keep the same engine as an external `database.url` —
the k8s target must run the same dialect the dump came from.

## Step 3 — Provision the target

Pick the production posture: external Postgres (`postgres.enabled=false`)
and whatever storage mode you want. Two PVC strategies — pick one:

- **Let the chart create claims** (`storage.data`, `ssh.hostKeys`, …) and
  copy data in. Simplest.
- **Pre-create claims yourself** and pass them via `existingClaim` —
  required when you want a specific StorageClass, an already-provisioned
  EFS/CephFS filesystem, or the data seeded by an out-of-band job.

Create the namespace and the database Secret first — the install
references it:

```bash
kubectl create namespace oxidean
kubectl -n oxidean create secret generic ox-db \
  --from-literal=DATABASE_URL="postgres://oxidean:<target-password>@<db-host>:5432/oxidean"
```

The key name is the chart default (`database.urlKey=DATABASE_URL`), so no
extra `--set` is needed. Then install the chart so its claims exist:

```bash
helm install ox oci://ghcr.io/oxidean/charts/oxidean \
  --namespace oxidean \
  --set publicOrigin=https://git.example.com \
  --set postgres.enabled=false \
  --set database.existingSecret=ox-db \
  --set secrets.existingSecret=ox-secrets
kubectl -n oxidean scale deploy/ox-oxidean-api --replicas=0   # keep it off for the copy
```

(`ox-secrets` is created in Step 6 — before the api scales up in Step 8.
Only the api reads it, so the order is safe.)

**Bundled-Postgres variant** — if you keep `postgres.enabled=true` instead of
an external database, the postgres pod reads `POSTGRES_PASSWORD` from
`ox-secrets` at first boot, so it must exist *before the install* (not in
Step 6). Create it early, drop the external-database flags, and use that
same password for the Step 4 restore:

```bash
kubectl -n oxidean create secret generic ox-secrets \
  --from-literal=POSTGRES_PASSWORD="<target db password>"   # plus the OXIDEAN_* keys from Step 6
helm install ox oci://ghcr.io/oxidean/charts/oxidean \
  --namespace oxidean \
  --set publicOrigin=https://git.example.com \
  --set secrets.existingSecret=ox-secrets
# no postgres.enabled/database.* sets — the bundled path is the default
```

## Step 4 — Load the database

Run a throwaway postgres client pod, copy the dump in, then restore —
`kubectl cp` needs a pod that is already Running, so a one-shot
`--rm` command cannot work:

```bash
kubectl -n oxidean run pg-restore --restart=Never \
  --image=postgres:18-alpine --command -- sleep 3600
kubectl -n oxidean wait --for=condition=Ready pod/pg-restore --timeout=60s
kubectl -n oxidean cp oxidean-db.dump pg-restore:/tmp/oxidean-db.dump
kubectl -n oxidean exec pg-restore -- env PGPASSWORD="<target-password>" \
  pg_restore -h <db-host> -U <db-user> -d <db-name> \
  --clean --if-exists --no-owner /tmp/oxidean-db.dump
kubectl -n oxidean delete pod pg-restore
```

For the bundled-postgres path the same restore runs against
`ox-oxidean-postgres:5432` with the chart's generated `POSTGRES_PASSWORD`
(`kubectl -n oxidean get secret ox-oxidean-secrets -o jsonpath='{.data.POSTGRES_PASSWORD}' | base64 -d`).

## Step 5 — Copy the data volume

The `/var` tree is the bulk: `repos/`, `lfs/`, `releases/`, `packages/`,
`pages/`, `actions/`, `uploads/`, `templates/`. Tar it on the source host
and expand it into the target PVC through a temporary pod.

On the compose host:

Resolve the volume the api actually mounts — the Compose project name
prefix varies, so never guess `oxidean_selfhost_oxidean_data`:

```bash
API_CID="$(docker compose ps --all -q api)"   # --all: Step 1 stopped api
[ -n "$API_CID" ] || { echo "api container not found"; exit 1; }
DATA_VOL="$(docker inspect "$API_CID" \
  --format '{{ range .Mounts }}{{ if eq .Destination "/var" }}{{ .Name }}{{ end }}{{ end }}')"
[ -n "$DATA_VOL" ] || { echo "no /var volume mounted on $API_CID"; exit 1; }
docker run --rm -v "$DATA_VOL":/var:ro -v "$PWD":/out \
  alpine tar czf /out/oxidean-var.tgz -C /var .
```

In the cluster — a throwaway pod mounting the data PVC:

```yaml
apiVersion: v1
kind: Pod
metadata: { name: ox-var-seed, namespace: oxidean }
spec:
  restartPolicy: Never
  containers:
    - name: seed
      image: alpine
      command: ["sh", "-ec", "apk add --no-cache tar && tar xzf /in/oxidean-var.tgz -C /var && tail -f /dev/null"]
      volumeMounts:
        - { name: data, mountPath: /var }
        - { name: in,  mountPath: /in }
  volumes:
    - name: data
      persistentVolumeClaim:
        claimName: ox-oxidean-data          # or your existingClaim
    - name: in
      emptyDir: {}
```

`kubectl cp oxidean-var.tgz oxidean/ox-var-seed:/in/` works too — drop the
copy step that suits you. Verify (`ls /var/repos` inside the pod), then
delete the seed pod.

## Step 6 — Port the secrets

Every `OXIDEAN_*` value in the compose `.env` becomes a key in
`secrets.existingSecret`. The ones that MUST move verbatim:

```bash
kubectl -n oxidean create secret generic ox-secrets \
  --from-literal=OXIDEAN_ACTIONS_SECRETS_KEY="$OXIDEAN_ACTIONS_SECRETS_KEY" \
  --from-literal=OXIDEAN_SESSION_SECRET="$OXIDEAN_SESSION_SECRET" \
  --from-literal=OXIDEAN_SECRETS_KEY="$OXIDEAN_SECRETS_KEY" \
  --from-literal=POSTGRES_PASSWORD="<target db password>"
```

Losing `OXIDEAN_ACTIONS_SECRETS_KEY` / `OXIDEAN_SECRETS_KEY` makes every
stored Actions secret, TOTP secret, and mirror/webhook credential
undecryptable — they are the keys the vault data was encrypted with.
`OXIDEAN_SESSION_SECRET` can be regenerated (logs everyone out once);
the rest cannot.

## Step 7 — SSH host keys

Copy the Compose `ssh_host_keys` volume contents verbatim into the
`ssh.hostKeys` PVC the same way as Step 5 — these are the fingerprints
every user's SSH client already trusts. Regenerating them means every
user hits a `REMOTE HOST IDENTIFICATION HAS CHANGED` warning.

## Step 8 — Cut over

```bash
kubectl -n oxidean scale deploy/ox-oxidean-api --replicas=1
kubectl -n oxidean rollout status deploy/ox-oxidean-api --timeout=10m
```

Then:

1. Point `git.example.com` DNS at the ingress address, and the SSH hostname
   at the `ox-oxidean-ssh` LoadBalancer address (`kubectl -n oxidean get
   svc ox-oxidean-ssh` — MetalLB/kube-vip for bare metal).
2. Confirm `publicOrigin` and `ssh.host`/`ssh.port` match the new URLs.
3. Smoke it: clone over SSH, clone over HTTPS, open a PR, hit `/health`
   on both tiers.
4. Keep the compose stack stopped (not deleted) for a few days — it is
   the rollback.

## Rollback

**Only safe before the cluster serves writes.** While Kubernetes has not
accepted a single push or login, stopping its api and pointing DNS back
at the compose host restores service exactly as it was.

Once the cluster has taken writes, switching back without a reverse
transfer silently discards them — Steps 2–5 only copy *from* compose
*to* Kubernetes. To roll back after cluster writes, run the same
procedure in reverse first: dump the target database, restore it into
the compose postgres, and copy the PVC contents back onto the compose
volumes — then cut DNS over.

## Notes and edge cases

- **The s3 picture is one hop simpler**: if the compose stack already ran
  the `s3` profile, point `storage.s3.endpoint` at the same bucket — no
  object copy needed. If it ran bundled RustFS and you want to keep those
  objects in-cluster, copy the `objectstore_data` volume like Step 5.
- **`runner_data`** holds only registration state — re-register the
  runner (`admin.actions.createRegistrationToken` → `runner.
  registrationToken`) rather than copying it.
- **fs→s3 as part of the move** is fine *for the env contract*, but the
  api's store-by-store `BlobStore` migration (OPS-02) is still landing —
  keep `storage.s3.retainDataVolume: true` (default) so `/var` persists
  alongside the s3 envs.
- **LFS objects under `/var/repos/.lfs`** move inside the same tarball —
  no separate step.
- **The `/setup` wizard does not re-run** — admin accounts live in the
  database you restored.
