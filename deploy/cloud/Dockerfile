# Oxidean Cloud HTTP gateway (D-CLOUD-03).
# Pins the Docker Official Image approved for Phase 22.
# Build from repo root: docker build -f deploy/cloud/Dockerfile .
FROM caddy:2.11.7-alpine
COPY deploy/cloud/Caddyfile /etc/caddy/Caddyfile
EXPOSE 8080
# Railway injects PORT; Caddyfile defaults to 8080 when unset.
