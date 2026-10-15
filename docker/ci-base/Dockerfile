# CI base images — self-owned, content-tagged, run-isolated.
#
# Published to ghcr.io/<owner>/oxidean-ci by .github/workflows/ci-base.yml as
# <flavor>-<hash> (immutable content tag, hash via scripts/ci-image-tag.sh)
# plus <flavor>-latest (moving fallback). ci.yml resolves every pull to an
# @sha256 digest so all jobs in a run see byte-identical bits.
#
# Why this exists: the CI lanes used to lean on Swatinem/rust-cache (a mutable
# cross-run cache) plus ~10 per-job tool installs. Here the toolchain, helper
# binaries, and — for compile lanes — cargo-chef-cooked third-party dep
# artifacts are baked once per dependency/tooling change. Cargo fingerprints
# dep artifacts on (package, features, rustc, profile, rustflags) — all
# path-independent — so a job pointing CARGO_TARGET_DIR (or
# CARGO_LLVM_COV_TARGET_DIR for coverage) at the cooked dir reuses every dep
# and compiles only the fresh workspace checkout. The image is a shared
# read-only input; nothing a run does can contaminate another run.
#
# Flavors (build targets):
#   tools   — bun + Playwright chromium + docker CLI for web/exec-only lanes
#   dev     — rust + mold/protoc + cargo tools + /ci/target-dev (build+clippy)
#   release — rust + mold/protoc + /ci/target-release
#   cov     — rust + llvm-cov + /ci/target-cov (instrumented)
#
# rustc is pinned exactly (rust:1.99.0-bookworm) — a toolchain bump is a
# one-line PR against this ARG, and since this file is a hash input the bump
# mints new content tags. The weekly ci-base.yml refresh still rebuilds so the
# Debian base's patch drift and apt package pins stay current.

ARG RUST_IMAGE=rust:1.99.0-bookworm
ARG BUN_IMAGE=oven/bun:1.4.0-debian
ARG DEBIAN_IMAGE=debian:bookworm-slim

# Pinned tool versions — bump deliberately; the content tag covers this file.
ARG CARGO_NEXTEST_VERSION=0.9.146
ARG CARGO_LLVM_COV_VERSION=0.9.1
ARG CARGO_MACHETE_VERSION=0.9.2
ARG CARGO_CHEF_VERSION=0.1.78
ARG DOCKER_CLI_VERSION=29.9.0
ARG DOCKER_COMPOSE_VERSION=2.40.3
ARG DOCKER_BUILDX_VERSION=0.30.1

# ── cargo tools ─────────────────────────────────────────────────────────────
# cargo install --locked: deterministic, once per image build (never per run).
FROM ${RUST_IMAGE} AS cargo-tools
ARG CARGO_NEXTEST_VERSION CARGO_LLVM_COV_VERSION CARGO_MACHETE_VERSION CARGO_CHEF_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install --locked \
      cargo-nextest@${CARGO_NEXTEST_VERSION} \
      cargo-llvm-cov@${CARGO_LLVM_COV_VERSION} \
      cargo-machete@${CARGO_MACHETE_VERSION} \
      cargo-chef@${CARGO_CHEF_VERSION}

# ── dep recipe ───────────────────────────────────────────────────────────────
# cargo-chef splits third-party deps from workspace crates: cooking the recipe
# produces dep-only artifacts, so workspace crates still compile fresh per run.
FROM ${RUST_IMAGE} AS planner
COPY --from=cargo-tools /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
WORKDIR /ws
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path /recipe.json

# ── bun workspace cache ──────────────────────────────────────────────────────
# The bun global install cache is the reuse unit: flavors copy /opt/bun-cache
# and CI's `bun install --frozen-lockfile` links from it in seconds.
FROM ${BUN_IMAGE} AS bun-deps
ENV BUN_INSTALL_CACHE_DIR=/opt/bun-cache
WORKDIR /ws
COPY package.json bun.lock turbo.json ./
COPY apps/web/package.json ./apps/web/package.json
COPY packages/api-client/package.json ./packages/api-client/package.json
RUN bun install --frozen-lockfile

