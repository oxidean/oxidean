#!/usr/bin/env bash
# Load prebuilt Compose api+web images from a docker-save tarball (CI same-run reuse).
# Expects OXIDEAN_CI_IMAGES_TGZ (path to .tgz) or a single positional argument.
set -euo pipefail

TGZ="${1:-${OXIDEAN_CI_IMAGES_TGZ:-}}"
if [[ -z "$TGZ" ]]; then
  echo "FAIL: pass image tarball path or set OXIDEAN_CI_IMAGES_TGZ" >&2
  exit 1
fi
if [[ ! -f "$TGZ" ]]; then
  echo "FAIL: image tarball not found: $TGZ" >&2
  exit 1
fi

# Fail fast on truncated/corrupt artifact downloads — a partial tarball
# otherwise dies cryptically inside docker load ("unexpected EOF").
if ! gzip -t "$TGZ"; then
  echo "FAIL: image tarball is truncated or corrupt: $TGZ" >&2
  echo "     re-run the job — this is an artifact-download flake." >&2
  exit 1
fi

echo "==> docker load from $TGZ"
gzip -dc "$TGZ" | docker load
echo "==> verifying expected tags"
missing=0
for tag in oxidean-api:ci oxidean-web:ci; do
  if ! docker image inspect "$tag" >/dev/null 2>&1; then
    echo "FAIL: missing image $tag after load" >&2
    missing=1
  else
    echo "  ok $tag"
  fi
done
if [[ "$missing" -ne 0 ]]; then
  docker image ls >&2 || true
  exit 1
fi
