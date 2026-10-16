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
# the lockfile), the bun workspace manifests the Dockerfile COPYs into its
# bun-deps/tools stages (package.json, bun.lock, turbo.json,
# apps/web/package.json, packages/api-client/package.json),
# docker/ci-base/**, and this script. Any change → new tag → ci-base.yml
# publishes it; nothing else reuses a stale tag.
#
# Resolution order per flavor: <flavor>-<hash> → <flavor>-latest. The -latest
# fallback covers lockfile-ahead PRs (a dep bump before the publisher lands):
# dep fingerprints are per-crate, so a stale-lockfile image still hits every
# unchanged dep — only the delta recompiles. Missing entirely → fail loudly
# with the bootstrap command.
#
# Env: CI_IMAGE_NS overrides the registry namespace (default ghcr.io/oxidean).
# CI_IMAGE_WAIT_SECONDS — resolve-all only: seconds to poll for THIS run's
# <flavor>-<hash> tag before resolving (a same-repo run's ci-base leg is
# still publishing it; probing ref() first would return a stale -latest
# without ever testing the tag that leg is publishing). At the deadline,
# resolve via ref() anyway — exact → -latest → fail. Fork PRs can't publish
# → keep 0 (single immediate resolve, same order). Default 0.
# CI_IMAGE_POLL_SECONDS — poll interval, default 30.
set -euo pipefail

FLAVORS=(dev release cov tools)
IMAGE_NAME="oxidean-ci"
IMAGE_NS="${CI_IMAGE_NS:-ghcr.io/oxidean}"
WAIT_SECONDS="${CI_IMAGE_WAIT_SECONDS:-0}"
POLL_SECONDS="${CI_IMAGE_POLL_SECONDS:-30}"

# A non-git cwd (or broken git) must die here, at script top where `set -e` is
# active — inside a `!`/`&&` command-substitution context errexit is suspended
# and an empty file list would otherwise hash to a phantom tag.
git rev-parse --is-inside-work-tree >/dev/null

content_hash() {
  # LC_ALL=C: collation of the filename list feeds the hash chain — without a
  # pinned locale the same tree hashes differently on en_US.UTF-8 vs C.UTF-8
  # (observed: dev workstation vs ubuntu-latest runner).
  git ls-files -z -- \
      Cargo.toml Cargo.lock 'crates/*/Cargo.toml' \
      package.json bun.lock turbo.json \
      apps/web/package.json packages/api-client/package.json \
      docker/ci-base scripts/ci-image-tag.sh \
    | LC_ALL=C sort -z \
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
  local hash exact fallback chosen
  # assignment on its own line — `local x=$(…)` would mask content_hash failure
  hash="$(content_hash)"
  exact="${img}:${flavor}-${hash}"; fallback="${img}:${flavor}-latest"
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

# When waiting is enabled (resolve-all with CI_IMAGE_WAIT_SECONDS > 0), poll
# for THIS run's <flavor>-<hash> tag until RESOLVE_DEADLINE — the bootstrap
# race: ci.yml and ci-base.yml start together on the same event and the
# publisher lands the content tag a few minutes later. Probing ref() here
# would return a stale -latest on the first pass and never test the tag this
# run's own ci-base leg is publishing. At the deadline (or with no wait
# budget — fork PRs can never publish), resolve once via ref: the loud
# ::warning::/::error:: guidance fires there exactly once.
ref_wait() { # $1 = flavor; RESOLVE_DEADLINE set by resolve-all
  local f="$1" img="${IMAGE_NS}/${IMAGE_NAME}"
  if [ "$WAIT_SECONDS" -gt 0 ]; then
    local hash exact
    # assignment on its own line — `local x=$(…)` would mask content_hash failure
    hash="$(content_hash)"
    exact="${img}:${f}-${hash}"
    while ! tag_exists "$exact"; do
      if [ "$(date +%s)" -ge "$RESOLVE_DEADLINE" ]; then
        echo "ci-image: ${exact#${img}:} still unpublished after ${WAIT_SECONDS}s — resolving anyway" >&2
        break
      fi
      echo "ci-image: ${exact#${img}:} not published yet — polling (${POLL_SECONDS}s interval)" >&2
      sleep "$POLL_SECONDS"
    done
  fi
  ref "$f"
}

case "${1:-}" in
  hash)        content_hash ;;
  tag)         h="$(content_hash)"; echo "${2:?flavor}-${h}" ;;
  ref)         ref "${2:?flavor}" ;;
  resolve-all) # `v=$(ref_wait …)` propagates failure under set -e (echo would mask it)
               # one shared budget across flavors — the ci-base matrix legs
               # publish in parallel, so once dev lands the rest are imminent.
               RESOLVE_DEADLINE=$(( $(date +%s) + WAIT_SECONDS ))
               for f in "${FLAVORS[@]}"; do v="$(ref_wait "$f")"; echo "${f}=${v}"; done ;;
  *)
    sed -n '2,20p' "$0" >&2
    exit 64
    ;;
esac