# ── shared rust lane base ────────────────────────────────────────────────────
# ENV pins the flags dep fingerprints were cooked with — changing them here
# changes this file's hash, which changes the image tag. Keep in sync with
# what ci.yml expects (jobs must not override RUSTFLAGS/CARGO_INCREMENTAL).
FROM ${RUST_IMAGE} AS rust-ci
RUN apt-get update \
    # cmake/pkg-config: -sys dep build scripts (aws-lc-sys vendored C) need them
    # at bake time; build-script outputs are fingerprinted so runtime doesn't.
    && apt-get install -y --no-install-recommends \
      mold protobuf-compiler git curl ca-certificates jq time xz-utils \
      cmake pkg-config \
    && rm -rf /var/lib/apt/lists/*
ENV CARGO_INCREMENTAL=0 \
    RUSTFLAGS="-C link-arg=-fuse-ld=mold" \
    CARGO_TERM_COLOR=always
# bun for jobs that mix rust + web tooling (knip, coverage-web). The node→bun
# symlink mirrors the oven/bun image's own fallback: Playwright's browser
# downloader spawns `node` out-of-process, and bun's node-compat mode serves
# it (nothing in these lanes needs real V8 — everything else runs via bun).
COPY --from=bun-deps /usr/local/bin/bun /usr/local/bin/bunx /usr/local/bin/
RUN ln -s /usr/local/bin/bun /usr/local/bin/node
COPY --from=bun-deps /opt/bun-cache /opt/bun-cache
ENV BUN_INSTALL_CACHE_DIR=/opt/bun-cache

# ── dev lane: rust-build + dead-code ─────────────────────────────────────────
FROM rust-ci AS dev
# rust image ships rustup's minimal profile — clippy is needed both by
# `chef cook --clippy` (dep check-artifacts) and the job's clippy gate.
RUN rustup component add clippy
COPY --from=cargo-tools /usr/local/cargo/bin/ /usr/local/cargo/bin/
WORKDIR /ws
COPY --from=planner /recipe.json recipe.json
# Build artifacts (libs/bins/tests incl. dev-deps) + clippy check artifacts
# for the whole third-party graph. Workspace members are not in the recipe —
# they always recompile from the fresh checkout.
# Four artifact families, one target dir: plain cook covers
# `cargo build --workspace --bins`'s feature set and `--all-targets` covers
# the nextest archive — they differ because all-targets unifies dev-deps into
# shared deps (axum/reqwest/… → a different fingerprint). Same split in
# check-mode: `cargo clippy --all-targets` vs `cargo doc --no-deps` (which
# resolves deps like a plain build), so both check passes are cooked too.
RUN cargo chef cook --workspace --recipe-path recipe.json \
      --target-dir /ci/target-dev \
    && cargo chef cook --workspace --all-targets --recipe-path recipe.json \
      --target-dir /ci/target-dev \
    && cargo chef cook --clippy --workspace --all-targets \
      --recipe-path recipe.json --target-dir /ci/target-dev \
    && cargo chef cook --check --workspace --all-targets \
      --recipe-path recipe.json --target-dir /ci/target-dev \
    && cargo chef cook --check --workspace \
      --recipe-path recipe.json --target-dir /ci/target-dev \
    && rm -rf /ws /ci/target-dev/.locked "$CARGO_HOME/registry/cache"

# ── release lane: build-metrics ──────────────────────────────────────────────
FROM rust-ci AS release
COPY --from=cargo-tools /usr/local/cargo/bin/cargo-chef /usr/local/cargo/bin/cargo-chef
WORKDIR /ws
COPY --from=planner /recipe.json recipe.json
# Package-scoped: the metrics job builds `-p oxidean-api`, whose dep feature
# unification differs from --workspace — a workspace-scoped bake would be
# missed by ~60 crates. Cook what the lane resolves.
RUN cargo chef cook --release --package oxidean-api --recipe-path recipe.json \
      --target-dir /ci/target-release \
    && rm -rf /ws "$CARGO_HOME/registry/cache"

# ── coverage lane: coverage-weighted ─────────────────────────────────────────
FROM rust-ci AS cov
RUN rustup component add llvm-tools-preview
COPY --from=cargo-tools /usr/local/cargo/bin/ /usr/local/cargo/bin/
WORKDIR /ws
COPY --from=planner /recipe.json recipe.json
# Instrumented dep artifacts, compile-only: llvm-cov instruments via a rustc
# WRAPPER applied only to workspace crates, so dep artifacts are plain
# (mold-flagged) builds — the show-env vars are exported anyway so the wrapper
# path participates in the fingerprint exactly as it does at run time. The
# cov target dir is exported before show-env so it resolves to /ci/target-cov
# (the path the CI job sets via CARGO_LLVM_COV_TARGET_DIR) rather than
# defaulting to <ws>/target. No test execution here (suite needs DBs).
RUN export CARGO_LLVM_COV_TARGET_DIR=/ci/target-cov \
    && eval "$(cargo llvm-cov show-env --export-prefix)" \
    && CARGO_TARGET_DIR=/ci/target-cov \
       cargo chef cook --workspace --all-targets --recipe-path recipe.json \
    && rm -rf /ws "$CARGO_HOME/registry/cache"

# ── tools lane: web / exec-only jobs ─────────────────────────────────────────
# No rustc: jobs on this flavor never compile — they run prebuilt bin
# artifacts (OXIDEAN_PREBUILT_BINS=1) or bun-only lanes.
FROM ${DEBIAN_IMAGE} AS tools
ARG DOCKER_CLI_VERSION DOCKER_COMPOSE_VERSION DOCKER_BUILDX_VERSION
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      git curl ca-certificates jq tar gzip xz-utils \
    && rm -rf /var/lib/apt/lists/*
# docker CLI only (no daemon) — jobs mount /var/run/docker.sock; compose v2
# plugin for `docker compose` + buildx plugin (`docker compose up --build`
# shells to buildx — dev-auth stubs in e2e-stack).
RUN curl -fsSL "https://download.docker.com/linux/static/stable/x86_64/docker-${DOCKER_CLI_VERSION}.tgz" \
      | tar -xz --strip-components=1 -C /usr/local/bin docker/docker \
    && mkdir -p /usr/local/lib/docker/cli-plugins \
    && curl -fsSL "https://github.com/docker/compose/releases/download/v${DOCKER_COMPOSE_VERSION}/docker-compose-linux-x86_64" \
      -o /usr/local/lib/docker/cli-plugins/docker-compose \
    && curl -fsSL "https://github.com/docker/buildx/releases/download/v${DOCKER_BUILDX_VERSION}/buildx-v${DOCKER_BUILDX_VERSION}.linux-amd64" \
      -o /usr/local/lib/docker/cli-plugins/docker-buildx \
    && chmod +x /usr/local/lib/docker/cli-plugins/docker-compose \
      /usr/local/lib/docker/cli-plugins/docker-buildx \
    && docker --version \
    && docker compose version \
    && docker buildx version
COPY --from=bun-deps /usr/local/bin/bun /usr/local/bin/bunx /usr/local/bin/
# node→bun symlink (same as oven/bun's fallback) — Playwright's installer
# spawns `node` for its out-of-process browser download.
RUN ln -s /usr/local/bin/bun /usr/local/bin/node
ENV BUN_INSTALL_CACHE_DIR=/opt/bun-cache \
    PLAYWRIGHT_BROWSERS_PATH=/opt/pw
COPY --from=bun-deps /opt/bun-cache /opt/bun-cache
# Playwright chromium at the repo's locked version + its apt deps — CI's
# `bunx playwright install chromium` becomes a verify-only no-op.
WORKDIR /ws
COPY package.json bun.lock turbo.json ./
COPY apps/web/package.json ./apps/web/package.json
COPY packages/api-client/package.json ./packages/api-client/package.json
RUN bun install --frozen-lockfile \
    && cd apps/web && bunx playwright install --with-deps chromium \
    && rm -rf /ws/node_modules /ws/apps/*/node_modules /ws/packages/*/node_modules
WORKDIR /
