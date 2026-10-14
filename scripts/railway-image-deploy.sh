#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Deploy a GHCR image tag to a Railway environment by pinning each service's
# source.image — the image-based replacement for serviceInstanceDeployV2
# commit deploys (oxidean#124).
#
# Usage:
#   scripts/railway-image-deploy.sh --environment <name> --tag <image-tag> [opts]
#   scripts/railway-image-deploy.sh --environment-id <id> --tag <image-tag> [opts]
#   scripts/railway-image-deploy.sh --pr <number> --tag <image-tag> [opts]
#
# Options:
#   --services api,web,runner,gateway   service subset (canonical deploy order
#                                     api → web → runner → gateway is kept)
#   --channel-tag <tag>                 after a successful deploy, retag each
#                                       pushed image <tag> → <channel-tag> on
#                                       GHCR (keeps the IaC channel seed — e.g.
#                                       :staging / :production — pointing at
#                                       the environment's live image)
#   --timeout <secs>                    --pr environment wait bound (default 600)
#   --dry-run                           print mutations without applying
#
# --pr resolves the ephemeral environment whose meta.prNumber matches, polling
# until --timeout. No matching environment after the bound → warn + exit 0
# (a later push retries; the env may simply not have been created).
#
# Auth: same chain as railway-production-deploy.sh (RAILWAY_TOKEN project
# token → Project-Access-Token; RAILWAY_API_TOKEN → Bearer; CLI login).
#
# Env overrides:
#   RAILWAY_PROJECT_ID          (default: Oxidean Cloud project)
#   RAILWAY_ENVIRONMENT_ID      (used directly when --environment-id is absent
#                               and set; beats --environment name resolution)
#   RAILWAY_SERVICE_API_ID / RAILWAY_SERVICE_WEB_ID /
#   RAILWAY_SERVICE_RUNNER_ID / RAILWAY_SERVICE_GATEWAY_ID
#   OXIDEAN_IMAGE_BASE          (default ghcr.io/oxidean)
#
# Channel retag needs `docker` + a ghcr.io login with push rights. Without it
# the deploy still succeeds and a warning is printed.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"

PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
ENVIRONMENT_ID="${RAILWAY_ENVIRONMENT_ID:-}"
IMAGE_BASE="${OXIDEAN_IMAGE_BASE:-ghcr.io/oxidean}"

SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
SERVICE_WEB_ID="${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
SERVICE_RUNNER_ID="${RAILWAY_SERVICE_RUNNER_ID:-36f9b50a-738a-419a-9171-49f56c902e6a}"
SERVICE_GATEWAY_ID="${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"

DRY_RUN=0

# Canonical deploy order — backends before the edge.
service_pairs() {
  printf '%s %s\n' api "$SERVICE_API_ID"
  printf '%s %s\n' web "$SERVICE_WEB_ID"
  printf '%s %s\n' runner "$SERVICE_RUNNER_ID"
  printf '%s %s\n' gateway "$SERVICE_GATEWAY_ID"
}

# Poll for the ephemeral PR environment (Railway creates it asynchronously
# when the PR opens). Prints the environment id; empty when the bound elapses.
wait_for_pr_environment() {
  local pr="$1" timeout="$2"
  local deadline=$((SECONDS + timeout)) env_id=""
  while (( SECONDS < deadline )); do
    env_id="$(railway_find_pr_environment_id "$PROJECT_ID" "$pr")"
    if [[ -n "$env_id" && "$env_id" != "null" ]]; then
      printf '%s' "$env_id"
      return 0
    fi
    echo "waiting for Railway to create the PR #${pr} environment (sleep 15s)" >&2
    sleep 15
  done
  printf ''
}

# Retag pushed image <tag> → <channel> on GHCR (tag-only op, no rebuild).
# Keeps the environment's IaC channel seed pointing at its live image so a
# `railway config apply` redeploys identical bits instead of drifting.
retag_channel() {
  local tag="$1" channel="$2" services_csv="$3"
  echo "channel tag: ${tag} → ${channel}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would retag ${services_csv} images to :${channel}"
    return 0
  fi
  if ! command -v docker >/dev/null 2>&1; then
    echo "warning: docker not on PATH — :${channel} channel tag NOT moved" >&2
    echo "         run: docker buildx imagetools create -t <img>:${channel} <img>:${tag}" >&2
    return 0
  fi
  local svc failed=0
  IFS=',' read -r -a svcs <<< "$services_csv"
  for svc in "${svcs[@]}"; do
    local img="${IMAGE_BASE}/oxidean-${svc}"
    if docker buildx imagetools create -t "${img}:${channel}" "${img}:${tag}" >/dev/null 2>&1; then
      echo "  ${img}:${channel} → ${tag}"
    else
      echo "warning: retag failed for ${img} (docker ghcr.io login with push rights?)" >&2
      failed=1
    fi
  done
  if [[ "$failed" == "1" ]]; then
    echo "warning: one or more channel retags failed — :${channel} may lag behind" >&2
  fi
}

