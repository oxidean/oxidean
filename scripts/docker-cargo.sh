#!/usr/bin/env bash
# Run cargo/tooling inside the ci-base dev image — nothing compiles on the host.
#
# Resolves the same image CI uses (scripts/ci-image-tag.sh → ghcr.io/oxidean/
# oxidean-ci:dev-<hash>@sha256), bind-mounts the repo at /src, and points
# CARGO_TARGET_DIR + the cargo registry at named docker volumes so build
# artifacts live in docker storage, not the working tree.
#
# Usage:
#   scripts/docker-cargo.sh cargo check --workspace
#   scripts/docker-cargo.sh cargo nextest run --workspace
#   scripts/docker-cargo.sh /usr/bin/time -v cargo build -p oxidean-api -j1
#   scripts/docker-cargo.sh bash                 # interactive shell in the image
#
# Env overrides: DC_IMAGE (skip resolution), DC_TARGET_VOL, DC_REGISTRY_VOL.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

IMAGE="${DC_IMAGE:-}"
if [ -z "$IMAGE" ]; then
  IMAGE="$(cd "$ROOT" && CI_IMAGE_NS="${CI_IMAGE_NS:-ghcr.io/oxidean}" ./scripts/ci-image-tag.sh ref dev 2>/dev/null)" \
    || IMAGE="${CI_IMAGE_NS:-ghcr.io/oxidean}/oxidean-ci:dev-latest"
fi

TARGET_VOL="${DC_TARGET_VOL:-oxidean-cargo-target}"
REGISTRY_VOL="${DC_REGISTRY_VOL:-oxidean-cargo-registry}"

docker volume create "$TARGET_VOL" >/dev/null
docker volume create "$REGISTRY_VOL" >/dev/null

# Seed the target volume with the image's cargo-chef-cooked dep artifacts once
# (an empty named volume would shadow /ci/target-dev otherwise).
if [ "$(docker run --rm -v "$TARGET_VOL":/dst "$IMAGE" sh -c 'ls -A /dst | wc -l')" = "0" ]; then
  echo "docker-cargo: seeding $TARGET_VOL from image /ci/target-dev" >&2
  docker run --rm -v "$TARGET_VOL":/dst "$IMAGE" sh -c 'cp -a /ci/target-dev/. /dst/ 2>/dev/null || true'
fi

exec docker run --rm ${DC_TTY:+-it} \
  -v "$ROOT:/src" -w /src \
  -v "$TARGET_VOL":/ci/target-dev \
  -v "$REGISTRY_VOL":/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/ci/target-dev \
  -e CARGO_TERM_COLOR=always \
  -e CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}" \
  ${DC_ENV:-} \
  "$IMAGE" "$@"
