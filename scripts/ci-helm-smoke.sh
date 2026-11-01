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

for bin in helm kubectl kind; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "FAIL: '$bin' not on PATH" >&2
    exit 1
  fi
done

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

if [ "${HELM_SMOKE_SKIP_CLUSTER:-0}" = "1" ]; then
  echo "==> cluster smoke skipped (HELM_SMOKE_SKIP_KIND_LOAD/CLUSTER)"
  echo "==> ci-helm-smoke OK (lint+render only)"
  exit 0
fi

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

trap - ERR
echo "==> ci-helm-smoke OK"
