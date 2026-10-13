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
# mold: fast linker for the release binaries. curl: OXIDEAN_CLI_DIST_URL fetch.
RUN apt-get update \
  && apt-get install -y --no-install-recommends protobuf-compiler mold curl ca-certificates \
  && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p oxidean-api -p oxidean-cli
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p oxidean-api --bin oxidean-api --bin oxidean-protection-hook \
  && cp /app/target/release/oxidean-api /app/target/release/oxidean-protection-hook /usr/local/bin/
# CLI distribution (D-CLI-DIST): stage a cargo-built host `ox` first (host-only
# / empty-URL path). When OXIDEAN_CLI_DIST_URL is non-empty, fetch every triple
# listed in ox-targets.txt — including the host — so staged digests match the
# feed (CI's image-matrix job sha256sum-checks every target against ox-dist).
# Default URL is this product's rolling ox-dist release so Cloud / Railway PR
# envs get darwin/windows without a manual IaC apply. Compose and CI pass an
# empty build-arg for host-only. Self-hosts that must not pull upstream should
# set OXIDEAN_CLI_DIST_URL=. When the URL is set, fetch is fail-closed
# (retries, then fail the build). Split from cargo build so
# OXIDEAN_CLI_DIST_BUST can invalidate only the download layer.
ARG OXIDEAN_CLI_DIST_URL=https://github.com/oxidean/oxidean/releases/download/ox-dist
RUN cargo build --release -p oxidean-cli --bin ox \
  && mkdir -p /usr/local/share/oxidean-cli \
  && target=$(rustc -vV | awk '/^host:/ {print $2}') \
  && cp /app/target/release/ox "/usr/local/share/oxidean-cli/ox-$target"
# Bump OXIDEAN_CLI_DIST_BUST (e.g. git SHA) to re-fetch ox-dist without
# re-running cargo build. Staging redeploy sets this before deploy.
ARG OXIDEAN_CLI_DIST_BUST=
RUN echo "ox-dist bust=${OXIDEAN_CLI_DIST_BUST:-none}" \
  && if [ -n "$OXIDEAN_CLI_DIST_URL" ]; then \
       echo "staging full ox matrix from $OXIDEAN_CLI_DIST_URL"; \
       ok=0; \
       i=1; \
       while [ "$i" -le 20 ]; do \
         if curl -fsSL "$OXIDEAN_CLI_DIST_URL/ox-targets.txt" -o /tmp/ox-targets.txt; then \
           ok=1; \
           break; \
         fi; \
         echo "ox-targets.txt not ready (attempt $i/20); sleeping 15s"; \
         i=$((i + 1)); \
         sleep 15; \
       done; \
       if [ "$ok" != 1 ]; then \
         echo "ERROR: could not fetch ox-targets.txt from $OXIDEAN_CLI_DIST_URL" >&2; \
         exit 1; \
       fi; \
       while read -r t || [ -n "$t" ]; do \
         [ -z "$t" ] && continue; \
         if curl -fsSL "$OXIDEAN_CLI_DIST_URL/ox-$t" -o "/usr/local/share/oxidean-cli/ox-$t" \
           && curl -fsSL "$OXIDEAN_CLI_DIST_URL/ox-$t.sha256" -o "/tmp/ox-$t.sha256" \
           && (cd /usr/local/share/oxidean-cli && sha256sum -c "/tmp/ox-$t.sha256"); then \
           :; \
         else \
           echo "ERROR: failed to stage ox-$t from $OXIDEAN_CLI_DIST_URL" >&2; \
           rm -f "/usr/local/share/oxidean-cli/ox-$t"; \
           exit 1; \
         fi; \
       done < /tmp/ox-targets.txt; \
       if ! curl -fsSL "$OXIDEAN_CLI_DIST_URL/SOURCE_COMMIT" \
         -o /usr/local/share/oxidean-cli/SOURCE_COMMIT; then \
         echo "ERROR: SOURCE_COMMIT missing at $OXIDEAN_CLI_DIST_URL" >&2; \
         exit 1; \
       fi; \
       ls -lh /usr/local/share/oxidean-cli; \
     fi

FROM debian:bookworm-slim
# git: forge ops + D-33 boot gate (GIT-09); ca-certificates/curl: TLS + healthcheck;
# openssh-client: two-way mirror SSH remotes (ssh, ssh-keygen, ssh-keyscan)
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl git openssh-client gnupg \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/oxidean-api /usr/local/bin/oxidean-api
COPY --from=builder /usr/local/bin/oxidean-protection-hook /usr/local/bin/oxidean-protection-hook
COPY --from=builder /usr/local/share/oxidean-cli /usr/local/share/oxidean-cli
ENV API_BIND=0.0.0.0:8080
EXPOSE 8080
HEALTHCHECK --interval=5s --timeout=5s --retries=12 \
  CMD curl -fsS http://127.0.0.1:8080/health || exit 1
CMD ["oxidean-api"]
