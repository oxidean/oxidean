#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL/gh-api literals stay single-quoted; $vars bind via jq --arg
# Assert + heal service-source drift on Oxidean Cloud (issue #206).
#
# Every deployable service (api/web/runner/gateway) is image-sourced by
# .railway/railway.ts — images.yml pins each environment's instances to
# GHCR tags. A serviceInstance left repo-connected still fires Railway
# deployments on the connected branch: PR environments fork preview's
# instances wholesale, so a drifted seed makes every fresh fork's initial
# deploy evaluate the branch diff — the instance's watchPatterns cancel
# the non-matching legs and Railway posts `Oxidean - <svc>` commit
# statuses in state `error` ("Deployment cancelled") on the pushed
# commit. A cancelled deploy is not a failure — but it reads as one on
# every main commit until the repo connection is gone.
#
# Surfaces:
#   check (default)   list repo-connected managed instances across the
#                     project's environments; exit 1 on any drift
#   --repair          repin each drifted instance to
#                     <image-base>/oxidean-<svc>:<channel> — the
#                     environment's IaC channel (production→:production,
#                     staging→:staging, everything else→:main). Config
#                     only — nothing rolls until the next deploy/bind.
#   --repair-statuses post `success` over `Oxidean - <svc>` commit
#                     statuses whose latest entry is error/"cancelled" —
#                     the no-change cancels that must resolve as pass.
#                     Scans the last --status-limit commits on the default
#                     branch (or a single commit via --status-sha).
#                     Only ever touches error+cancelled entries — real
#                     failure/pending/success statuses are left alone.
#
# Usage:
#   RAILWAY_API_TOKEN=*** GH_TOKEN=… GH_REPO=owner/repo \
#     scripts/railway-image-source-check.sh [--repair] [--repair-statuses] \
#       [--status-limit N | --status-sha <sha>] [--environment <name>] [--dry-run]
#
# Credentials:
#   RAILWAY_API_TOKEN  account/workspace token (environment enumeration
#                      needs one — env-scoped project tokens cannot list
#                      environments), or a `railway login` config. Missing
#                      → the Railway surface warns and skips.
#   GH_TOKEN           token able to read+write commit statuses (the
#                      sweep/deploy jobs grant statuses: write). Missing →
#                      gh falls back to its own auth; scope-denied calls
#                      degrade to warnings.
#   GH_REPO            owner/repo (default: $GITHUB_REPOSITORY)
#
# Env overrides: RAILWAY_PROJECT_ID, RAILWAY_SERVICE_{API,WEB,RUNNER,
# GATEWAY}_ID, OXIDEAN_IMAGE_BASE (default ghcr.io/oxidean),
# OXIDEAN_STATUS_CONTEXT_PREFIX (default "Oxidean - "),
# OXIDEAN_STATUS_LIMIT (default 30), OXIDEAN_DEFAULT_BRANCH (default main).

set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd -P)"
# shellcheck source=scripts/railway-gql.sh
source "${SCRIPT_DIR}/railway-gql.sh"
# shellcheck source=scripts/gh-api-helpers.sh
source "${SCRIPT_DIR}/gh-api-helpers.sh"

PROJECT_ID="${RAILWAY_PROJECT_ID:-cec803d0-289b-4078-80cf-7291c16fc5ec}"
IMAGE_BASE="${OXIDEAN_IMAGE_BASE:-ghcr.io/oxidean}"
REPO="${GH_REPO:-${GITHUB_REPOSITORY:-}}"
CONTEXT_PREFIX="${OXIDEAN_STATUS_CONTEXT_PREFIX:-Oxidean - }"
STATUS_LIMIT="${OXIDEAN_STATUS_LIMIT:-30}"
DEFAULT_BRANCH="${OXIDEAN_DEFAULT_BRANCH:-main}"

REPAIR=0 REPAIR_STATUSES=0 DRY_RUN=0
STATUS_SHA="" ENV_FILTER=""

service_pairs() {
  printf '%s %s\n' api "${RAILWAY_SERVICE_API_ID:-19bf0a47-2fdf-4130-b6b0-858de92cb858}"
  printf '%s %s\n' web "${RAILWAY_SERVICE_WEB_ID:-26b7e1b6-6cfa-4c7e-942a-bed8e5ce12a2}"
  printf '%s %s\n' runner "${RAILWAY_SERVICE_RUNNER_ID:-36f9b50a-738a-419a-9171-49f56c902e6a}"
  printf '%s %s\n' gateway "${RAILWAY_SERVICE_GATEWAY_ID:-864fa81c-6e5d-4939-b33b-85eb435365ac}"
}

