#!/usr/bin/env bash
# Redeploy the Oxidean Cloud staging `api` service at a git commit SHA.
#
# Before deploy, pins the service's OXIDEAN_CLI_DIST_URL to the `ox-v*` release
# tag on that commit (the tag's release assets are the CLI staging feed).
# Untagged commits get an empty URL → the image stages host-only `ox`.
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

API_URL="${RAILWAY_API_URL:-https://backboard.railway.com/graphql/v2}"
PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-}"
ENVIRONMENT_NAME="${RAILWAY_ENVIRONMENT_NAME:-staging}"
SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"

DRY_RUN=0

die() {
  echo "error: $*" >&2
  exit 1
}

need_jq() {
  command -v jq >/dev/null 2>&1 || die "jq is required"
}

resolve_auth_header() {
  if [[ -n "${RAILWAY_TOKEN:-}" ]]; then
    printf 'header = "Project-Access-Token: %s"\n' "$RAILWAY_TOKEN"
    return
  fi
  if [[ -n "${RAILWAY_API_TOKEN:-}" ]]; then
    printf 'header = "Authorization: Bearer %s"\n' "$RAILWAY_API_TOKEN"
    return
  fi
  local cfg="${HOME}/.railway/config.json"
  if [[ -f "$cfg" ]]; then
    local t
    t="$(jq -r '.user.accessToken // .user.token // empty' "$cfg" 2>/dev/null || true)"
    if [[ -n "$t" && "$t" != "null" ]]; then
      printf 'header = "Authorization: Bearer %s"\n' "$t"
      return
    fi
  fi
  die "no Railway credential: set RAILWAY_TOKEN (staging project token) or RAILWAY_API_TOKEN, or run railway login"
}

gql() {
  local query="$1"
  local variables_json
  if [[ $# -ge 2 && -n "${2}" ]]; then
    variables_json="$2"
  else
    variables_json='{}'
  fi
  local auth_line payload raw
  auth_line="$(resolve_auth_header)"
  payload="$(jq -n --arg q "$query" --argjson v "$variables_json" '{query: $q, variables: $v}')"
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

resolve_staging_environment_id() {
  if [[ -n "$ENVIRONMENT_ID" ]]; then
    printf '%s' "$ENVIRONMENT_ID"
    return
  fi
  local raw id
  raw="$(
    gql '
      query($id: String!) {
        project(id: $id) {
          environments { edges { node { id name } } }
        }
      }
    ' "$(jq -n --arg id "$PROJECT_ID" '{id:$id}')"
  )"
  id="$(echo "$raw" | jq -r --arg name "$ENVIRONMENT_NAME" '
    .data.project.environments.edges[]?
    | .node
    | select(.name == $name)
    | .id
  ' | head -n1)"
  [[ -n "$id" && "$id" != "null" ]] || die "environment named '${ENVIRONMENT_NAME}' not found in project ${PROJECT_ID}"
  printf '%s' "$id"
}

# Resolve the ox-v* release tag on a commit — prefer a stable cut when several
# tags point at the same SHA. Empty result means the commit has no tag and the
# deploy should stage host-only ox.
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
# host-only). The Dockerfile fetch layer keys on the URL, so a per-tag pin busts
# the cache naturally — no separate BUST variable.
upsert_cli_dist_url() {
  local url="$1"
  local env_id="$2"
  echo "staging api: upsert OXIDEAN_CLI_DIST_URL=${url:-<empty=host-only>} (skipDeploys)"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: would call variableUpsert OXIDEAN_CLI_DIST_URL"
    return 0
  fi
  gql '
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

promote_api() {
  local sha="$1"
  local env_id="$2"
  echo "staging api redeploy: environment=${env_id} service=${SERVICE_API_ID} commit=${sha}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: would call serviceInstanceDeployV2"
    return 0
  fi
  local raw dep_id
  raw="$(
    gql '
      mutation($environmentId: String!, $serviceId: String!, $commitSha: String!) {
        serviceInstanceDeployV2(
          environmentId: $environmentId
          serviceId: $serviceId
          commitSha: $commitSha
        )
      }
    ' "$(jq -n \
      --arg environmentId "$env_id" \
      --arg serviceId "$SERVICE_API_ID" \
      --arg commitSha "$sha" \
      '{environmentId:$environmentId,serviceId:$serviceId,commitSha:$commitSha}')"
  )"
  dep_id="$(echo "$raw" | jq -r '.data.serviceInstanceDeployV2 // empty')"
  [[ -n "$dep_id" ]] || die "staging api redeploy: empty deployment id"
  echo "deployment=${dep_id}"
}

main() {
  need_jq
  local sha=""
  local arg
  for arg in "$@"; do
    case "$arg" in
      --dry-run) DRY_RUN=1 ;;
      -h|--help)
        sed -n '2,20p' "$0"
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
  env_id="$(resolve_staging_environment_id)"
  url="$(resolve_ox_dist_url "$sha")"
  upsert_cli_dist_url "$url" "$env_id"
  promote_api "$sha" "$env_id"
}

main "$@"
