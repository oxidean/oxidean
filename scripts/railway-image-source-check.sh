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
#                     Every repair is verified against the Railway API
#                     before posting: the status's target_url links its
#                     deployment, and a status resolves only when the
#                     deployment provably had nothing to do — ephemeral
#                     (or already-deleted) PR environments, or a commit
#                     diff touching none of the deployment's snapshot
#                     watchPatterns. User-cancelled real deploys and
#                     anything unverifiable stay as-is.
#   --no-source-check skip the check_sources surface (status-repair-only
#                     invocations, e.g. deploy-staging's --status-sha)
#
# Usage:
#   RAILWAY_API_TOKEN=*** GH_TOKEN=… GH_REPO=owner/repo \
#     scripts/railway-image-source-check.sh [--repair] [--repair-statuses] \
#       [--status-limit N | --status-sha <sha>] [--environment <name>] \
#       [--no-source-check] [--dry-run]
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

REPAIR=0 REPAIR_STATUSES=0 DRY_RUN=0 SOURCE_CHECK=1
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

DEPLOYMENT_STATUS_QUERY='
  query($id: String!) {
    deployment(id: $id) {
      status
      environment { name isEphemeral }
      meta
    }
  }
'

ENVIRONMENT_QUERY='
  query($id: String!) {
    environment(id: $id) { name isEphemeral }
  }
'

# Extract a query param from a Railway dashboard URL — the cancelled
# status's target_url carries its deployment (?id=<uuid>) and environment
# (&environmentId=<uuid>), the only link from a status back to its deploy.
url_param() {
  printf '%s' "$1" | sed -n "s/.*[?&]$2=\([^&#]*\).*/\1/p"
}

# gitignore-style glob → ERE, repo-root anchored (`**/` → zero or more
# directories, `**` → .*, `*` → [^/]*, `?` → [^/]). Slash-less patterns
# match the basename at any depth — over-matching only ever leaves a
# status alone, the safe direction.
watch_pattern_match() {
  local file="$1" pat="$2" re="" i=0 ch
  pat="${pat#/}"
  local anywhere=0
  [[ "$pat" != */* ]] && anywhere=1
  while (( i < ${#pat} )); do
    ch="${pat:i:1}"
    case "$ch" in
      '*')
        if [[ "${pat:i:2}" == "**" ]]; then
          if [[ "${pat:i+2:1}" == "/" ]]; then re+='(.*/)?'; i=$((i + 3));
          else re+='.*'; i=$((i + 2)); fi
        else re+='[^/]*'; i=$((i + 1)); fi ;;
      '?') re+='[^/]'; i=$((i + 1)) ;;
      \\) re+='\\\\'; i=$((i + 1)) ;;
      .|^|'$'|+|\(|\)|\[|\]|\{|\}|\|) re+="\\${ch}"; i=$((i + 1)) ;;
      *) re+="$ch"; i=$((i + 1)) ;;
    esac
  done
  if (( anywhere )); then
    [[ "$file" =~ (^|/)${re}$ ]]
  else
    [[ "$file" =~ ^${re}$ ]]
  fi
}

