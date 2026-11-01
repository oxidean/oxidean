#!/usr/bin/env bash
# CI fail-closed entrypoint for the Helm chart lane:
#   1. helm lint
#   2. helm template render checks (default + representative overrides)
#   3. kind install smoke — api/web/postgres rollout + health probes
#
# Expects: helm, kubectl, kind on PATH; a kind cluster already created
# (kind-action in CI, `kind create cluster` locally); oxidean-api:ci and
# oxidean-web:ci loadable into the node (CI loads the docker-ci-images
# artifact first). Set HELM_SMOKE_SKIP_KIND_LOAD=1 when the images are
# already in the cluster nodes.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CHART="deploy/helm/oxidean"
NS="ox-smoke"
RELEASE="ox"
API_TAG="${HELM_SMOKE_API_TAG:-ci}"
WEB_TAG="${HELM_SMOKE_WEB_TAG:-ci}"

# helm is needed for lint+render; kubectl/kind only matter for the cluster
# leg (checked below, after the HELM_SMOKE_SKIP_CLUSTER early exit).
if ! command -v helm >/dev/null 2>&1; then
  echo "FAIL: 'helm' not on PATH" >&2
  exit 1
fi

echo "==> helm dependency build (vendored subcharts)"
# charts/ is gitignored — on a clean checkout the resolver needs the URL
# registered locally before it can fetch the pinned Chart.lock entry.
helm repo add seaweedfs https://seaweedfs.github.io/seaweedfs/helm >/dev/null
helm dependency build "$CHART"

echo "==> helm lint"
helm lint "$CHART"

echo "==> helm template — default values"
RENDERED="$(helm template "$RELEASE" "$CHART" -n "$NS")"
for expected in "kind: Deployment" "kind: Service" "kind: Secret" "kind: PersistentVolumeClaim" "kind: Ingress"; do
  if ! grep -q "$expected" <<<"$RENDERED"; then
    echo "FAIL: default render missing $expected" >&2
    exit 1
  fi
done
grep -q 'name: ox-oxidean-data' <<<"$RENDERED" \
  || { echo "FAIL: fs backend must render the data PVC" >&2; exit 1; }

echo "==> helm template — s3 slim mode"
S3="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.backend=s3 \
  --set storage.s3.bucket=ci-bucket \
  --set storage.s3.retainDataVolume=false \
  --set storage.s3.accessKeyId=ci-ak \
  --set storage.s3.secretAccessKey=ci-sk)"
grep -q 'name: ox-oxidean-repos' <<<"$S3" \
  || { echo "FAIL: s3 slim mode must render the repos PVC" >&2; exit 1; }
if grep -q 'name: ox-oxidean-data' <<<"$S3"; then
  echo "FAIL: s3 slim mode must not render the catch-all data PVC" >&2
  exit 1
fi
grep -q 'OXIDEAN_S3_BUCKET' <<<"$S3" \
  || { echo "FAIL: s3 mode must inject OXIDEAN_S3_* env" >&2; exit 1; }

echo "==> helm template — existingSecret mode renders no Secret"
EXT="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set secrets.existingSecret=ops-vault \
  --set database.existingSecret=db-vault \
  --set database.urlKey=url)"
if grep -q '^kind: Secret' <<<"$EXT"; then
  echo "FAIL: existingSecret mode must not render a chart-managed Secret" >&2
  exit 1
fi
grep -q 'name: "ops-vault"' <<<"$EXT" \
  || { echo "FAIL: existingSecret envFrom not rendered" >&2; exit 1; }

echo "==> helm template — runner enabled"
RUNNER="$(helm template "$RELEASE" "$CHART" -n "$NS" --set runner.enabled=true)"
grep -q 'privileged: true' <<<"$RUNNER" \
  || { echo "FAIL: runner dind sidecar must be privileged" >&2; exit 1; }

echo "==> helm template — s3 credential half-pair fails closed"
if helm template "$RELEASE" "$CHART" -n "$NS" \
    --set storage.backend=s3 \
    --set storage.s3.bucket=ci-bucket \
    --set storage.s3.accessKeyId=ci-ak >/dev/null 2>&1; then
  echo "FAIL: s3 half-credentials must abort templating" >&2
  exit 1
fi

echo "==> helm template — bundled objectstore renders and wires s3"
BUNDLED="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.backend=s3 \
  --set objectstore.enabled=true)"
