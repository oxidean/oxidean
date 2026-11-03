#!/usr/bin/env bash
# Reclaim hosted-runner disk before GB-scale Rust work.
#
# ubuntu-latest VMs have been observed starting jobs at ~92% disk used
# (~6 GB free of 72 GB), which is below what rust-build's workspace build,
# coverage-weighted's instrumented llvm-cov build, or a release build writes
# to /ci/target-* — the failure mode is `mold: failed to write to an output
# file` mid-link, or the runner worker dying writing its own diag log.
#
# Runs in both CI contexts:
#   host job      — sudo rm on the runner fs (same dir list e2e-stack /
#                   api-rust / compose-smoke / helm already inline).
#   container job — host paths aren't mounted, but /__t is the hosted
#                   toolcache bind-mount (rw) and /var/run/docker.sock is
#                   injected, so the script drives a sibling container for
#                   host-path deletes and prunes the hosted image's
#                   pre-pulled docker images. The job container's own image
#                   is in use and survives the prune.
#
# Every step is best-effort: a reclaim miss must never fail the build.

set -uo pipefail

# Same list as the inline free-disk steps elsewhere in this workflow —
# hosted-image toolchains these jobs never touch.
FAT_DIRS=(/usr/share/dotnet /usr/local/lib/android /opt/ghc /usr/local/share/boost /usr/share/swift)
# Pinned to docker/ci-base/Dockerfile's ARG DOCKER_CLI_VERSION — the rust
# lanes (dev/cov/release images) ship no docker CLI, so it is fetched on
# demand when the socket is present but the binary is not. It lands OFF
# PATH on purpose: tests like the workflow conformance suite gate docker
# cases on `command -v docker`, and a visible CLI changes what they run.
DOCKER_CLI_VERSION="${CI_DOCKER_CLI_VERSION:-29.9.2}"
DOCKER_BIN_DIR=/tmp/ci-free-disk-bin

echo "==> disk before"
df -h / || true

# Hosted toolcache — multi-GB of pinned SDK versions. Bind-mounted rw at
# /__t inside container jobs; on the host it lives at $AGENT_TOOLSDIRECTORY.
rm -rf /__t/* "${AGENT_TOOLSDIRECTORY:-/opt/hostedtoolcache}"/* 2>/dev/null || true

if [ -S /var/run/docker.sock ]; then
  DOCKER_BIN="$(command -v docker || true)"
  if [ -z "$DOCKER_BIN" ]; then
    mkdir -p "$DOCKER_BIN_DIR"
    if curl -fsSL "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz" \
      | tar -xz --strip-components=1 -C "$DOCKER_BIN_DIR" docker/docker; then
      DOCKER_BIN="$DOCKER_BIN_DIR/docker"
    fi
  fi
  if [ -n "$DOCKER_BIN" ] && "$DOCKER_BIN" version >/dev/null 2>&1; then
    "$DOCKER_BIN" system prune -af >/dev/null 2>&1 || true
    HOST_DIRS=("${FAT_DIRS[@]/#//host}")
    "$DOCKER_BIN" run --rm -v /:/host alpine rm -rf "${HOST_DIRS[@]}" || true
  fi
elif [ "$(id -u)" = 0 ]; then
  rm -rf "${FAT_DIRS[@]}" || true
elif command -v sudo >/dev/null 2>&1; then
  sudo rm -rf "${FAT_DIRS[@]}" || true
fi

echo "==> disk after"
df -h / || true
