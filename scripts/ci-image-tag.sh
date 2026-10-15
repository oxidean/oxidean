#!/usr/bin/env bash
# Content-keyed tags + digest resolution for the ci-base images
# (docker/ci-base/Dockerfile → ghcr.io/<owner>/oxidean-ci).
#
# Usage:
#   ci-image-tag.sh hash                 print the content hash (tag suffix)
#   ci-image-tag.sh tag <flavor>         print <flavor>-<hash>
#   ci-image-tag.sh ref <flavor>         print image ref pinned to @sha256
#   ci-image-tag.sh resolve-all          print dev=… release=… cov=… tools=…
#                                        lines (feed $GITHUB_OUTPUT)
#
# Hash inputs: Cargo.toml + Cargo.lock (dep tree + profile settings),
# crates/*/Cargo.toml (feature changes alter dep fingerprints without moving
# the lockfile), docker/ci-base/**, and this script. Any change → new tag →
# ci-base.yml publishes it; nothing else reuses a stale tag.
#
# Resolution order per flavor: <flavor>-<hash> → <flavor>-latest. The -latest
# fallback covers lockfile-ahead PRs (a dep bump before the publisher lands):
# dep fingerprints are per-crate, so a stale-lockfile image still hits every
# unchanged dep — only the delta recompiles. Missing entirely → fail loudly
# with the bootstrap command.
#
# Env: CI_IMAGE_NS overrides the registry namespace (default ghcr.io/oxidean).
set -euo pipefail

FLAVORS=(dev release cov tools)
IMAGE_NAME="oxidean-ci"
IMAGE_NS="${CI_IMAGE_NS:-ghcr.io/oxidean}"

content_hash() {
  git ls-files -z -- \
      Cargo.toml Cargo.lock 'crates/*/Cargo.toml' \
      docker/ci-base scripts/ci-image-tag.sh \
    | sort -z \
    | xargs -0 sha256sum \
    | sha256sum \
    | cut -c1-12
}

tag_exists() { # $1 = image:tag
  docker buildx imagetools inspect --raw "$1" >/dev/null 2>&1
}

# The canonical digest of a tag is sha256 over the manifest document itself —
# --raw emits exactly those bytes, for single-manifest and OCI index alike.
digest_of() { # $1 = image:tag
  docker buildx imagetools inspect --raw "$1" | sha256sum | cut -d' ' -f1
}

ref() { # $1 = flavor → echoes image:tag@sha256:digest
  local flavor="$1" img="${IMAGE_NS}/${IMAGE_NAME}"
  local exact="${img}:${flavor}-$(content_hash)" fallback="${img}:${flavor}-latest"
  local chosen
  if tag_exists "$exact"; then
    chosen="$exact"
  elif tag_exists "$fallback"; then
    echo "::warning::$(basename "$exact") not published — using ${flavor}-latest (stale inputs; unchanged deps still hit)" >&2
    chosen="$fallback"
  else
    echo "::error::no ${IMAGE_NAME}:${flavor} image — neither ${exact#${img}:} nor ${flavor}-latest exists on GHCR. Run the ci-base workflow (workflow_dispatch) to bootstrap." >&2
    return 1
  fi
  echo "${chosen}@sha256:$(digest_of "$chosen")"
}

case "${1:-}" in
  hash)        content_hash ;;
  tag)         echo "${2:?flavor}-$(content_hash)" ;;
  ref)         ref "${2:?flavor}" ;;
  resolve-all) # `v=$(ref …)` propagates ref's failure under set -e (echo would mask it)
               for f in "${FLAVORS[@]}"; do v="$(ref "$f")"; echo "${f}=${v}"; done ;;
  *)
    sed -n '2,20p' "$0" >&2
    exit 64
    ;;
esac
