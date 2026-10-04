# cargo-chef splits dependency compilation from workspace compilation into its
# own layer, so source-only changes reuse the cooked deps (local BuildKit layer
# cache; in CI the type=gha backend exports it — see docker-ci-images; Railway's
# layer cache covers deploys). mold links the release binaries.
# Note: no RUN --mount — Railway's builder only accepts cache mounts whose id
# carries a per-service `s/<service-id>-` cacheKey, which can't be expressed
# portably in a shared Dockerfile. The chef layer below is the cache mechanism.
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# protobuf-compiler: prost-build compiles proto/runner.proto (Phase 19).
# mold: fast linker for the release binaries.
RUN apt-get update \
  && apt-get install -y --no-install-recommends protobuf-compiler mold \
  && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p oxidean-api
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p oxidean-api --bin oxidean-api --bin oxidean-protection-hook \
  && cp /app/target/release/oxidean-api /app/target/release/oxidean-protection-hook /usr/local/bin/

FROM debian:bookworm-slim
# git: forge ops + D-33 boot gate (GIT-09); ca-certificates/curl: TLS + healthcheck;
# openssh-client: two-way mirror SSH remotes (ssh, ssh-keygen, ssh-keyscan)
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl git openssh-client gnupg \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/oxidean-api /usr/local/bin/oxidean-api
COPY --from=builder /usr/local/bin/oxidean-protection-hook /usr/local/bin/oxidean-protection-hook
ENV API_BIND=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=5s --timeout=5s --retries=12 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1
CMD ["oxidean-api"]
