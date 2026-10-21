#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Find-or-create the ephemeral Railway environment for a pull request
# (oxidean-pr-<n>). Prints the environment id on stdout; every status line
# goes to stderr so callers can capture the id directly.
#
# Why this exists: image-sourced services carry no repo triggers, so
# Railway's GitHub app never sees PR events and never provisions the
# ephemeral PR environment (project prDeploys is moot). When the poll finds
# nothing, this script forks the persistent `preview` environment itself —
# same seed Railway's own PR envs use — via environmentCreate.
#
# Naming: the public API cannot set Environment.meta.prNumber, so
# self-created envs are found by the conventional name oxidean-pr-<n>
# (railway_find_pr_environment_id matches meta.prNumber OR the name).
#
# Support services: skipInitialDeploys keeps the four image-managed
# services (api/web/runner/gateway) idle until the bind repins them, but
# it also leaves forked support services (postgres) with no deployment
# at all — an instance that exists yet never runs, so
# postgres.railway.internal never resolves and api crash-loops to
# FAILED. Worse, the fork copies no volumes — postgres cannot boot
# without one. Every emit path therefore runs ensure_support_services:
# an idempotent pass that recreates missing volume mounts and rolls
# non-image services lacking a healthy deployment (which also heals
# envs created before this step).
#
# Auth: environment enumeration/creation needs an account/workspace
# credential — RAILWAY_API_TOKEN or a `railway login` config. Env-scoped
# project tokens (RAILWAY_TOKEN) can neither list nor create environments
# and are ignored here. With no usable credential this warns and exits 0 —
# the bind job soft-skips exactly like the old find-only behaviour.
#
# Usage:
#   RAILWAY_API_TOKEN=… scripts/railway-pr-env-ensure.sh <pr-number> [project-id] [--dry-run]
#
# Env overrides:
#   RAILWAY_PR_ENV_SOURCE_ID    fork source environment id (default: the
#                               environment named "preview", falling back to
#                               the literal Oxidean Cloud preview id)
#   OXIDEAN_PR_ENV_FIND_TIMEOUT seconds to wait for a Railway-provisioned
#                               env before creating one (default 120)
#   OXIDEAN_PR_ENV_FORK_TIMEOUT seconds to wait for the fork's service
#                               instances to materialize (default 240)
#   OXIDEAN_PR_ENV_BOOT_TIMEOUT seconds to wait for triggered support-service
#                               deploys (postgres) to settle (default 180)
#   OXIDEAN_PR_ENV_DRY_RUN=1    same as --dry-run: print the mutation that
#                               would run, touch nothing

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"

# Oxidean Cloud `preview` — the documented PR-env fork source. Resolution by
# name is preferred; this literal is only the fallback when name resolution
# is unavailable (e.g. a token that cannot enumerate project.environments…
# though such a token cannot create either — the fallback exists chiefly for
# --dry-run output).
PREVIEW_ENVIRONMENT_ID="bf9bd7cf-2461-4495-a7c6-b45141ad804d"
SOURCE_ENVIRONMENT_NAME="preview"

SERVICE_API_ID="${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
SERVICE_WEB_ID="${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
SERVICE_RUNNER_ID="${RAILWAY_SERVICE_RUNNER_ID:-36f9b50a-738a-419a-9171-49f56c902e6a}"
SERVICE_GATEWAY_ID="${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"

FIND_TIMEOUT="${OXIDEAN_PR_ENV_FIND_TIMEOUT:-120}"
FORK_TIMEOUT="${OXIDEAN_PR_ENV_FORK_TIMEOUT:-240}"
BOOT_TIMEOUT="${OXIDEAN_PR_ENV_BOOT_TIMEOUT:-180}"
DRY_RUN="${OXIDEAN_PR_ENV_DRY_RUN:-0}"

# Services CI deploys by image pin — every other forked service is a
# support service (postgres today) and needs its own first deploy.
MANAGED_SERVICE_IDS=" ${SERVICE_API_ID} ${SERVICE_WEB_ID} ${SERVICE_RUNNER_ID} ${SERVICE_GATEWAY_ID} "

log() { echo "$*" >&2; }

# Poll for the PR environment (meta.prNumber or oxidean-pr-<n> name) until
# the bound elapses. Prints the id; empty on timeout.
find_pr_environment() {
  local project_id="$1" pr="$2" timeout="$3"
  local deadline=$((SECONDS + timeout)) env_id=""
  while :; do
    env_id="$(railway_find_pr_environment_id "$project_id" "$pr" 2>/dev/null || true)"
    if [[ -n "$env_id" && "$env_id" != "null" ]]; then
      printf '%s' "$env_id"
      return 0
    fi
    (( SECONDS >= deadline )) && break
    log "waiting for the PR #${pr} environment (sleep 15s)"
    sleep 15
  done
  printf ''
}

