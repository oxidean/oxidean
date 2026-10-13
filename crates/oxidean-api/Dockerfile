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
# CLI distribution (D-CLI-DIST): the api serves /cli/bin/ox-{target} — stage the
# companion `ox` build under the server's own target triple, then layer the
# full matrix from OXIDEAN_CLI_DIST_URL (ox-targets.txt + ox-{target} +
# .sha256). Default points at the rolling GitHub Release `ox-dist` so Cloud
# and networked self-host builds pick up macOS/Windows/etc. without a
# per-deploy ritual. Pass `--build-arg OXIDEAN_CLI_DIST_URL=` (empty) for
# airgapped / host-only staging. Fetch is fail-soft: retries then warn and
# continue with host-only so a missing release never bricks the image build.
ARG OXIDEAN_CLI_DIST_URL=https://github.com/oxidean/oxidean/releases/download/ox-dist
RUN cargo build --release -p oxidean-cli --bin ox \
  && mkdir -p /usr/local/share/oxidean-cli \
  && target=$(rustc -vV | awk '/^host:/ {print $2}') \
  && cp /app/target/release/ox "/usr/local/share/oxidean-cli/ox-$target" \
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
         echo "WARNING: could not fetch ox-targets.txt from $OXIDEAN_CLI_DIST_URL; continuing with host-only ox-$target"; \
       else \
         while read -r t || [ -n "$t" ]; do \
           [ -z "$t" ] && continue; \
           [ "$t" = "$target" ] && continue; \
           if curl -fsSL "$OXIDEAN_CLI_DIST_URL/ox-$t" -o "/usr/local/share/oxidean-cli/ox-$t" \
             && curl -fsSL "$OXIDEAN_CLI_DIST_URL/ox-$t.sha256" -o "/tmp/ox-$t.sha256" \
             && (cd /usr/local/share/oxidean-cli && sha256sum -c "/tmp/ox-$t.sha256"); then \
             :; \
           else \
             echo "WARNING: failed to stage ox-$t from $OXIDEAN_CLI_DIST_URL; skipping that target"; \
             rm -f "/usr/local/share/oxidean-cli/ox-$t"; \
           fi; \
         done < /tmp/ox-targets.txt; \
         ls -lh /usr/local/share/oxidean-cli; \
       fi; \
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
