#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL/gh-api literals stay single-quoted; $vars bind via jq --arg
# Tear down every per-PR surface for <pr-number>:
#
#   - Railway ephemeral environment oxidean-pr-<n> (via railway-pr-env-delete.sh)
#   - GitHub Environment "Oxidean / oxidean-pr-<n>" — materialised by the
#     deployment statuses the bind posts; needs an admin-scoped token,
#     GITHUB_TOKEN can never manage environments
#   - that environment's GitHub Deployments — marked inactive when the
#     environment itself cannot be deleted
#   - GHCR pr-<n> / pr-<n>-sha-* tags on the four service images
#
# Every leg is best-effort — failures warn and the run continues — but the
# script exits non-zero when any leg errored so a leak stays visible as a
# red job instead of a clean log nobody reads.
#
# Credentials:
#   RAILWAY_API_TOKEN    account/workspace token (Railway leg skipped when
#                        unset; env-scoped project tokens cannot enumerate)
#   RAILWAY_PROJECT_ID   project the PR env lives in (required for that leg)
#   GH_ADMIN_TOKEN       token able to delete environments + post deployment
#                        statuses (release GitHub App / admin PAT). Falls
#                        back to GH_TOKEN — which usually fails soft here.
#   GH_TOKEN             repo token for gh api — GHCR version deletes need
#                        packages:write on the job
#   GH_REPO              owner/repo (default: $GITHUB_REPOSITORY)
#
# Usage:
#   scripts/pr-env-teardown.sh <pr-number> [--dry-run]

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/railway-gql.sh
source scripts/railway-gql.sh

PR="" DRY_RUN="${OXIDEAN_TEARDOWN_DRY_RUN:-0}"
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    *)
      [[ -z "$PR" ]] || die "unexpected argument: ${arg}"
      PR="$arg" ;;
  esac
done
[[ -n "$PR" ]] || die "usage: pr-env-teardown.sh <pr-number> [--dry-run]"
[[ "$PR" =~ ^[0-9]+$ ]] || die "not a PR number: ${PR}"

REPO="${GH_REPO:-${GITHUB_REPOSITORY:-}}"
[[ -n "$REPO" ]] || die "set GH_REPO or GITHUB_REPOSITORY"
ORG="${REPO%%/*}"
ENV_LABEL="Oxidean / oxidean-pr-${PR}"
ENV_NAME_ENC="Oxidean%20%2F%20oxidean-pr-${PR}"

need_jq
command -v gh >/dev/null 2>&1 || die "gh is required"

status=0

# --- Railway environment ----------------------------------------------------
if [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
  echo "no account Railway credential — skipping Railway env delete"
elif [[ -z "${RAILWAY_PROJECT_ID:-}" ]]; then
  echo "warn: RAILWAY_PROJECT_ID unset — skipping Railway env delete"
  status=1
elif [[ "$DRY_RUN" == "1" ]]; then
  echo "dry-run: would run railway-pr-env-delete.sh ${PR}"
else
  # Lookup failures die loudly inside the script (auth outage ≠ no env).
  scripts/railway-pr-env-delete.sh "$PR" || status=1
fi

# --- GitHub environment -----------------------------------------------------
admin_token="${GH_ADMIN_TOKEN:-${GH_TOKEN:-}}"
if [[ -z "$admin_token" ]]; then
  echo "warn: no GitHub token — skipping environment + deployment cleanup"
  status=1
elif [[ "$DRY_RUN" == "1" ]]; then
  if GH_TOKEN="$admin_token" gh api "repos/${REPO}/environments/${ENV_NAME_ENC}" >/dev/null 2>&1; then
    echo "dry-run: would delete github environment '${ENV_LABEL}'"
  else
    echo "dry-run: github environment '${ENV_LABEL}' not present (or unreadable)"
  fi
elif GH_TOKEN="$admin_token" gh api -X DELETE "repos/${REPO}/environments/${ENV_NAME_ENC}" >/dev/null 2>&1; then
  echo "deleted github environment '${ENV_LABEL}'"
elif ! GH_TOKEN="$admin_token" gh api "repos/${REPO}/environments/${ENV_NAME_ENC}" >/dev/null 2>&1; then
  echo "github environment '${ENV_LABEL}' already absent"
else
  # Exists but the token can't delete it — at least stop the deployments
  # pointing at a dead preview URL.
  echo "warn: could not delete github environment '${ENV_LABEL}' (token lacks environment admin) — marking its deployments inactive"
  status=1
  dep_ids="$(GH_TOKEN="$admin_token" gh api "repos/${REPO}/deployments" \
    -f environment="$ENV_LABEL" -f per_page=100 --jq '.[].id' 2>/dev/null || true)"
  for dep in $dep_ids; do
    GH_TOKEN="$admin_token" gh api -X POST "repos/${REPO}/deployments/${dep}/statuses" \
      -f state=inactive -f description="PR #${PR} closed — environment retired" >/dev/null 2>&1 \
      || echo "warn: could not mark deployment ${dep} inactive"
  done
fi

# --- GHCR pr-<n> tags --------------------------------------------------------
if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "warn: GH_TOKEN unset — skipping GHCR pr-${PR} tag cleanup"
  status=1
elif [[ "$DRY_RUN" == "1" ]]; then
  echo "dry-run: would delete pr-${PR}*/pr-${PR}-sha-* tags on oxidean-{api,web,runner,gateway}"
else
  for svc in api web runner gateway; do
    pkg="oxidean-${svc}"
    ids="$(gh api "orgs/${ORG}/packages/container/${pkg}/versions?per_page=100" \
      --jq ".[] | select(.metadata.container.tags[]? | test(\"^pr-${PR}(-|\$)\")) | .id" 2>/dev/null || true)"
    for id in $ids; do
      if gh api -X DELETE "orgs/${ORG}/packages/container/${pkg}/versions/${id}" >/dev/null 2>&1; then
        echo "deleted ${pkg} version ${id}"
      else
        echo "warn: delete failed for ${pkg} ${id}"
        status=1
      fi
    done
  done
fi

if (( status == 0 )); then
  echo "teardown done: PR #${PR}"
else
  echo "teardown done with errors: PR #${PR}" >&2
fi
exit "$status"