for expected in "name: ${RELEASE}-oxidean-objectstore" "name: ${RELEASE}-oxidean-objectstore-init"; do
  if ! grep -q "$expected" <<<"$BUNDLED"; then
    echo "FAIL: bundled render missing $expected" >&2
    exit 1
  fi
done
grep -q 'value: "http://ox-oxidean-objectstore:9000"' <<<"$BUNDLED" \
  || { echo "FAIL: bundled endpoint must default to the service DNS" >&2; exit 1; }
grep -q 'OXIDEAN_S3_INSECURE_HTTP' <<<"$BUNDLED" \
  || { echo "FAIL: bundled http endpoint must emit OXIDEAN_S3_INSECURE_HTTP" >&2; exit 1; }
grep -q 'helm.sh/hook": post-install,post-upgrade' <<<"$BUNDLED" \
  || { echo "FAIL: bucket init must be a post-install/upgrade hook Job" >&2; exit 1; }

echo "==> helm template — explicit s3.endpoint overrides the bundled store"
OVERRIDE="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.backend=s3 \
  --set objectstore.enabled=true \
  --set storage.s3.endpoint=https://s3.example.com \
  --set storage.s3.bucket=ext-bucket)"
grep -q 'value: "https://s3.example.com"' <<<"$OVERRIDE" \
  || { echo "FAIL: explicit endpoint must win over the bundled default" >&2; exit 1; }
if grep -q 'OXIDEAN_S3_INSECURE_HTTP' <<<"$OVERRIDE"; then
  echo "FAIL: https endpoint must not emit OXIDEAN_S3_INSECURE_HTTP" >&2
  exit 1
fi

echo "==> helm template — bare s3 without bucket or store fails closed"
if helm template "$RELEASE" "$CHART" -n "$NS" \
    --set storage.backend=s3 >/dev/null 2>&1; then
  echo "FAIL: backend=s3 without bucket/objectstore must abort templating" >&2
  exit 1
fi

echo "==> helm template — seaweedfs bundled provider renders and wires s3"
SWFS="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.backend=s3 \
  --set seaweedfs.enabled=true)"
for expected in "name: ${RELEASE}-seaweedfs-s3" "name: ${RELEASE}-seaweedfs-s3-secret"; do
  if ! grep -q "$expected" <<<"$SWFS"; then
    echo "FAIL: seaweedfs render missing $expected" >&2
    exit 1
  fi
done
grep -q 'value: "http://ox-seaweedfs-s3:8333"' <<<"$SWFS" \
  || { echo "FAIL: seaweedfs endpoint must default to the s3 gateway service" >&2; exit 1; }
grep -q 'key: "admin_access_key_id"' <<<"$SWFS" \
  || { echo "FAIL: zero-config seaweedfs must keyref the subchart admin pair" >&2; exit 1; }
if grep -q 'name: ox-oxidean-objectstore$' <<<"$SWFS"; then
  echo "FAIL: seaweedfs mode must not render the RustFS deployment" >&2
  exit 1
fi

echo "==> helm template — bundled stores are mutually exclusive"
if helm template "$RELEASE" "$CHART" -n "$NS" \
    --set objectstore.enabled=true \
    --set seaweedfs.enabled=true >/dev/null 2>&1; then
  echo "FAIL: objectstore.enabled + seaweedfs.enabled must abort templating" >&2
  exit 1
fi

echo "==> helm template — existingClaim mounts and suppresses the PVC"
CLAIM="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.data.existingClaim=shared-efs)"
if grep -q 'name: ox-oxidean-data' <<<"$CLAIM"; then
  echo "FAIL: existingClaim must suppress the chart-managed data PVC" >&2
  exit 1
fi
grep -q 'claimName: shared-efs' <<<"$CLAIM" \
  || { echo "FAIL: existingClaim must be mounted by the api deployment" >&2; exit 1; }

echo "==> helm template — accessModes passes RWX through"
grep -q 'ReadWriteMany' <<<"$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set 'storage.data.accessModes[0]=ReadWriteMany')" \
  || { echo "FAIL: accessModes must render ReadWriteMany" >&2; exit 1; }

