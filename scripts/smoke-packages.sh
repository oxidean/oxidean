#!/usr/bin/env bash
# Compose Packages Registry smoke (PKG-01..03 / D-PKG-01).
# Asserts Traefik routes /v2, /npm, /generic to the API (not SPA HTML).
#
# Prerequisites:
#   - Docker Compose stack up (`make up`) with Traefik on :80
#   - Registry PathPrefix routers (api-packages) from Phase 20 plan 02
#
# Env knobs:
#   OXIDEAN_SMOKE_URL   default http://localhost (must match Traefik Host)
#
# Operator hosts without Docker/stack: exits 0 with a skip message.
# CI=true or SMOKE_REQUIRE_STACK=1 fails closed (T-11.1-40 / D-QH-04).
# Never prints PATs or other secrets.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
SMOKE_NAME="smoke-packages"

BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"

smoke_require_docker
# Fast skip when Compose API isn't up (avoid 2-minute health wait).
if ! smoke_compose_api_running; then
  smoke_require_or_skip "Compose API not running; skipping smoke-packages (run make up to exercise Traefik routing)"
fi

echo "==> wait for ${BASE_URL}/health"
ok=0
for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null "${BASE_URL}/health" 2>/dev/null; then
    ok=1
    break
  fi
  sleep 2
done
if [[ "$ok" -ne 1 ]]; then
  echo "health check failed at ${BASE_URL}/health — is the stack up? (make up)" >&2
  exit 1
fi

check_not_spa() {
  local path="$1"
  local url="${BASE_URL}${path}"
  local tmp_headers tmp_body http_code ct
  tmp_headers="$(mktemp)"
  tmp_body="$(mktemp)"
  http_code="$(
    curl -sS -D "$tmp_headers" -o "$tmp_body" -w "%{http_code}" \
      -H "Host: localhost" \
      -I "$url" 2>/dev/null || \
    curl -sS -D "$tmp_headers" -o "$tmp_body" -w "%{http_code}" \
      -H "Host: localhost" \
      "$url" || true
  )"
  ct="$(grep -i '^content-type:' "$tmp_headers" | head -1 | tr -d '\r' || true)"
  rm -f "$tmp_headers" "$tmp_body"

  echo "==> ${path} → HTTP ${http_code} content-type=${ct:-unknown}"
  if echo "$ct" | grep -qi 'text/html'; then
    echo "FAIL: ${path} returned text/html (SPA stole registry path). Expected API via Traefik PathPrefix /v2|/npm|/generic (api-packages priority ≥110)." >&2
    exit 1
  fi
}

# Expected Traefik labels (plan 20-02): PathPrefix(`/v2`)|PathPrefix(`/npm`)|PathPrefix(`/generic`) → api
check_not_spa "/v2/"
check_not_spa "/npm/"
check_not_spa "/generic/"

echo "smoke-packages: registry path prefixes do not return SPA HTML"
exit 0
