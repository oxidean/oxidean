#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Assert Oxidean Cloud production has no GitHub deployment triggers
# (autodeploy off). Staging is expected to keep triggers — not checked here.
#
# Post image-migration (oxidean#124): services pull GHCR images instead of
# building from GitHub, so image-sourced services cannot carry deployment
# triggers at all — this check guards against re-adding a repo source or
# manual Autodeploy flips.
#
# Usage:
#   RAILWAY_TOKEN=… scripts/railway-production-autodeploy-check.sh
#   make cloud-production-autodeploy-check
#
# Prefers `railway api` (CLI login). Falls back to curl with the first
# available credential: RAILWAY_TOKEN (project token → `Project-Access-Token`
# header), RAILWAY_API_TOKEN (account/workspace token → `Authorization:
# Bearer`), or ~/.railway/config.json accessToken (Bearer).
#
# Exit 0 when production api/web/runner/gateway have zero deploymentTriggers.
# Exit 1 if any trigger exists (autodeploy would fire on push to the branch).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"

API_URL="$(railway_api_url)"
PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-b1c33c22-90ce-433b-99f6-864656512115}"
SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
SERVICE_WEB_ID="${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
SERVICE_RUNNER_ID="${RAILWAY_SERVICE_RUNNER_ID:-36f9b50a-738a-419a-9171-49f56c902e6a}"
SERVICE_GATEWAY_ID="${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"

QUERY='query($projectId: String!, $environmentId: String!, $serviceId: String!) {
  deploymentTriggers(
    projectId: $projectId
    environmentId: $environmentId
    serviceId: $serviceId
  ) {
    edges {
      node { id branch checkSuites repository provider }
    }
  }
}'

# Prints response JSON to stdout.
fetch_triggers() {
  local sid="$1"
  if command -v railway >/dev/null 2>&1; then
    railway api --variables "$(
      jq -n \
        --arg projectId "$PROJECT_ID" \
        --arg environmentId "$ENVIRONMENT_ID" \
        --arg serviceId "$sid" \
        '{projectId:$projectId,environmentId:$environmentId,serviceId:$serviceId}'
    )" "$QUERY"
    return
  fi
  local auth_line payload raw
  auth_line="$(railway_auth_header)"
  payload="$(
    jq -n \
      --arg q "$QUERY" \
      --arg projectId "$PROJECT_ID" \
      --arg environmentId "$ENVIRONMENT_ID" \
      --arg serviceId "$sid" \
      '{
        query: $q,
        variables: {
          projectId: $projectId,
          environmentId: $environmentId,
          serviceId: $serviceId
        }
      }'
  )"
  raw="$(
    printf '%s' "$payload" | curl -sS "$API_URL" \
      -H "Content-Type: application/json" \
      --config <(printf '%s' "$auth_line") \
      -d @-
  )" || die "Railway GraphQL request failed"
  if echo "$raw" | jq -e '.errors? | select(length > 0)' >/dev/null 2>&1; then
    echo "$raw" | jq '.' >&2
    die "Railway GraphQL returned errors"
  fi
  printf '%s' "$raw"
}

service_pairs() {
  printf '%s %s\n' api "$SERVICE_API_ID"
  printf '%s %s\n' web "$SERVICE_WEB_ID"
  printf '%s %s\n' runner "$SERVICE_RUNNER_ID"
  printf '%s %s\n' gateway "$SERVICE_GATEWAY_ID"
}

main() {
  need_jq
  echo "project=${PROJECT_ID} environment=${ENVIRONMENT_ID} (production autodeploy check)"
  local failed=0
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    local raw count
    raw="$(fetch_triggers "$sid")"
    count="$(echo "$raw" | jq '.data.deploymentTriggers.edges | length')"
    if [[ "$count" != "0" ]]; then
      echo "FAIL ${name}: ${count} deployment trigger(s) — autodeploy is ON" >&2
      echo "$raw" | jq -r '
        .data.deploymentTriggers.edges[]?.node
        | "  id=\(.id) branch=\(.branch) checkSuites=\(.checkSuites) repo=\(.repository)"
      ' >&2
      failed=1
    else
      echo "ok ${name}: no deployment triggers (autodeploy off)"
    fi
  done < <(service_pairs)

  if [[ "$failed" -ne 0 ]]; then
    cat >&2 <<'EOF'

Production must not autodeploy. Services pull GHCR images pinned by deploy
tooling — a GitHub deployment trigger means a repo source or Autodeploy was
re-added. Remove the trigger (service Settings → disconnect repo source /
disable Autodeploy), then re-run this check.

Promote only via: Actions → Production deploy (workflow_dispatch).
Docs: docs/DEPLOYMENT.md, .railway/README.md
EOF
    exit 1
  fi
  echo "all production services: autodeploy off"
}

main "$@"
