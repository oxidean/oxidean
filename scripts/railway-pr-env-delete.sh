#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Delete the ephemeral Railway environment for a pull request
# (oxidean-pr-<n>), looked up via Environment.meta.prNumber.
#
# Deleting the environment removes every service instance + deployment in it.
# Missing env, or no credential, is a no-op exit 0 — PR cleanup is
# best-effort and must never fail the workflow.
#
# Environment enumeration needs an account/workspace credential, so this
# ignores RAILWAY_TOKEN (env-scoped project tokens cannot list environments).
#
# Usage:
#   RAILWAY_API_TOKEN=… scripts/railway-pr-env-delete.sh <pr-number>

set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/railway-gql.sh
source scripts/railway-gql.sh

PR="${1:?usage: railway-pr-env-delete.sh <pr-number>}"
[[ "$PR" =~ ^[0-9]+$ ]] || die "not a PR number: ${PR}"

PROJECT_ID="${RAILWAY_PROJECT_ID:?set RAILWAY_PROJECT_ID}"

if [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
  echo "No account Railway credential — skipping PR env delete."
  exit 0
fi

# Env enumeration requires the account/workspace token — env-scoped project
# tokens cannot list environments, and a PR env is never covered by one.
unset RAILWAY_TOKEN

env_id="$(railway_find_pr_environment_id "$PROJECT_ID" "$PR")"
if [[ -z "$env_id" ]]; then
  echo "No ephemeral environment for PR #${PR} — nothing to delete."
  exit 0
fi

echo "deleting PR #${PR} environment ${env_id}"
out="$(railway_gql '
  mutation($id: String!) { environmentDelete(id: $id) }
' "$(jq -n --arg id "$env_id" '{id:$id}')" | jq -r '.data.environmentDelete')"
[[ "$out" == "true" ]] || die "environmentDelete returned ${out}"
echo "done: environment ${env_id} deleted"
