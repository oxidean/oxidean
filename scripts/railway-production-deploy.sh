#!/usr/bin/env bash
# Promote a git commit to Railway production (api/web/gateway) or roll back
# each service to the previous canRollback deployment — without Environment Sync.
#
# Usage:
#   RAILWAY_TOKEN=… scripts/railway-production-deploy.sh promote <sha> [--dry-run]
#   RAILWAY_TOKEN=… scripts/railway-production-deploy.sh rollback [--dry-run]
#   scripts/railway-production-deploy.sh list   # query-only (auth required)
#
# Auth (checked in order):
#   RAILWAY_TOKEN       Railway *project* token — sent via the
#                       `Project-Access-Token` header (scoped to one
#                       environment; recommended for CI).
#   RAILWAY_API_TOKEN   Railway *account/workspace* token — sent via
#                       `Authorization: Bearer`.
#   ~/.railway/config.json  CLI login access token — `Authorization: Bearer`.
#
# Env overrides (defaults = Oxidean Cloud production):
#   RAILWAY_PROJECT_ID, RAILWAY_ENVIRONMENT_ID
#   RAILWAY_SERVICE_API_ID, RAILWAY_SERVICE_WEB_ID, RAILWAY_SERVICE_GATEWAY_ID
#   OXIDEAN_PRODUCTION_HEALTH_URL (default https://app.oxidean.dev/health)
#   SKIP_HEALTH_CHECK=1 to skip the post-action health probe

set -euo pipefail

API_URL="${RAILWAY_API_URL:-https://backboard.railway.com/graphql/v2}"
PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-b1c33c22-90ce-433b-99f6-864656512115}"
SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
SERVICE_WEB_ID="${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
SERVICE_GATEWAY_ID="${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"
HEALTH_URL="${OXIDEAN_PRODUCTION_HEALTH_URL:-https://app.oxidean.dev/health}"

DRY_RUN=0

die() {
  echo "error: $*" >&2
  exit 1
}

need_jq() {
  command -v jq >/dev/null 2>&1 || die "jq is required"
}

# Prints one curl config-file line selecting the right auth header for the
# available credential. Railway project tokens are NOT bearer tokens — they
# only authenticate via the `Project-Access-Token` header.
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
    # CLI login stores the bearer in user.accessToken (user.token may be empty).
    t="$(jq -r '.user.accessToken // .user.token // empty' "$cfg" 2>/dev/null || true)"
    if [[ -n "$t" && "$t" != "null" ]]; then
      printf 'header = "Authorization: Bearer %s"\n' "$t"
      return
    fi
  fi
  die "no Railway credential: set RAILWAY_TOKEN (project token) or RAILWAY_API_TOKEN (account/workspace token), or run railway login"
}

# GraphQL: prints response JSON to stdout. Fails on HTTP/transport errors or GraphQL errors.
gql() {
  local query="$1"
  local variables_json
  if [[ $# -ge 2 && -n "${2}" ]]; then
    variables_json="$2"
  else
    variables_json='{}'
  fi
  local auth_line payload
  auth_line="$(resolve_auth_header)"
  payload="$(jq -n --arg q "$query" --argjson v "$variables_json" '{query: $q, variables: $v}')"
  local raw
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
  printf '%s %s\n' gateway "$SERVICE_GATEWAY_ID"
}

# Resolve the ox-v* release tag on a commit — prefer a stable cut when several
# tags point at the same SHA. Empty result means the commit has no tag and the
# api deploy should stage host-only ox (preview semantics).
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

# Pin the api service's CLI feed to the promoted commit's ox-v* tag release
# (empty URL = host-only). Per-tag URLs make the Docker fetch layer key on the
# version — no separate bust variable needed.
upsert_cli_dist_url() {
  local url="$1"
  echo "production api: upsert OXIDEAN_CLI_DIST_URL=${url:-<empty=host-only>} (skipDeploys)"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call variableUpsert OXIDEAN_CLI_DIST_URL"
    return 0
  fi
  gql '
    mutation($input: VariableUpsertInput!) {
      variableUpsert(input: $input)
    }
  ' "$(jq -n \
    --arg projectId "$PROJECT_ID" \
    --arg environmentId "$ENVIRONMENT_ID" \
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

health_check() {
  if [[ "${SKIP_HEALTH_CHECK:-0}" == "1" ]]; then
    echo "skip health check (SKIP_HEALTH_CHECK=1)"
    return 0
  fi
  echo "health: GET ${HEALTH_URL}"
  local code
  code="$(curl -sS -o /tmp/oxidean-prod-health.out -w '%{http_code}' --max-time 30 "$HEALTH_URL" || true)"
  if [[ "$code" != 2* ]]; then
    echo "health body:" >&2
    cat /tmp/oxidean-prod-health.out >&2 || true
    die "health check failed (HTTP ${code})"
  fi
  echo "health: ok (HTTP ${code})"
}

cmd_list() {
  echo "project=${PROJECT_ID} environment=${ENVIRONMENT_ID}"
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    echo "--- ${name} (${sid}) ---"
    local raw
    raw="$(
      gql '
        query($input: DeploymentListInput!, $first: Int) {
          deployments(input: $input, first: $first) {
            edges { node { id status canRollback createdAt } }
          }
        }
      ' "$(jq -n \
        --arg projectId "$PROJECT_ID" \
        --arg environmentId "$ENVIRONMENT_ID" \
        --arg serviceId "$sid" \
        '{input:{projectId:$projectId,environmentId:$environmentId,serviceId:$serviceId},first:5}')"
    )"
    echo "$raw" | jq -r '
      .data.deployments.edges[]?
      | .node
      | "\(.createdAt)  \(.status)\t canRollback=\(.canRollback)  \(.id)"
    '
  done < <(service_pairs)
}

promote_one() {
  local name="$1"
  local sid="$2"
  local sha="$3"
  echo "promote ${name}: commit ${sha}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call serviceInstanceDeployV2"
    return 0
  fi
  local raw
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
      --arg environmentId "$ENVIRONMENT_ID" \
      --arg serviceId "$sid" \
      --arg commitSha "$sha" \
      '{environmentId:$environmentId,serviceId:$serviceId,commitSha:$commitSha}')"
  )"
  local dep_id
  dep_id="$(echo "$raw" | jq -r '.data.serviceInstanceDeployV2 // empty')"
  [[ -n "$dep_id" ]] || die "promote ${name}: empty deployment id"
  echo "  deployment=${dep_id}"
}

