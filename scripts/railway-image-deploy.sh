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
#   --wait                              after pinning + rolling, poll the
#                                       triggered deployments until all reach
#                                       a terminal state (SUCCESS/SKIPPED ok;
#                                       FAILED/CRASHED/REMOVED fail) — makes
#                                       "redeploy done" observable instead of
#                                       returning at mutation-accepted
#   --timeout <secs>                    --pr environment wait + --wait bound
#                                       (default 600)
#   --dry-run                           print mutations without applying
#
# --pr resolves the ephemeral environment whose meta.prNumber matches — or
# the conventional oxidean-pr-<n> name for envs CI created itself
# (railway-pr-env-ensure.sh) — polling until --timeout. No match after the
# bound → warn + exit 0 (a later push retries; the env may not exist).
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

# Poll for the ephemeral PR environment (Railway may create it
# asynchronously when the PR opens, or CI may have created it — either is
# found). Prints the environment id; empty when the bound elapses.
wait_for_pr_environment() {
  local pr="$1" timeout="$2"
  local deadline=$((SECONDS + timeout)) env_id=""
  while (( SECONDS < deadline )); do
    env_id="$(railway_find_pr_environment_id "$PROJECT_ID" "$pr" 2>/dev/null || true)"
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

# Poll the deployments this run triggered (exact ids from
# serviceInstanceDeployV2) until all reach a terminal state. Prints a status
# line whenever one changes. Dies on failure or timeout. Transient query
# failures count as pending — a network blip must not fail a healthy deploy.
wait_for_deployments() {
  local env_id="$1" ids_csv="$2" timeout="$3"
  local deadline=$((SECONDS + timeout))

  declare -A last_seen=()
  while (( SECONDS < deadline )); do
    local raw
    if ! raw="$(
      railway_gql '
        query($id: String!) {
          environment(id: $id) {
            deployments(first: 100) {
              edges { node { id status } }
            }
          }
        }
      ' "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null
    )"; then
      echo "  wait: Railway query failed — retrying" >&2
      sleep 15
      continue
    fi
    local pending=0 hard_failed=0 soft_failed=0
    local dep_id status st
    for dep_id in ${ids_csv//,/ }; do
      status="$(echo "$raw" | jq -r --arg id "$dep_id" '
        [.data.environment.deployments.edges[].node | select(.id == $id)]
        | first | .status // "missing"
      ' 2>/dev/null || echo "missing")"
      # CRASHED is soft: restart policies can flap a deployment back to
      # DEPLOYING — only fail once every tracked id is terminal. FAILED and
      # REMOVED never recover — die immediately.
      case "$status" in
        SUCCESS | SKIPPED) st="ok" ;;
        FAILED | REMOVED) st="failed:${status}" ;;
        CRASHED) st="crashed" ;;
        *) st="$status" ;;  # QUEUED/BUILDING/DEPLOYING/INITIALIZING/missing…
      esac
      if [[ "${last_seen[$dep_id]:-}" != "$st" ]]; then
        echo "  wait: deployment ${dep_id:0:8}… ${st}"
        last_seen[$dep_id]="$st"
      fi
      case "$st" in
        ok) ;;
        failed:*) hard_failed=$((hard_failed + 1)) ;;
        crashed) soft_failed=$((soft_failed + 1)) ;;
        *) pending=$((pending + 1)) ;;
      esac
    done
    if (( hard_failed > 0 )); then
      die "deploy failed in environment ${env_id} (see Railway logs)"
    fi
    if (( pending == 0 )); then
      if (( soft_failed > 0 )); then
        die "deployments crashed in environment ${env_id} (see Railway logs)"
      fi
      echo "wait: all triggered deployments reached a terminal state"
      return 0
    fi
    sleep 15
  done
  die "deployments did not reach a terminal state within ${timeout}s"
}

