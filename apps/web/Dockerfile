# Web image: Astro static build → Rust (oxidean-web) middleware runtime.
# No JS runtime in the serving path — the API-facing proxy, SPA fallback,
# theme/meta injection, and auth gate all live in the Rust binary.
FROM oven/bun:1.4-debian AS deps
WORKDIR /app
COPY package.json bun.lock turbo.json ./
COPY apps/web/package.json ./apps/web/
COPY packages/api-client/package.json ./packages/api-client/
RUN bun install --frozen-lockfile

FROM deps AS build
WORKDIR /app
# Railway / CI may inject the git SHA so the service worker CACHE_NAME changes
# every deploy (clients pick up a new worker instead of a stale shell cache).
ARG RAILWAY_GIT_COMMIT_SHA
ARG SOURCE_COMMIT
ARG GITHUB_SHA
ENV RAILWAY_GIT_COMMIT_SHA=$RAILWAY_GIT_COMMIT_SHA \
    SOURCE_COMMIT=$SOURCE_COMMIT \
    GITHUB_SHA=$GITHUB_SHA
COPY apps/web ./apps/web
COPY packages/api-client ./packages/api-client
RUN bun run --filter @oxidean/web build

# Rust stage — cargo-chef splits dep compilation from workspace compilation
# (same mechanism as crates/oxidean-api/Dockerfile).
FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS rust-builder
RUN apt-get update \
  && apt-get install -y --no-install-recommends mold \
  && rm -rf /var/lib/apt/lists/*
ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p oxidean-web
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p oxidean-web --bin oxidean-web \
  && cp /app/target/release/oxidean-web /usr/local/bin/

FROM debian:bookworm-slim AS runner
WORKDIR /app
RUN apt-get update \
  && apt-get install -y --no-install-recommends curl ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY --from=rust-builder /usr/local/bin/oxidean-web /usr/local/bin/oxidean-web
COPY --from=build /app/apps/web/dist ./dist
ENV PORT=3000 \
    OXIDEAN_WEB_DIST=/app/dist
EXPOSE 3000
# Railway injects PORT at runtime; the binary honors ${PORT:-3000}.
# Platform healthcheck is GET / (no custom headers). Container HEALTHCHECK
# still uses the header-gated /health probe for local Compose.
HEALTHCHECK --interval=5s --timeout=5s --retries=12 --start-period=10s \
  CMD curl -fsS -H 'Oxidean-Health-Probe: 1' http://127.0.0.1:${PORT:-3000}/health || exit 1
CMD ["oxidean-web"]
