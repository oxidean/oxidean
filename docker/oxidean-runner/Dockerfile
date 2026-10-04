# Official Oxidean Actions runner.
#
# Runs the native `oxidean-runner` binary against Oxidean's JSON runner
# protocol (/api/actions/*). Executes `actions/checkout` via git clone and
# `run:` steps on the host or in docker:// containers.
#
# Build context is the REPOSITORY ROOT (needs workspace Cargo.toml/lock +
# crates/), matching crates/oxidean-api/Dockerfile. cargo-chef splits deps from
# workspace builds so source-only changes reuse the cached deps layer. Cache
# mounts carry explicit `id=` for Railway's strict frontend.
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update \
  && apt-get install -y --no-install-recommends mold \
  && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=oxidean-runner-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=oxidean-runner-target,target=/app/target \
    cargo chef cook --release --recipe-path recipe.json -p oxidean-runner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,id=oxidean-runner-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=oxidean-runner-target,target=/app/target \
    cargo build --release -p oxidean-runner --bin oxidean-runner \
    && cp /app/target/release/oxidean-runner /usr/local/bin/

FROM debian:bookworm-slim
# git: actions/checkout clones over smart HTTP; ca-certificates: TLS origins.
# curl: fetch the docker CLI static binary below.
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl git \
  && rm -rf /var/lib/apt/lists/*
# docker CLI only (no daemon) — enables `label:docker://image` execution when
# /var/run/docker.sock is mounted. Host-mode labels need no Docker. Fetched as
# a static binary: Railway's builder rejects `COPY --from=<external image>`.
ARG DOCKER_CLI_VERSION=28.5.2
RUN curl -fsSL "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz" \
    | tar -xz --strip-components=1 -C /usr/local/bin docker/docker
COPY --from=builder /usr/local/bin/oxidean-runner /usr/local/bin/oxidean-runner

# Keep this file to the instruction subset Railway's Railpack validator
# accepts: no LABEL, no VOLUME (volumes attach via service config), no
# ENTRYPOINT (use CMD) — otherwise the build dies pre-flight with
# "The Dockerfile failed validation".

# Default labels: host execution so jobs run without a Docker daemon.
# Add `name:docker://image` labels for containerized steps (requires the
# Docker socket mount).
ENV OXIDEAN_RUNNER_LABELS="ubuntu-latest,self-hosted" \
    OXIDEAN_RUNNER_STATE=/data/runner.json \
    OXIDEAN_RUNNER_WORK_DIR=/data/work

CMD ["oxidean-runner"]
