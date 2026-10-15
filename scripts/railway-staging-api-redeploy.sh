#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Repin the Oxidean Cloud staging `api` service's CLI feed and re-pull its
# image so the api container's entrypoint fetches the new ox-v* release feed.
#
# The service's OXIDEAN_CLI_DIST_URL is pinned to the `ox-v*` release tag on
# the given commit (the tag's release assets are the CLI staging feed), then
# serviceInstanceRedeploy re-pulls the service's current image — the boot
# entrypoint re-fetches the feed. No image rebuild is involved.
# Untagged commits get an empty URL → host-only `ox` from the image.
#
# Usage:
#   RAILWAY_API_TOKEN=… scripts/railway-staging-api-redeploy.sh <sha> [--dry-run]
#   RAILWAY_TOKEN=…     # project token scoped to *staging* (not production)
#
# Auth (checked in order) — same as scripts/railway-production-deploy.sh:
#   RAILWAY_TOKEN       Railway *project* token → Project-Access-Token header
#   RAILWAY_API_TOKEN   account/workspace token → Authorization: Bearer
#   ~/.railway/config.json CLI login
#
# Env overrides:
#   RAILWAY_PROJECT_ID          (default: Oxidean Cloud project)
#   RAILWAY_ENVIRONMENT_ID      (optional; discovered as name "staging" when unset)
#   RAILWAY_ENVIRONMENT_NAME    (default: staging)
#   RAILWAY_SERVICE_API_ID      (default: same api service as production)

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"

PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-}"
ENVIRONMENT_NAME="${RAILWAY_ENVIRONMENT_NAME:-staging}"
SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"

DRY_RUN=0

# Resolve the ox-v* release tag on a commit — prefer a stable cut when several
# tags point at the same SHA. Empty result means the commit has no tag and the
# deploy should serve host-only ox.
resolve_ox_dist_url() {
  local sha="$1"
  if command -v git >/dev/null 2>&1; then
    git fetch --tags --force origin >/dev/null 2>&1 || true
    local tag
    tag="$(git tag --points-at "$sha" --list 'ox-v*.*.*' \
      | grep -v -e '-alpha' -e '-beta' -e '-nightly' | sort -V | tail -n1)"
    if [[ -z "$tag" ]]; then
      tag="$(git tag --points-at "$sha" --list 'ox-v*' | sort -V | tail -n1)"
    fi
    if [[ -n "$tag" ]]; then
      printf '%s' "https://github.com/${GITHUB_REPOSITORY:-oxidean/oxidean}/releases/download/${tag}"
      return 0
    fi
  fi
  printf ''
}

# Pin the api service's CLI feed to the tag's release download URL (or empty =
# host-only). The container entrypoint fetches this URL at boot and keys its
# cache on it — a per-tag pin re-fetches naturally, no separate bust variable.
upsert_cli_dist_url() {
  local url="$1"
  local env_id="$2"
  echo "staging api: upsert OXIDEAN_CLI_DIST_URL=${url:-<empty=host-only>} (skipDeploys)"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: would call variableUpsert OXIDEAN_CLI_DIST_URL"
    return 0
  fi
  railway_gql '
    mutation($input: VariableUpsertInput!) {
      variableUpsert(input: $input)
    }
  ' "$(jq -n \
    --arg projectId "$PROJECT_ID" \
    --arg environmentId "$env_id" \
    --arg serviceId "$SERVICE_API_ID" \
    --arg value "$url" \
    '{input:{
      projectId:$projectId,
      environmentId:$environmentId,
      serviceId:$serviceId,
      name:"OXIDEAN_CLI_DIST_URL",
      value:$value,
      skipDeploys:true
    }}')" >/dev/null
}

redeploy_api() {
  local env_id="$1"
  echo "staging api redeploy: environment=${env_id} service=${SERVICE_API_ID} (re-pull image → boot refetches feed)"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: would call serviceInstanceRedeploy"
    return 0
  fi
  local raw ok
  raw="$(
    railway_gql '
      mutation($environmentId: String!, $serviceId: String!) {
        serviceInstanceRedeploy(
          environmentId: $environmentId
          serviceId: $serviceId
        )
      }
    ' "$(jq -n \
      --arg environmentId "$env_id" \
      --arg serviceId "$SERVICE_API_ID" \
      '{environmentId:$environmentId,serviceId:$serviceId}')"
  )"
  ok="$(echo "$raw" | jq -r '.data.serviceInstanceRedeploy')"
  [[ "$ok" == "true" ]] || die "staging api redeploy: serviceInstanceRedeploy returned ${ok}"
  echo "ok"
}

main() {
  need_jq
  local sha=""
  local arg
  for arg in "$@"; do
    case "$arg" in
      --dry-run) DRY_RUN=1 ;;
      -h|--help)
        sed -n '2,25p' "$0"
        exit 0
        ;;
      *)
        if [[ -z "$sha" ]]; then
          sha="$arg"
        else
          die "unexpected argument: ${arg}"
        fi
        ;;
    esac
  done
  [[ -n "$sha" ]] || die "usage: $0 <commit-sha> [--dry-run]"
  if [[ ! "$sha" =~ ^[0-9a-f]{7,40}$ ]]; then
    die "invalid commit SHA: ${sha}"
  fi
  if command -v git >/dev/null 2>&1 && git rev-parse --verify "${sha}^{commit}" >/dev/null 2>&1; then
    sha="$(git rev-parse "${sha}^{commit}")"
  fi
  local env_id url
  if [[ -n "$ENVIRONMENT_ID" ]]; then
    env_id="$ENVIRONMENT_ID"
  else
    env_id="$(railway_resolve_environment_id "$PROJECT_ID" "$ENVIRONMENT_NAME")"
  fi
  url="$(resolve_ox_dist_url "$sha")"
  upsert_cli_dist_url "$url" "$env_id"
  redeploy_api "$env_id"
}

main "$@"