# environmentCreate forks `preview` asynchronously — serviceInstances fill
# in after the mutation returns. Wait until all four deployable services
# are visible so the bind's image pins land on real instances.
wait_for_service_instances() {
  local env_id="$1" timeout="$2"
  local deadline=$((SECONDS + timeout))
  local want=" ${SERVICE_API_ID} ${SERVICE_WEB_ID} ${SERVICE_RUNNER_ID} ${SERVICE_GATEWAY_ID} "
  while (( SECONDS < deadline )); do
    local raw got sid missing=0
    if raw="$(
      railway_gql '
        query($id: String!) {
          environment(id: $id) {
            serviceInstances { edges { node { serviceId } } }
          }
        }
      ' "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null
    )"; then
      got=" $(echo "$raw" | jq -r '.data.environment.serviceInstances.edges[]?.node.serviceId' | tr '\n' ' ')"
      for sid in $want; do
        [[ "$got" == *" $sid "* ]] || missing=$((missing + 1))
      done
      if (( missing == 0 )); then
        log "environment ${env_id}: all four service instances materialized"
        return 0
      fi
      log "environment ${env_id}: waiting for ${missing} service instance(s) to fork (sleep 10s)"
    else
      log "environment ${env_id}: serviceInstances query failed — retrying (sleep 10s)"
    fi
    sleep 10
  done
  return 1
}

SERVICE_INSTANCES_QUERY='
  query($id: String!) {
    environment(id: $id) {
      serviceInstances {
        edges { node { serviceId serviceName latestDeployment { status } } }
      }
    }
  }
'

