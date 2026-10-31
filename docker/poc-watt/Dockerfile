# PoC image for issue #209 — one Platformatic Watt process serving web + edge.
#
# Layout: Bun builds apps/web's static dist exactly like apps/web/Dockerfile;
# the runtime stage is Node 22 running wattpm with two worker applications:
#   gateway — Traefik-role edge (docker-compose.yml label contract) + proxy
#   web     — the crates/oxidean-web middleware port + dist serving
#
# Build context is the repo root:
#   docker build -f docker/poc-watt/Dockerfile -t oxidean-poc-watt:ci .

# ---- Static dist build (same recipe as apps/web/Dockerfile) ----
FROM oven/bun:1.4-debian AS deps
WORKDIR /app
COPY package.json bun.lock turbo.json ./
COPY apps/web/package.json ./apps/web/
COPY packages/api-client/package.json ./packages/api-client/
RUN bun install --frozen-lockfile

FROM deps AS build
WORKDIR /app
ARG RAILWAY_GIT_COMMIT_SHA
ARG SOURCE_COMMIT
ARG GITHUB_SHA
ENV RAILWAY_GIT_COMMIT_SHA=$RAILWAY_GIT_COMMIT_SHA \
    SOURCE_COMMIT=$SOURCE_COMMIT \
    GITHUB_SHA=$GITHUB_SHA
COPY apps/web ./apps/web
COPY packages/api-client ./packages/api-client
RUN bun run --filter @oxidean/web build

# ---- Watt runtime — Node 22 pinned (repo is Bun-first; this lane adds Node) ----
FROM node:22-bookworm-slim AS runner
WORKDIR /app
ENV NODE_ENV=production

# Install the watt workspace deps first for layer caching.
COPY docker/poc-watt/package.json docker/poc-watt/package-lock.json ./
COPY docker/poc-watt/services/gateway/package.json ./services/gateway/
COPY docker/poc-watt/services/web/package.json ./services/web/
RUN npm ci --omit=dev

# App code + runtime config, then the built SPA dist.
COPY docker/poc-watt/watt.json ./watt.json
COPY docker/poc-watt/services ./services
COPY docker/poc-watt/shared ./shared
COPY --from=build /app/apps/web/dist ./dist

ENV PORT=80 \
    PLT_SERVER_HOSTNAME=0.0.0.0 \
    PLT_SERVER_LOGGER_LEVEL=info \
    PLT_MANAGEMENT_API=false \
    OXIDEAN_API_ORIGIN=http://api:8080 \
    OXIDEAN_WEB_ORIGIN=http://web.plt.local \
    OXIDEAN_WEB_DIST=/app/dist
EXPOSE 80
CMD ["npx", "wattpm", "start"]
