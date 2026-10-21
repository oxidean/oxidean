#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Delete oxidean-pr-<n> Railway environments whose PR is no longer open.
#
# Why this exists: the pull_request:closed cleanup run is routinely
# cancelled before any job starts — on merge GitHub tears down
# refs/pull/<n>/merge and the queued run goes with it — so the
# on-close cleanup-pr job cannot be relied on. Each leaked env keeps its
# volumes; past ~6 leaks the project hits the 20-volume cap and every
# subsequent PR env bind dies inside volumeCreate.
#
# For every ephemeral env named oxidean-pr-<n> the PR's state is resolved
# through `gh`; MERGED/CLOSED (or any non-open state) envs are deleted via
# railway-pr-env-delete.sh. A PR lookup that fails is skipped — envs are
# never deleted on ambiguity. The GitHub Environment and GHCR pr-* tags
# for closed PRs remain the closed-run cleanup's job; the sweep only owns
# the capacity-critical Railway side.
#
# Environment enumeration needs an account/workspace credential —
# RAILWAY_API_TOKEN or a `railway login` config. With none, this is a
# no-op exit 0 (same posture as railway-pr-env-delete.sh).
#
# Usage:
#   RAILWAY_API_TOKEN=… GH_TOKEN=… GH_REPO=owner/repo \
#     scripts/railway-pr-env-sweep.sh
#
#   GH_TOKEN/GH_REPO fall back to the ambient gh auth and the repo in the
#   current directory when unset.

set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/railway-gql.sh
source scripts/railway-gql.sh

need_jq
command -v gh >/dev/null 2>&1 || die "gh is required"

PROJECT_ID="${RAILWAY_PROJECT_ID:?set RAILWAY_PROJECT_ID}"

if [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
  echo "No account Railway credential — skipping PR env sweep."
  exit 0
fi
unset RAILWAY_TOKEN

raw="$(railway_gql '
  query($id: String!) {
    project(id: $id) {
      environments { edges { node { id name isEphemeral } } }
    }
  }
' "$(jq -n --arg id "$PROJECT_ID" '{id:$id}')")"

pairs="$(echo "$raw" | jq -r '
  .data.project.environments.edges[]?.node
  | select(.isEphemeral == true)
  | select(.name | test("^oxidean-pr-[0-9]+$"))
  | "\(.name | sub("^oxidean-pr-";""))\t\(.id)"')"

if [[ -z "$pairs" ]]; then
  echo "no oxidean-pr-* environments — nothing to sweep."
  exit 0
fi

repo_args=()
if [[ -n "${GH_REPO:-}" ]]; then
  repo_args=(--repo "$GH_REPO")
fi

swept=0 skipped=0
while IFS=$'\t' read -r pr env_id; do
  [[ -n "$pr" && -n "$env_id" ]] || continue
  state="$(gh pr view "$pr" "${repo_args[@]}" --json state --jq '.state' 2>/dev/null || true)"
  case "$state" in
    OPEN)
      skipped=$((skipped + 1)) ;;
    MERGED | CLOSED)
      echo "PR #${pr} is ${state} — deleting environment ${env_id}"
      scripts/railway-pr-env-delete.sh "$pr"
      swept=$((swept + 1)) ;;
    *)
      echo "warn: could not resolve PR #${pr} state (got '${state:-empty}') — leaving ${env_id} in place"
      skipped=$((skipped + 1)) ;;
  esac
done <<< "$pairs"

echo "sweep done: ${swept} environment(s) deleted, ${skipped} kept (open or unresolved)"