# Heal a forked env's data plane. environmentCreate's fork copies
# service config but NOT volumes — preview mounts /var/lib/postgresql/
# data (postgres), /data (runner), /var (api) while a fresh fork has
# none, so the postgres template exits instantly and api/runner crash
# on the missing postgres.railway.internal. This pass (a) creates any
# volume the source env has that the target lacks, for every service
# instance present, then (b) rolls every non-image service that has no
# healthy deployment via serviceInstanceDeployV2 — the same roll the
# bind uses. (environmentTriggersDeploy only fires registered deployment
# triggers; image services have none, so it is a no-op here.)
# Idempotent: existing volumes and healthy services are untouched —
# Railway-provisioned envs arrive complete and this is a no-op — while
# envs created before this step (or half-forked ones) get repaired.
# Best-effort: failures warn but never abort, and the settle wait is
# bounded so a slow postgres cannot stall the job.
ensure_support_services() {
  local env_id="$1" project_id="$2"
  local raw vols_src vols_dst

  # -- volumes: mirror the source env's mounts --------------------------
  local src_id="${RAILWAY_PR_ENV_SOURCE_ID:-}"
  if [[ -z "$src_id" ]]; then
    src_id="$(railway_resolve_environment_id "$project_id" "$SOURCE_ENVIRONMENT_NAME" 2>/dev/null || true)"
  fi
  src_id="${src_id:-$PREVIEW_ENVIRONMENT_ID}"

  vols_src="$(
    railway_gql '
      query($id: String!) {
        environment(id: $id) {
          volumeInstances { edges { node { serviceId mountPath } } }
        }
      }
    ' "$(jq -n --arg id "$src_id" '{id:$id}')" 2>/dev/null \
      | jq -r '.data.environment.volumeInstances.edges[]?.node
               | "\(.serviceId)\t\(.mountPath)"' || true
  )"
  vols_dst="$(
    railway_gql '
      query($id: String!) {
        environment(id: $id) {
          volumeInstances { edges { node { serviceId mountPath } } }
        }
      }
    ' "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null \
      | jq -r '.data.environment.volumeInstances.edges[]?.node
               | "\(.serviceId)\t\(.mountPath)"' || true
  )"
  if [[ -n "$vols_src" ]]; then
    local vsid vmp
    while IFS=$'\t' read -r vsid vmp; do
      [[ -n "$vsid" && -n "$vmp" ]] || continue
      case "$vols_dst" in
        *"$vsid"*) continue ;;  # service already has a volume here
      esac
      if railway_gql '
          mutation($input: VolumeCreateInput!) {
            volumeCreate(input: $input) { id }
          }
        ' "$(jq -n --arg e "$env_id" --arg p "$project_id" --arg s "$vsid" --arg m "$vmp" \
             '{input:{environmentId:$e, projectId:$p, serviceId:$s, mountPath:$m}}')" \
           >/dev/null 2>&1; then
        log "environment ${env_id}: created volume ${vmp} for service ${vsid}"
      else
        log "warning: environment ${env_id}: could not create volume ${vmp} for service ${vsid}"
      fi
    done <<< "$vols_src"
  fi

  # -- support services: roll anything not healthy ----------------------
  if ! raw="$(
    railway_gql "$SERVICE_INSTANCES_QUERY" \
      "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null
  )"; then
    log "warning: ${env_id}: serviceInstances query failed — skipping support-service deploys"
    return 0
  fi
  local pending
  pending="$(echo "$raw" | jq -r --arg managed "$MANAGED_SERVICE_IDS" '
    .data.environment.serviceInstances.edges[].node
    | select((.latestDeployment.status // "none")
             | IN("none", "FAILED", "CRASHED", "REMOVED"))
    | . as $node
    | select(($managed | contains(" " + $node.serviceId + " ")) | not)
    | "\($node.serviceId)\t\($node.serviceName)"')"
  [[ -n "$pending" ]] || return 0

  local -a booted=()
  local sid sname
  while IFS=$'\t' read -r sid sname; do
    [[ -n "$sid" ]] || continue
    if railway_gql '
        mutation($environmentId: String!, $serviceId: String!) {
          serviceInstanceDeployV2(environmentId: $environmentId, serviceId: $serviceId)
        }
      ' "$(jq -n --arg e "$env_id" --arg s "$sid" \
           '{environmentId:$e, serviceId:$s}')" >/dev/null; then
      log "environment ${env_id}: rolled support service ${sname:-$sid}"
      booted+=("$sid")
    else
      log "warning: environment ${env_id}: could not roll support service ${sname:-$sid}"
    fi
  done <<< "$pending"
  (( ${#booted[@]} > 0 )) || return 0

  # Give the deploys a head start before the bind rolls the apps — api
  # retries its database connect, but a ready postgres avoids a needless
  # crash-loop window. Non-fatal on timeout.
  local deadline=$((SECONDS + BOOT_TIMEOUT)) settling st
  while (( SECONDS < deadline )); do
    if raw="$(
      railway_gql "$SERVICE_INSTANCES_QUERY" \
        "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null
    )"; then
      settling=0
      for sid in "${booted[@]}"; do
        st="$(echo "$raw" | jq -r --arg s "$sid" '
          [.data.environment.serviceInstances.edges[].node
           | select(.serviceId == $s)
           | .latestDeployment.status // "none"][0] // "none"')"
        case "$st" in
          QUEUED | INITIALIZING | BUILDING | DEPLOYING | none)
            settling=$((settling + 1)) ;;
        esac
      done
      if (( settling == 0 )); then
        log "environment ${env_id}: support service deploy(s) settled"
        return 0
      fi
      log "environment ${env_id}: waiting on ${settling} support deploy(s) (sleep 10s)"
    else
      log "environment ${env_id}: serviceInstances query failed — retrying (sleep 10s)"
    fi
    sleep 10
  done
  log "warning: environment ${env_id}: support deploys still settling after ${BOOT_TIMEOUT}s — continuing"
  return 0
}

# Print the environment id after the idempotent support-service pass.
# Skipped for --dry-run (a find may succeed but nothing may mutate).
# The pass is best-effort — a reconciliation failure must never swallow the
# id: a broken env produces a visible red Deployment on the PR, while an
# empty id is a silent skip no one sees.
emit_env_id() {
  local env_id="$1" project_id="$2"
  if [[ "$DRY_RUN" != "1" ]]; then
    ensure_support_services "$env_id" "$project_id" \
      || log "warning: support-service reconciliation failed for ${env_id} — emitting the id anyway"
  fi
  printf '%s' "$env_id"
}

usage() {
  sed -n '2,35p' "$0"
}

main() {
  need_jq
  local pr="" project_id=""
  local arg
  for arg in "$@"; do
    case "$arg" in
      --dry-run) DRY_RUN=1 ;;
      -h | --help)
        usage >&2
        exit 0
        ;;
      *)
        if [[ -z "$pr" ]]; then
          pr="$arg"
        elif [[ -z "$project_id" ]]; then
          project_id="$arg"
        else
          die "unexpected argument: ${arg}"
        fi
        ;;
    esac
  done
  [[ -n "$pr" ]] || die "usage: railway-pr-env-ensure.sh <pr-number> [project-id] [--dry-run]"
  [[ "$pr" =~ ^[0-9]+$ ]] || die "not a PR number: ${pr}"
  project_id="${project_id:-${RAILWAY_PROJECT_ID:?set RAILWAY_PROJECT_ID or pass <project-id>}}"
  local env_name="oxidean-pr-${pr}"

  # Enumeration AND creation both need an account/workspace credential —
  # env-scoped project tokens can do neither, so RAILWAY_TOKEN is dropped
  # here entirely (same posture as railway-pr-env-delete.sh).
  local have_credential=1
  if [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
    have_credential=0
  fi
  if [[ "$have_credential" == "0" && "$DRY_RUN" != "1" ]]; then
    log "No account Railway credential — cannot create the PR #${pr} environment. Skipping (images are still published)."
    exit 0
  fi
  unset RAILWAY_TOKEN

  # Railway's own provisioning still wins when it works: poll briefly so a
  # Railway-created env (matched via meta.prNumber) is used untouched.
  # Dry-run does a single lookup — enough to say whether a create is needed.
  local env_id=""
  if [[ "$have_credential" == "1" ]]; then
    if [[ "$DRY_RUN" == "1" ]]; then
      env_id="$(railway_find_pr_environment_id "$project_id" "$pr" 2>/dev/null || true)"
    else
      env_id="$(find_pr_environment "$project_id" "$pr" "$FIND_TIMEOUT")"
    fi
  fi
  if [[ -n "$env_id" ]]; then
    log "found environment ${env_id} for PR #${pr}"
    emit_env_id "$env_id" "$project_id"
    return 0
  fi

  # Fork source: the persistent preview environment — the same seed
  # Railway's own PR environments clone (its :main image + variables).
  local source_id="${RAILWAY_PR_ENV_SOURCE_ID:-}"
  if [[ -z "$source_id" && "$have_credential" == "1" ]]; then
    source_id="$(
      railway_resolve_environment_id "$project_id" "$SOURCE_ENVIRONMENT_NAME" 2>/dev/null || true
    )"
  fi
  if [[ -z "$source_id" ]]; then
    source_id="$PREVIEW_ENVIRONMENT_ID"
  fi

  local input
  input="$(jq -n \
    --arg projectId "$project_id" \
    --arg name "$env_name" \
    --arg sourceEnvironmentId "$source_id" \
    '{input:{
      projectId:$projectId,
      name:$name,
      sourceEnvironmentId:$sourceEnvironmentId,
      ephemeral:true,
      skipInitialDeploys:true,
      applyChangesInBackground:true
    }}')"

  if [[ "$DRY_RUN" == "1" ]]; then
    log "dry-run: no ${env_name} environment found — would run:"
    log "  mutation environmentCreate(input:) $(echo "$input" | jq -c .)"
    log "  then poll environment.serviceInstances for api/web/runner/gateway"
    exit 0
  fi

  log "no Railway-provisioned environment for PR #${pr} — creating ${env_name} (ephemeral fork of ${SOURCE_ENVIRONMENT_NAME} ${source_id})"
  local raw
  if ! raw="$(
    railway_gql '
      mutation($input: EnvironmentCreateInput!) {
        environmentCreate(input: $input) { id name }
      }
    ' "$input"
  )"; then
    # A concurrent push may have won the create (duplicate name) — re-find
    # instead of failing; any winner's env is as good as one we made.
    log "environmentCreate failed — re-checking for a concurrently created ${env_name}"
    env_id="$(find_pr_environment "$project_id" "$pr" 60)"
    if [[ -n "$env_id" ]]; then
      log "found environment ${env_id} for PR #${pr} after the create race"
      emit_env_id "$env_id" "$project_id"
      return 0
    fi
    die "environmentCreate failed and no ${env_name} environment appeared"
  fi
  env_id="$(echo "$raw" | jq -r '.data.environmentCreate.id // empty')"
  [[ -n "$env_id" && "$env_id" != "null" ]] || die "environmentCreate returned no id: ${raw}"
  log "created environment ${env_id} (${env_name})"

  if ! wait_for_service_instances "$env_id" "$FORK_TIMEOUT"; then
    # Still return the id — the instances may simply be slow, and a failed
    # bind reports a red Deployment on the PR (visible) instead of a silent
    # skip. A later push retries the whole flow either way.
    log "warning: ${env_name} service instances did not all materialize within ${FORK_TIMEOUT}s — returning the id anyway"
  fi
  emit_env_id "$env_id" "$project_id"
}

main "$@"