# The IaC channel seed per environment (.railway/railway.ts).
channel_for_env() {
  case "$1" in
    production) printf 'production' ;;
    staging) printf 'staging' ;;
    *) printf 'main' ;;
  esac
}

ENVIRONMENTS_QUERY='
  query($id: String!) {
    project(id: $id) {
      environments { edges { node { id name isEphemeral } } }
    }
  }
'

SOURCES_QUERY='
  query($id: String!) {
    environment(id: $id) {
      serviceInstances {
        edges { node { serviceId serviceName source { image repo } } }
      }
    }
  }
'

# Repin one instance to the environment's channel image — the same
# serviceInstanceUpdate the bind/deploy scripts run (config stage only).
repin_instance() {
  local env_id="$1" env_name="$2" name="$3" sid="$4" ref="$5"
  if [[ "$DRY_RUN" == "1" ]]; then
    echo "  dry-run: ${env_name}/${name} would pin source.image=${ref}"
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
  )" || return 1
  ok="$(echo "$raw" | jq -r '.data.serviceInstanceUpdate // empty')"
  [[ "$ok" == "true" ]]
}

# Surface 1: repo-connected managed serviceInstances. Returns non-zero on
# unrepaired drift or an enumeration failure (degraded cred path returns
# 0 — the surface is simply invisible this run, same posture as the sweep).
check_sources() {
  if [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
    echo "no account Railway credential — service-source surface invisible this run"
    return 0
  fi
  # Env enumeration needs the account/workspace token; a project token
  # scoped to one env would 401 the project query.
  unset RAILWAY_TOKEN
  local raw envs
  if ! raw="$(railway_gql "$ENVIRONMENTS_QUERY" \
      "$(jq -n --arg id "$PROJECT_ID" '{id:$id}')")"; then
    echo "::error::could not enumerate Railway environments" >&2
    return 1
  fi
  envs="$(echo "$raw" | jq -r '.data.project.environments.edges[]?.node | "\(.id)\t\(.name)"')"

  local drifted=0 failed=0 env_id env_name channel inst sid name repo image
  while IFS=$'\t' read -r env_id env_name; do
    [[ -n "$env_id" ]] || continue
    if [[ -n "$ENV_FILTER" && "$env_name" != "$ENV_FILTER" ]]; then
      continue
    fi
    channel="$(channel_for_env "$env_name")"
    if ! inst="$(railway_gql "$SOURCES_QUERY" \
        "$(jq -n --arg id "$env_id" '{id:$id}')")"; then
      echo "warn: serviceInstances query failed for ${env_name} — skipping it"
      failed=1
      continue
    fi
    while read -r name sid; do
      [[ -n "$name" ]] || continue
      repo="$(echo "$inst" | jq -r --arg sid "$sid" '
        [.data.environment.serviceInstances.edges[]?.node | select(.serviceId == $sid)
         | .source.repo // ""][0] // ""')"
      image="$(echo "$inst" | jq -r --arg sid "$sid" '
        [.data.environment.serviceInstances.edges[]?.node | select(.serviceId == $sid)
         | .source.image // ""][0] // ""')"
      # Missing instance: never forked into this env — not drift.
      [[ -z "$repo$image" ]] && continue
      # Image-sourced — desired state.
      [[ -z "$repo" ]] && continue
      drifted=$((drifted + 1))
      if [[ "$REPAIR" == "1" ]]; then
        if repin_instance "$env_id" "$env_name" "$name" "$sid" "${IMAGE_BASE}/oxidean-${name}:${channel}"; then
          [[ "$DRY_RUN" == "1" ]] || echo "healed ${env_name}/${name}: repo source → ${IMAGE_BASE}/oxidean-${name}:${channel}"
        else
          echo "::error::${env_name}/${name}: serviceInstanceUpdate failed" >&2
          failed=1
        fi
      else
        echo "drift ${env_name}/${name}: repo-connected to ${repo} (channel seed would be :${channel})"
      fi
    done < <(service_pairs)
  done <<< "$envs"

  if (( drifted == 0 )); then
    echo "all managed service instances are image-sourced"
  elif [[ "$REPAIR" != "1" ]]; then
    echo "::error::${drifted} repo-connected service instance(s) — rerun with --repair or repin to the env channel image" >&2
  fi
  (( failed == 0 && (drifted == 0 || REPAIR == 1) ))
}

# Surface 2: cancelled deployment statuses. The combined status endpoint
# returns newest first, so the first entry seen per context is the live
# one — only error entries whose description reads as a cancellation get
# a success posted over them.
repair_statuses() {
  command -v gh >/dev/null 2>&1 || { echo "warn: gh not on PATH — status repair skipped"; return 0; }
  [[ -n "$REPO" ]] || { echo "warn: GH_REPO unset — status repair skipped"; return 0; }

  local -a shas=()
  if [[ -n "$STATUS_SHA" ]]; then
    shas=("$STATUS_SHA")
  else
    local out s
    if out="$(gh api "repos/${REPO}/commits?sha=${DEFAULT_BRANCH}&per_page=${STATUS_LIMIT}" \
        --jq '.[].sha' 2>&1)"; then
      while IFS= read -r s; do [[ -n "$s" ]] && shas+=("$s"); done <<< "$out"
    elif is_scope_denial "$out"; then
      echo "warn: could not list ${DEFAULT_BRANCH} commits (scope wall) — status repair skipped"
      return 0
    else
      # Reporting surface, never a gate — warn and move on.
      echo "warn: could not list ${DEFAULT_BRANCH} commits — ${out}" >&2
      return 0
    fi
  fi

  local repaired=0 sha entries ctx st desc url
  local -A seen=()
  for sha in "${shas[@]}"; do
    local raw
    if ! raw="$(gh api "repos/${REPO}/commits/${sha}/status?per_page=100" 2>/dev/null)"; then
      echo "warn: status read failed for ${sha:0:7} — skipping"
      continue
    fi
    seen=()
    entries="$(echo "$raw" | jq -r --arg p "$CONTEXT_PREFIX" '
      .statuses[]?
      | select(.context | startswith($p))
      | [.context, .state, (.description // ""), (.target_url // "")] | @tsv')"
    while IFS=$'\t' read -r ctx st desc url; do
      [[ -n "$ctx" ]] || continue
      [[ -z "${seen[$ctx]:-}" ]] || continue  # newest entry per context wins
      seen[$ctx]=1
      [[ "$st" == "error" ]] || continue
      case "$desc" in *[Cc]ancel*) ;; *) continue ;; esac
      if [[ "$DRY_RUN" == "1" ]]; then
        echo "  dry-run: ${sha:0:7} would post success over '${ctx}' (was: ${desc})"
        repaired=$((repaired + 1))
        continue
      fi
      if jq -n --arg state "success" --arg context "$ctx" --arg url "$url" \
          --arg desc "deploy cancelled by the Railway diff gate — no-op, image deploys own this surface" \
          '{state:$state, context:$context, description:$desc}
           + (if $url == "" then {} else {target_url:$url} end)' \
        | gh api -X POST "repos/${REPO}/statuses/${sha}" --input - >/dev/null 2>&1; then
        echo "repaired ${sha:0:7} '${ctx}': error(cancelled) → success"
        repaired=$((repaired + 1))
      else
        echo "warn: could not post status for ${sha:0:7} '${ctx}'"
      fi
    done <<< "$entries"
  done
  echo "status repair: ${repaired} cancelled-deployment status(es) resolved as success"
  # Always 0 — a cosmetic repair must never gate a deploy or a sweep.
  return 0
}

usage() {
  sed -n '3,51p' "$0"
}

main() {
  need_jq
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repair) REPAIR=1 ;;
      --repair-statuses) REPAIR_STATUSES=1 ;;
      --status-limit) STATUS_LIMIT="$2"; shift ;;
      --status-sha) STATUS_SHA="$2"; REPAIR_STATUSES=1; shift ;;
      --environment|--env) ENV_FILTER="$2"; shift ;;
      --dry-run) DRY_RUN=1 ;;
      -h | --help)
        usage
        exit 0
        ;;
      *) die "unexpected argument: $1" ;;
    esac
    shift
  done

  local rc=0
  check_sources || rc=1
  if [[ "$REPAIR_STATUSES" == "1" ]]; then
    repair_statuses || rc=1
  fi
  return "$rc"
}

main "$@"