# Is the cancelled status's linked deployment provably a no-change cancel?
# Verified paths (return 0):
#   - the deployment lived in an ephemeral PR environment — fork-seeded
#     instances fire initial deploys that the diff gate cancels; those
#     statuses are artifacts on the main commit (the env's real signal is
#     its own GitHub Deployment record), or
#   - the deployment/environment is already deleted — only ephemeral
#     environments get torn down, so this is the same artifact class, or
#   - a persistent environment whose commit diff touches none of the
#     deployment's snapshot watchPatterns — the same check the gate ran.
# Everything else (missing links, lookup failures, matching patterns,
# non-terminal deployment states) returns 1: leave the status alone.
is_noop_cancel() {
  local sha="$1" dep_id="$2" env_id="$3"
  local dep dep_status env_name env_eph commit_hash env_raw
  if ! dep="$(railway_gql "$DEPLOYMENT_STATUS_QUERY" \
      "$(jq -n --arg id "$dep_id" '{id:$id}')" 2>/dev/null)"; then
    echo "skip ${dep_id:0:8}…: deployment lookup failed — cannot verify the cancel"
    return 1
  fi
  if [[ "$(echo "$dep" | jq -r '.data.deployment // "null"')" == "null" ]]; then
    # Deployment gone → only an environment teardown removes deployments,
    # and only ephemeral PR environments get torn down — but that needs
    # the environment link to confirm. No env id → unverifiable.
    if [[ -z "$env_id" ]]; then
      echo "skip ${dep_id:0:8}…: deployment gone, no environment link — cannot verify"
      return 1
    fi
    if ! env_raw="$(railway_gql "$ENVIRONMENT_QUERY" \
        "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null)"; then
      echo "skip ${dep_id:0:8}…: deployment gone, environment lookup failed — cannot verify"
      return 1
    fi
    # Persistent env still standing → unverifiable. A null environment
    # (deleted) or isEphemeral resolve as the ephemeral-artifact class —
    # jq's // also replaces false, so test null-ness explicitly.
    if [[ "$(echo "$env_raw" | jq -r 'if .data.environment == null then "gone" elif .data.environment.isEphemeral == true then "true" else "false" end')" == "false" ]]; then
      echo "skip ${dep_id:0:8}…: deployment gone but environment still exists — cannot verify"
      return 1
    fi
    echo "verified ${dep_id:0:8}…: deployment deleted with its ephemeral environment"
    return 0
  fi
  commit_hash="$(echo "$dep" | jq -r '.data.deployment.meta.commitHash // ""')"
  if [[ "$commit_hash" != "$sha" ]]; then
    echo "skip ${dep_id:0:8}…: deployment commit ${commit_hash:0:7} != status sha ${sha:0:7}"
    return 1
  fi
  dep_status="$(echo "$dep" | jq -r '.data.deployment.status // ""')"
  case "$dep_status" in
    REMOVED | REMOVING | SKIPPED) ;;
    *)
      echo "skip ${dep_id:0:8}…: deployment is ${dep_status} — not a cancelled no-op"
      return 1 ;;
  esac
  env_name="$(echo "$dep" | jq -r '.data.deployment.environment.name // ""')"
  env_eph="$(echo "$dep" | jq -r '.data.deployment.environment.isEphemeral // false')"
  if [[ -z "$env_name" && -n "$env_id" ]] \
    && env_raw="$(railway_gql "$ENVIRONMENT_QUERY" \
      "$(jq -n --arg id "$env_id" '{id:$id}')" 2>/dev/null)"; then
    env_name="$(echo "$env_raw" | jq -r '.data.environment.name // ""')"
    env_eph="$(echo "$env_raw" | jq -r '.data.environment.isEphemeral // false')"
    if [[ -z "$env_name" ]]; then
      # Environment deleted — same ephemeral-artifact class.
      echo "verified ${dep_id:0:8}…: environment deleted (ephemeral PR env)"
      return 0
    fi
  fi
  if [[ "$env_eph" == "true" ]]; then
    echo "verified ${dep_id:0:8}…: ephemeral environment '${env_name}' — fork-artifact cancel"
    return 0
  fi
  # Persistent environment: re-run the gate — the cancel was a no-change
  # skip only when the commit's diff touched none of the deployment's
  # snapshot watchPatterns.
  local patterns p f hit=""
  patterns="$(echo "$dep" | jq -r '.data.deployment.meta.serviceManifest.build.watchPatterns[]? // empty')"
  if [[ -z "$patterns" ]]; then
    echo "skip ${dep_id:0:8}… (${env_name:-unknown env}): no watchPatterns in the deployment snapshot — cancel was not the diff gate"
    return 1
  fi
  while IFS= read -r p; do
    if [[ "$p" == '!'* ]]; then
      echo "skip ${dep_id:0:8}… (${env_name:-unknown env}): negated watchPatterns are unsupported — cannot verify"
      return 1
    fi
  done <<< "$patterns"
  local files
  if ! files="$(gh api --paginate "repos/${REPO}/commits/${sha}" --jq '.files[]?.filename' 2>/dev/null)"; then
    echo "skip ${dep_id:0:8}… (${env_name:-unknown env}): could not list commit files — cannot verify"
    return 1
  fi
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    while IFS= read -r p; do
      [[ -z "$p" ]] && continue
      if watch_pattern_match "$f" "$p"; then
        hit="${f} ~ ${p}"
        break 2
      fi
    done <<< "$patterns"
  done <<< "$files"
  if [[ -n "$hit" ]]; then
    echo "skip ${dep_id:0:8}… (${env_name:-unknown env}): commit touched watched path (${hit}) — not a no-change cancel"
    return 1
  fi
  echo "verified ${dep_id:0:8}… (${env_name:-unknown env}): diff touched none of the deployment's watchPatterns"
  return 0
}

# Surface 2: cancelled deployment statuses. The combined status endpoint
# returns newest first, so the first entry seen per context is the live
# one — only error entries whose description reads as a cancellation and
# whose linked Railway deployment verifies as a no-change cancel get a
# success posted over them.
repair_statuses() {
  command -v gh >/dev/null 2>&1 || { echo "warn: gh not on PATH — status repair skipped"; return 0; }
  [[ -n "$REPO" ]] || { echo "warn: GH_REPO unset — status repair skipped"; return 0; }
  # Verification needs the Railway API — without a credential every status
  # is unverifiable, and unverifiable cancels stay untouched.
  if [[ -z "${RAILWAY_API_TOKEN:-}" && -z "${RAILWAY_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
    echo "no Railway credential — cannot verify cancelled statuses, repair skipped"
    return 0
  fi
  # Deployment/environment lookups must reach every environment — prefer
  # the account token; an env-scoped project token 401s outside its env.
  [[ -n "${RAILWAY_API_TOKEN:-}" ]] && unset RAILWAY_TOKEN

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

  local repaired=0 sha entries ctx st desc url dep_id
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
      # Never trust the description alone — the linked deployment must
      # verify as a no-change cancel before anything resolves.
      dep_id="$(url_param "$url" id)"
      if [[ -z "$dep_id" ]]; then
        echo "skip ${sha:0:7} '${ctx}': no deployment link — cannot verify the cancel"
        continue
      fi
      is_noop_cancel "$sha" "$dep_id" "$(url_param "$url" environmentId)" || continue
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
  sed -n '3,62p' "$0"
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
      --no-source-check) SOURCE_CHECK=0 ;;
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
  if [[ "$SOURCE_CHECK" == "1" ]]; then
    check_sources || rc=1
  fi
  if [[ "$REPAIR_STATUSES" == "1" ]]; then
    repair_statuses || rc=1
  fi
  return "$rc"
}

main "$@"