update_service_image() {
  local env_id="$1" name="$2" sid="$3" ref="$4"
  echo "pin ${name}: ${ref}"
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
  [[ "$ok" == "true" ]] || die "pin ${name}: serviceInstanceUpdate returned ${ok}"
}

# serviceInstanceUpdate only stages the source change — nothing rolls until a
# deploy is triggered. serviceInstanceDeployV2 rolls the service's current
# config (no commitSha for image sources) and returns the new deployment id,
# which --wait then tracks directly. Progress goes to stderr so the caller can
# capture the id on stdout.
deploy_service() {
  local env_id="$1" name="$2" sid="$3"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: would call serviceInstanceDeployV2 for ${name}" >&2
    return 0
  fi
  local raw dep_id
  raw="$(
    railway_gql '
      mutation($environmentId: String!, $serviceId: String!) {
        serviceInstanceDeployV2(environmentId: $environmentId, serviceId: $serviceId)
      }
    ' "$(jq -n --arg environmentId "$env_id" --arg serviceId "$sid" \
      '{environmentId:$environmentId, serviceId:$serviceId}')"
  )"
  dep_id="$(echo "$raw" | jq -r '.data.serviceInstanceDeployV2 // empty')"
  if [[ -z "$dep_id" || "$dep_id" == "null" ]]; then
    die "deploy ${name}: serviceInstanceDeployV2 returned ${raw}"
  fi
  echo "  roll ${name}: deployment ${dep_id:0:8}…" >&2
  printf '%s' "$dep_id"
}

usage() {
  sed -n '2,38p' "$0"
}

main() {
  need_jq
  local env_name="" env_id_arg="" pr="" tag="" services="api,web,runner,gateway"
  local channel_tag="" timeout=600 wait=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --environment) env_name="$2"; shift ;;
      --environment-id) env_id_arg="$2"; shift ;;
      --pr) pr="$2"; shift ;;
      --tag) tag="$2"; shift ;;
      --services) services="$2"; shift ;;
      --channel-tag) channel_tag="$2"; shift ;;
      --wait) wait=1 ;;
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

  # Validate --services up front — a typo'd name must fail loudly, not deploy
  # a partial set.
  local want=" ${services//,/ } " name sid known=" " valid=""
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    known+="${name} "
    [[ "$want" == *" $name "* ]] && valid+="${name} "
  done < <(service_pairs)
  local s
  for s in ${services//,/ }; do
    [[ "$known" == *" $s "* ]] || die "unknown service '${s}' (known:${known})"
  done
  [[ -n "$valid" ]] || die "--services matched nothing (known:${known})"

  # Phase 1: pin every bound service's image source (config only — nothing
  # rolls yet). Phase 2 then triggers one deploy per service, so every pin
  # settles before any roll and a mid-loop failure never leaves a half-rolled
  # mixed-revision set on new pins.
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    [[ "$want" == *" $name "* ]] || continue
    update_service_image "$env_id" "$name" "$sid" "${IMAGE_BASE}/oxidean-${name}:${tag}"
  done < <(service_pairs)

  local dep_ids="" dep_id
  while read -r name sid; do
    [[ -z "$name" ]] && continue
    [[ "$want" == *" $name "* ]] || continue
    dep_id="$(deploy_service "$env_id" "$name" "$sid")"
    [[ -n "$dep_id" ]] && dep_ids+="${dep_id},"
  done < <(service_pairs)
  dep_ids="${dep_ids%,}"

  # Wait before moving the channel tag: on failure the die above means
  # :staging / :production keep pointing at the env's last healthy image, so
  # a `railway config apply` never redeploys a broken sha.
  if [[ "$wait" == "1" && "$DRY_RUN" != "1" && -n "$dep_ids" ]]; then
    wait_for_deployments "$env_id" "$dep_ids" "$timeout"
  fi

  if [[ -n "$channel_tag" ]]; then
    retag_channel "$tag" "$channel_tag" "$services"
  fi
  echo "done: ${tag} deployed to environment ${env_id}"
}

main "$@"