echo "==> helm template — bundled-component existingClaims suppress PVCs"
CLAIM="$(helm template "$RELEASE" "$CHART" -n "$NS" \
  --set storage.backend=s3 --set objectstore.enabled=true \
  --set runner.enabled=true \
  --set postgres.persistence.existingClaim=pg-shared \
  --set objectstore.persistence.existingClaim=obj-shared \
  --set runner.persistence.existingClaim=runner-shared)"
PVC_NAMES="$(grep -A3 'kind: PersistentVolumeClaim' <<<"$CLAIM" | grep 'name:')"
# Neither the default names nor the configured claim names may render as
# PVC objects — a guard slip would render pg-shared/obj-shared/runner-shared.
for rendered in 'ox-oxidean-postgres' 'ox-oxidean-objectstore' 'ox-oxidean-runner' \
  'pg-shared' 'obj-shared' 'runner-shared'; do
  if grep -q "$rendered" <<<"$PVC_NAMES"; then
    echo "FAIL: $rendered PVC must be suppressed by existingClaim" >&2
    exit 1
  fi
done
for mounted in 'claimName: pg-shared' 'claimName: obj-shared' 'claimName: runner-shared'; do
  grep -q "$mounted" <<<"$CLAIM" \
    || { echo "FAIL: $mounted must be mounted by its workload" >&2; exit 1; }
done

if [ "${HELM_SMOKE_SKIP_CLUSTER:-0}" = "1" ]; then
  echo "==> cluster smoke skipped (HELM_SMOKE_SKIP_KIND_LOAD/CLUSTER)"
  echo "==> ci-helm-smoke OK (lint+render only)"
  exit 0
fi

for bin in kubectl kind; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "FAIL: '$bin' not on PATH (needed for the cluster smoke leg)" >&2
    exit 1
  fi
done

echo "==> kind cluster smoke"
kind get clusters | grep -qx "${KIND_CLUSTER:-kind}" || { echo "FAIL: kind cluster ${KIND_CLUSTER:-kind} not found" >&2; exit 1; }

if [ "${HELM_SMOKE_SKIP_KIND_LOAD:-0}" != "1" ]; then
  echo "==> loading images into kind"
  kind load docker-image --name "${KIND_CLUSTER:-kind}" "oxidean-api:${API_TAG}" "oxidean-web:${WEB_TAG}"
fi

echo "==> preload postgres image into kind"
docker pull postgres:18-alpine >/dev/null
kind load docker-image --name "${KIND_CLUSTER:-kind}" postgres:18-alpine

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

helm upgrade --install "$RELEASE" "$CHART" -n "$NS" \
  --set api.image.repository=oxidean-api \
  --set api.image.tag="$API_TAG" \
  --set api.image.pullPolicy=Never \
  --set web.image.repository=oxidean-web \
  --set web.image.tag="$WEB_TAG" \
  --set web.image.pullPolicy=Never \
  --set ingress.enabled=false \
  --set ssh.service.type=ClusterIP

dump_state() {
  echo "---- kubectl get all ----"
  kubectl -n "$NS" get all || true
  echo "---- pods ----"
  kubectl -n "$NS" get pods -o wide || true
  echo "---- describe api ----"
  kubectl -n "$NS" describe "deploy/${RELEASE}-oxidean-api" || true
  echo "---- logs api ----"
  kubectl -n "$NS" logs "deploy/${RELEASE}-oxidean-api" --all-containers --tail=120 || true
  echo "---- logs postgres ----"
  kubectl -n "$NS" logs "deploy/${RELEASE}-oxidean-postgres" --tail=60 || true
}
trap dump_state ERR

echo "==> waiting for rollouts"
kubectl -n "$NS" rollout status "deploy/${RELEASE}-oxidean-postgres" --timeout=300s
kubectl -n "$NS" rollout status "deploy/${RELEASE}-oxidean-api" --timeout=600s
kubectl -n "$NS" rollout status "deploy/${RELEASE}-oxidean-web" --timeout=300s

probe() {
  local svc="$1" port="$2" path="$3" extra="${4:-}"
  kubectl -n "$NS" port-forward "svc/${RELEASE}-${svc}" "${port}:${port}" >/dev/null 2>&1 &
  local pid=$!
  local ok=1
  local -a hdr=()
  [ -n "$extra" ] && hdr=(-H "$extra")
  for _ in $(seq 1 30); do
    if curl -fsS "${hdr[@]}" "http://127.0.0.1:${port}${path}" >/dev/null 2>&1; then
      ok=0
      break
    fi
    sleep 2
  done
  kill "$pid" >/dev/null 2>&1 || true
  wait "$pid" >/dev/null 2>&1 || true
  return "$ok"
}

