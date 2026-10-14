#!/usr/bin/env bash
# Create / update a GitHub Deployment for an image deploy, so redeploys are
# visible on the PR/commit under the environment Railway already posts to
# ("Oxidean / oxidean-pr-<n>", "Oxidean / staging", "Oxidean / production").
# Railway only posts Deployments for git-triggered builds — API-triggered
# image repins (serviceInstanceUpdate + serviceInstanceDeployV2) would
# otherwise be invisible on the PR.
#
# Usage:
#   GH_TOKEN=… scripts/github-deployment.sh create --ref <sha> \
#     --environment "<env name>" [--url <env_url>] [--description <text>]
#     [--transient] [--production]
#       → prints the deployment id (empty on failure — caller may continue)
#       --transient marks an ephemeral env (PR previews); --production marks
#       the deployment as production for GitHub's environment tracking
#   GH_TOKEN=… scripts/github-deployment.sh status <id> <state> \
#     [--url <env_url>] [--log-url <url>] [--description <text>]
#       → <state>: queued|in_progress|pending|success|failure|error|inactive
#
# Auth: GH_TOKEN (GITHUB_TOKEN needs deployments: write on the job). All
# failures are non-fatal — this is a reporting surface, never a deploy gate.

set -uo pipefail

die() { echo "error: $*" >&2; exit 1; }

repo="${GITHUB_REPOSITORY:?set GITHUB_REPOSITORY}"
run_url="https://github.com/${repo}/actions/runs/${GITHUB_RUN_ID:-}"

cmd="${1:-}"; shift || true

case "$cmd" in
  create)
    ref="" environment="" url="" desc="" transient=false production=false
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --ref) ref="$2"; shift ;;
        --environment) environment="$2"; shift ;;
        --url) url="$2"; shift ;;
        --description) desc="$2"; shift ;;
        --transient) transient=true ;;
        --production) production=true ;;
        *) die "unexpected argument: $1" ;;
      esac
      shift
    done
    [[ -n "$ref" && -n "$environment" ]] || die "create needs --ref and --environment"
    # required_contexts must be explicit []: the default would gate deployment
    # creation on the in-flight workflow's own checks (a deadlock).
    id="$(jq -n --arg ref "$ref" --arg env "$environment" --arg desc "${desc:-image deploy}" \
      --argjson transient "$transient" --argjson production "$production" \
      '{ref:$ref, environment:$env, auto_merge:false, transient_environment:$transient,
        production_environment:$production, required_contexts:[], description:$desc}' \
      | gh api -X POST "repos/${repo}/deployments" --input - --jq '.id' 2>/dev/null || true)"
    printf '%s' "$id"
    ;;
  status)
    id="${1:?status needs a deployment id}"; state="${2:?status needs a state}"; shift 2
    url="" log_url="" desc=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --url) url="$2"; shift ;;
        --log-url) log_url="$2"; shift ;;
        --description) desc="$2"; shift ;;
        *) die "unexpected argument: $1" ;;
      esac
      shift
    done
    [[ -n "$id" ]] || exit 0  # create failed earlier — nothing to update
    jq -n --arg state "$state" --arg url "$url" --arg log "${log_url:-$run_url}" --arg desc "$desc" \
      '{state:$state, auto_inactive:true}
       + (if $url != "" then {environment_url:$url} else {} end)
       + (if $log != "" then {log_url:$log} else {} end)
       + (if $desc != "" then {description:$desc} else {} end)' \
      | gh api -X POST "repos/${repo}/deployments/${id}/statuses" --input - >/dev/null 2>&1 || true
    ;;
  *) die "usage: github-deployment.sh create|status …" ;;
esac