update_service_image() {
  local env_id="$1" name="$2" sid="$3" ref="$4"
  echo "deploy ${name}: ${ref}"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call serviceInstanceUpdate source.image=${ref}"
    return 0
  fi
  local raw ok
  raw="$(
    railway_gql '
      mutation($environmentId: String!, $serviceId: String!, $input: ServiceInstanceUpdateInput!) {
        serviceInstanceUpdate(
          environmentId: $environmentId
          serviceId: $serviceId
          input: $input
        )
      }
    ' "$(jq -n \
      --arg environmentId "$env_id" \
      --arg serviceId "$sid" \
      --arg image "$ref" \
      '{environmentId:$environmentId,serviceId:$serviceId,input:{source:{image:$image}}}')"
  )"
  ok="$(echo "$raw" | jq -r '.data.serviceInstanceUpdate')"
  [[ "$ok" == "true" ]] || die "deploy ${name}: serviceInstanceUpdate returned ${ok}"
}

usage() {
  sed -n '2,38p' "$0"
}

main() {
  need_jq
  local env_name="" env_id_arg="" pr="" tag="" services="api,web,runner,gateway"
  local channel_tag="" timeout=600
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --environment) env_name="$2"; shift ;;
      --environment-id) env_id_arg="$2"; shift ;;
      --pr) pr="$2"; shift ;;
      --tag) tag="$2"; shift ;;
      --services) services="$2"; shift ;;
      --channel-tag) channel_tag="$2"; shift ;;
      --timeout) timeout="$2"; shift ;;
      --dry-run) DRY_RUN=1 ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) die "unexpected argument: $1" ;;
    esac
    shift
  done

  [[ -n "$tag" ]] || die "--tag is required (e.g. sha-<commit> or pr-<n>-sha-<sha>)"
  [[ "$tag" =~ ^[A-Za-z0-9._-]+$ ]] || die "invalid image tag: ${tag}"

  # Resolve the target environment.
  local env_id=""
  if [[ -n "$env_id_arg" ]]; then
    env_id="$env_id_arg"
  elif [[ -n "$ENVIRONMENT_ID" ]]; then
    env_id="$ENVIRONMENT_ID"
  elif [[ -n "$env_name" ]]; then
    env_id="$(railway_resolve_environment_id "$PROJECT_ID" "$env_name")"
  elif [[ -n "$pr" ]]; then
    echo "looking for ephemeral environment for PR #${pr} (bound ${timeout}s)"
    env_id="$(wait_for_pr_environment "$pr" "$timeout")"
    if [[ -z "$env_id" ]]; then
      echo "warning: no Railway environment found for PR #${pr} after ${timeout}s — nothing to bind (a later push retries)" >&2
      exit 0
    fi
  else
    usage >&2
    die "one of --environment, --environment-id, or --pr is required"
  fi
  echo "environment=${env_id} tag=${tag} services=${services}"

  # Pre-flight: every requested image manifest must exist before the first
  # Railway mutation — never pin a service at an unpushable ref. A dry-run
  # still prints the plan when images are missing (it exists to preview them).
  if ! "${SCRIPT_DIR}/ghcr-images.sh" check "$tag" --services "$services"; then
    if [[ "$DRY_RUN" == "1" ]]; then
      echo "warning: image set ${tag} incomplete — dry-run continues" >&2
    else
      die "image set ${tag} incomplete — refusing to deploy"
    fi
  fi

  # Deploy in canonical order (the --services list filters, never reorders).
  local want=" ${services//,/ } " name sid
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    [[ "$want" == *" $name "* ]] || continue
    update_service_image "$env_id" "$name" "$sid" "${IMAGE_BASE}/oxidean-${name}:${tag}"
  done < <(service_pairs)

  if [[ -n "$channel_tag" ]]; then
    retag_channel "$tag" "$channel_tag" "$services"
  fi
  echo "done: ${tag} deployed to environment ${env_id}"
}

main "$@"