echo "==> api /health"
probe "oxidean-api" 8080 "/health" || { echo "FAIL: api health probe" >&2; exit 1; }
echo "==> web /health (probe header)"
probe "oxidean-web" 3000 "/health" "Oxidean-Health-Probe: 1" || { echo "FAIL: web health probe" >&2; exit 1; }
echo "==> postgres readiness"
kubectl -n "$NS" exec "deploy/${RELEASE}-oxidean-postgres" -- \
  pg_isready -U oxidean >/dev/null || { echo "FAIL: postgres pg_isready" >&2; exit 1; }

echo "==> preload objectstore + init images into kind"
docker pull rustfs/rustfs:1.0.4 >/dev/null
docker pull amazon/aws-cli:2.39.3 >/dev/null
kind load docker-image --name "${KIND_CLUSTER:-kind}" rustfs/rustfs:1.0.4 amazon/aws-cli:2.39.3

echo "==> upgrade to storage.backend=s3 + bundled objectstore"
helm upgrade "$RELEASE" "$CHART" -n "$NS" \
  --reuse-values \
  --set storage.backend=s3 \
  --set objectstore.enabled=true

echo "==> waiting for objectstore + api rollouts"
kubectl -n "$NS" rollout status "deploy/${RELEASE}-oxidean-objectstore" --timeout=300s
kubectl -n "$NS" rollout status "deploy/${RELEASE}-oxidean-api" --timeout=600s

echo "==> bundled bucket exists in the store"
# The init Job is a helm hook with hook-succeeded cleanup — it is gone by the
# time helm returns, so assert the invariant it produces instead: the bucket
# is listable with the chart-minted credentials against the bundled service.
OS_AK="$(kubectl -n "$NS" get secret "${RELEASE}-oxidean-secrets" \
  -o jsonpath='{.data.OXIDEAN_S3_ACCESS_KEY_ID}' | base64 -d)"
OS_SK="$(kubectl -n "$NS" get secret "${RELEASE}-oxidean-secrets" \
  -o jsonpath='{.data.OXIDEAN_S3_SECRET_ACCESS_KEY}' | base64 -d)"
OS_OVR="$(printf '%s' \
  '{"spec":{"containers":[{"name":"ox-s3-check","image":"amazon/aws-cli:2.39.3",' \
  '"imagePullPolicy":"Never","command":["/bin/sh","-ec",' \
  '"aws --endpoint-url http://'"${RELEASE}"'-oxidean-objectstore:9000 s3 ls"],' \
  '"env":[{"name":"AWS_ACCESS_KEY_ID","value":"'"$OS_AK"'"},' \
  '{"name":"AWS_SECRET_ACCESS_KEY","value":"'"$OS_SK"'"},' \
  '{"name":"AWS_EC2_METADATA_DISABLED","value":"true"}]}]}}')"
# No --rm/-i attach — a fast-exiting pod's output can be lost before the
# attach stream opens; poll the phase and read logs instead.
kubectl -n "$NS" delete pod ox-s3-check --ignore-not-found >/dev/null
kubectl -n "$NS" run ox-s3-check --restart=Never \
  --image amazon/aws-cli:2.39.3 --overrides="$OS_OVR" >/dev/null
for _ in $(seq 1 60); do
  PHASE="$(kubectl -n "$NS" get pod ox-s3-check -o jsonpath='{.status.phase}' 2>/dev/null || true)"
  { [ "$PHASE" = "Succeeded" ] || [ "$PHASE" = "Failed" ]; } && break
  sleep 2
done
kubectl -n "$NS" logs ox-s3-check || true
{ [ "$PHASE" = "Succeeded" ] \
  && kubectl -n "$NS" logs ox-s3-check | grep -q "oxidean"; } \
  || { echo "FAIL: bundled bucket not listable" >&2; exit 1; }
kubectl -n "$NS" delete pod ox-s3-check >/dev/null

