#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Promote a commit's GHCR image set to Railway production (api/web/runner/
# gateway) or roll back each service to the previous canRollback deployment —
# without Environment Sync.
#
# Deploys pin each service's source.image to ghcr.io/oxidean/oxidean-<svc>:
# sha-<sha> (built + pushed as an all-or-nothing set by images.yml); rollback
# uses Railway deployment history, which restores the previous image ref.
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
#   RAILWAY_SERVICE_API_ID, RAILWAY_SERVICE_WEB_ID,
#   RAILWAY_SERVICE_RUNNER_ID, RAILWAY_SERVICE_GATEWAY_ID
#   OXIDEAN_PRODUCTION_HEALTH_URL (default https://app.oxidean.dev/health)
#   SKIP_HEALTH_CHECK=1 to skip the post-action health probe

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"
# shellcheck source=scripts/ox-dist-tag.sh
source "${SCRIPT_DIR}/ox-dist-tag.sh"

PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-b1c33c22-90ce-433b-99f6-864656512115}"
SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
SERVICE_WEB_ID="${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
SERVICE_RUNNER_ID="${RAILWAY_SERVICE_RUNNER_ID:-36f9b50a-738a-419a-9171-49f56c902e6a}"
SERVICE_GATEWAY_ID="${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"
HEALTH_URL="${OXIDEAN_PRODUCTION_HEALTH_URL:-https://app.oxidean.dev/health}"

DRY_RUN=0

# Canonical deploy order — backends before the edge.
service_pairs() {
  printf '%s %s\n' api "$SERVICE_API_ID"
  printf '%s %s\n' web "$SERVICE_WEB_ID"
  printf '%s %s\n' runner "$SERVICE_RUNNER_ID"
  printf '%s %s\n' gateway "$SERVICE_GATEWAY_ID"
}

# Resolve the ox-v* release tag for a commit — prefer a stable cut pointing at
# the SHA; when none does (e.g. a stable-cut bump commit on a promote that did
# not bump ox), fall back to the newest stable ox tag merged into the commit so
# production keeps serving the latest released CLI instead of going host-only.
# Empty result = no released ox at all → host-only ox in the image.
resolve_ox_dist_url() {
  local sha="$1"
  if command -v git >/dev/null 2>&1; then
    git fetch --tags --force origin >/dev/null 2>&1 || true
    ox_dist_url_for_sha "$sha" --merged-fallback
    return 0
  fi
  printf ''
}

# Pin the api service's CLI feed to the promoted commit's ox-v* tag release
# (empty URL = host-only). The image's entrypoint fetches this URL at boot —
# the pin is a runtime env var, so no rebuild is ever needed to change feeds.
upsert_cli_dist_url() {
  local url="$1"
  echo "production api: upsert OXIDEAN_CLI_DIST_URL=${url:-<empty=host-only>} (skipDeploys)"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call variableUpsert OXIDEAN_CLI_DIST_URL"
    return 0
  fi
  railway_gql '
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
      railway_gql '
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

  # Pin the api service's runtime CLI feed to the ox-v* tag on this commit
  # before deploying (empty URL on untagged commits = host-only ox in the image).
  upsert_cli_dist_url "$(resolve_ox_dist_url "$sha")"

  # Atomic image deploy: all four services pin ghcr.io/oxidean/oxidean-<svc>:
  # sha-<sha>, then the :production channel tag follows the deployed bits.
  # --wait makes the call return only once every service's new deployment is
  # terminal (dry-run skips it).
  local args=(--environment-id "$ENVIRONMENT_ID" --tag "sha-${sha}" --channel-tag production --wait)
  if [[ "$DRY_RUN" == "1" ]]; then
    args+=(--dry-run)
  fi
  "${SCRIPT_DIR}/railway-image-deploy.sh" "${args[@]}"

  if [[ "$DRY_RUN" == "1" ]]; then
    echo "dry-run: skipping health check"
    return 0
  fi
  # Give Railway a moment to start pulling images before probing; health may
  # still be the old replica briefly.
  sleep 5
  health_check
}

# Pick the newest canRollback deployment that is not the current SUCCESS (or first) live row.
rollback_target_id() {
  local sid="$1"
  local raw
  raw="$(
    railway_gql '
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
  local name="$1" sid="$2"
  local target
  target="$(rollback_target_id "$sid")"
  if [[ -z "$target" ]]; then
    # Warn + continue, not die: a service with no deploy history (e.g. a
    # newly added runner) must not block rolling back the rest.
    echo "rollback ${name}: no prior canRollback deployment — skipped" >&2
    return 0
  fi
  echo "rollback ${name}: deployment ${target}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call deploymentRollback"
    return 0
  fi
  local raw
  raw="$(
    railway_gql '
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