cmd_promote() {
  local sha="${1:-}"
  [[ -n "$sha" ]] || die "promote requires a commit SHA"
  if [[ ! "$sha" =~ ^[0-9a-f]{7,40}$ ]]; then
    die "invalid commit SHA: ${sha}"
  fi
  # Normalize to full SHA when git is available (Actions checkout).
  if command -v git >/dev/null 2>&1 && git rev-parse --verify "${sha}^{commit}" >/dev/null 2>&1; then
    sha="$(git rev-parse "${sha}^{commit}")"
  fi

  # Pin the api service's CLI dist feed to the ox-v* tag on this commit before
  # deploying (empty URL on untagged commits = host-only ox in the image).
  upsert_cli_dist_url "$(resolve_ox_dist_url "$sha")"

  while read -r name sid; do
    [[ -z "$name" ]] && continue
    promote_one "$name" "$sid" "$sha"
  done < <(service_pairs)

  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: skipping health check"
    return 0
  fi
  # Give Railway a moment to start builds before probing; health may still be old revision briefly.
  sleep 5
  health_check
}

# Pick the newest canRollback deployment that is not the current SUCCESS (or first) live row.
rollback_target_id() {
  local sid="$1"
  local raw
  raw="$(
    gql '
      query($input: DeploymentListInput!, $first: Int) {
        deployments(input: $input, first: $first) {
          edges { node { id status canRollback createdAt } }
        }
      }
    ' "$(jq -n \
      --arg projectId "$PROJECT_ID" \
      --arg environmentId "$ENVIRONMENT_ID" \
      --arg serviceId "$sid" \
      '{input:{projectId:$projectId,environmentId:$environmentId,serviceId:$serviceId},first:20}')"
  )"
  echo "$raw" | jq -r '
    .data.deployments.edges
    | map(.node)
    | (map(select(.status == "SUCCESS")) | .[0].id) as $live
    | map(select(.canRollback == true and .id != $live))
    | .[0].id // empty
  '
}

rollback_one() {
  local name="$1"
  local sid="$2"
  local target
  target="$(rollback_target_id "$sid")"
  [[ -n "$target" ]] || die "rollback ${name}: no prior canRollback deployment found"
  echo "rollback ${name}: deployment ${target}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call deploymentRollback"
    return 0
  fi
  local raw
  raw="$(
    gql '
      mutation($id: String!) {
        deploymentRollback(id: $id)
      }
    ' "$(jq -n --arg id "$target" '{id:$id}')"
  )"
  local ok
  ok="$(echo "$raw" | jq -r '.data.deploymentRollback')"
  [[ "$ok" == "true" ]] || die "rollback ${name}: deploymentRollback returned ${ok}"
  echo "  ok"
}

cmd_rollback() {
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    rollback_one "$name" "$sid"
  done < <(service_pairs)

  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: skipping health check"
    return 0
  fi
  sleep 5
  health_check
}

usage() {
  cat <<'EOF'
Usage:
  railway-production-deploy.sh promote <sha> [--dry-run]
  railway-production-deploy.sh rollback [--dry-run]
  railway-production-deploy.sh list
EOF
}

main() {
  need_jq
  local cmd="${1:-}"
  shift || true

  local args=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run) DRY_RUN=1 ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) args+=("$1") ;;
    esac
    shift
  done

  case "$cmd" in
    promote)
      [[ ${#args[@]} -ge 1 ]] || die "promote requires <sha>"
      cmd_promote "${args[0]}"
      ;;
    rollback)
      cmd_rollback
      ;;
    list)
      cmd_list
      ;;
    "" | -h | --help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      die "unknown command: ${cmd}"
      ;;
  esac
}

main "$@"