echo "==> api carries the bundled s3 env contract"
kubectl -n "$NS" exec "deploy/${RELEASE}-oxidean-api" -- printenv \
  | grep -q "^OXIDEAN_S3_ENDPOINT=http://ox-oxidean-objectstore:9000$" \
  || { echo "FAIL: api missing bundled OXIDEAN_S3_ENDPOINT" >&2; exit 1; }
kubectl -n "$NS" exec "deploy/${RELEASE}-oxidean-api" -- printenv \
  | grep -q "^OXIDEAN_STORAGE_BACKEND=s3$" \
  || { echo "FAIL: api missing OXIDEAN_STORAGE_BACKEND=s3" >&2; exit 1; }

echo "==> preload seaweedfs image into kind"
docker pull chrislusf/seaweedfs:4.51 >/dev/null
kind load docker-image --name "${KIND_CLUSTER:-kind}" chrislusf/seaweedfs:4.51

echo "==> upgrade to bundled SeaweedFS (swap providers)"
helm upgrade "$RELEASE" "$CHART" -n "$NS" \
  --reuse-values \
  --set objectstore.enabled=false \
  --set seaweedfs.enabled=true

echo "==> waiting for seaweedfs rollouts"
kubectl -n "$NS" rollout status "statefulset/${RELEASE}-seaweedfs-master" --timeout=300s
kubectl -n "$NS" rollout status "statefulset/${RELEASE}-seaweedfs-volume" --timeout=300s
kubectl -n "$NS" rollout status "statefulset/${RELEASE}-seaweedfs-filer" --timeout=300s
kubectl -n "$NS" rollout status "deploy/${RELEASE}-seaweedfs-s3" --timeout=300s

echo "==> seaweedfs bucket exists via the s3 gateway"
# Zero-config path: the subchart mints the admin pair into
# <release>-seaweedfs-s3-secret; the api and init Job keyref the same pair.
SW_AK="$(kubectl -n "$NS" get secret "${RELEASE}-seaweedfs-s3-secret" \
  -o jsonpath='{.data.admin_access_key_id}' | base64 -d)"
SW_SK="$(kubectl -n "$NS" get secret "${RELEASE}-seaweedfs-s3-secret" \
  -o jsonpath='{.data.admin_secret_access_key}' | base64 -d)"
SW_OVR="$(printf '%s' \
  '{"spec":{"containers":[{"name":"ox-s3-check","image":"amazon/aws-cli:2.39.3",' \
  '"imagePullPolicy":"Never","command":["/bin/sh","-ec",' \
  '"aws --endpoint-url http://'"${RELEASE}"'-seaweedfs-s3:8333 s3 ls"],' \
  '"env":[{"name":"AWS_ACCESS_KEY_ID","value":"'"$SW_AK"'"},' \
  '{"name":"AWS_SECRET_ACCESS_KEY","value":"'"$SW_SK"'"},' \
  '{"name":"AWS_EC2_METADATA_DISABLED","value":"true"}]}]}}')"
kubectl -n "$NS" delete pod ox-s3-check --ignore-not-found >/dev/null
kubectl -n "$NS" run ox-s3-check --restart=Never \
  --image amazon/aws-cli:2.39.3 --overrides="$SW_OVR" >/dev/null
for _ in $(seq 1 60); do
  PHASE="$(kubectl -n "$NS" get pod ox-s3-check -o jsonpath='{.status.phase}' 2>/dev/null || true)"
  { [ "$PHASE" = "Succeeded" ] || [ "$PHASE" = "Failed" ]; } && break
  sleep 2
done
kubectl -n "$NS" logs ox-s3-check || true
{ [ "$PHASE" = "Succeeded" ] \
  && kubectl -n "$NS" logs ox-s3-check | grep -q "oxidean"; } \
  || { echo "FAIL: seaweedfs bucket not listable via the s3 gateway" >&2; exit 1; }
kubectl -n "$NS" delete pod ox-s3-check >/dev/null

echo "==> api carries the seaweedfs s3 endpoint"
kubectl -n "$NS" exec "deploy/${RELEASE}-oxidean-api" -- printenv \
  | grep -q "^OXIDEAN_S3_ENDPOINT=http://${RELEASE}-seaweedfs-s3:8333$" \
  || { echo "FAIL: api missing seaweedfs OXIDEAN_S3_ENDPOINT" >&2; exit 1; }

trap - ERR
echo "==> ci-helm-smoke OK"
