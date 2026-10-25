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
elif env_probe="$(GH_TOKEN="$admin_token" gh api "repos/${REPO}/environments/${ENV_NAME_ENC}" 2>&1)"; then
  # Exists but the token can't delete it — at least stop the deployments
  # pointing at a dead preview URL. A denied delete alone is not a teardown
  # failure: the env object is cosmetic residue once nothing live points at
  # it, so only a fallback that itself cannot run fails this leg.
  echo "warn: could not delete github environment '${ENV_LABEL}' (token lacks environment admin) — marking its deployments inactive"
  # -X GET is load-bearing: gh api defaults to POST once -f params exist,
  # which lands on create-deployment ("ref wasn't supplied"), not a list.
  if ! dep_ids="$(GH_TOKEN="$admin_token" gh api -X GET "repos/${REPO}/deployments" \
    -f environment="$ENV_LABEL" -f per_page=100 --paginate --jq '.[].id' 2>/dev/null)"; then
    echo "warn: could not list deployments for '${ENV_LABEL}' — cannot retire them"
    status=1
  else
    dep_fail=0 inactivated=0
    while IFS= read -r dep; do
      [[ "$dep" =~ ^[0-9]+$ ]] || continue
      # Statuses need only deployments:write — which the job's GITHUB_TOKEN
      # is granted, while the admin-token chain may resolve to a PAT without
      # that scope (runs 40147871008/40150179728). Prefer the job token.
      if GH_TOKEN="${GH_TOKEN:-$admin_token}" gh api -X POST "repos/${REPO}/deployments/${dep}/statuses" \
        -f state=inactive -f description="PR #${PR} closed — environment retired" >/dev/null 2>&1; then
        inactivated=$((inactivated + 1))
      else
        echo "warn: could not mark deployment ${dep} inactive"
        dep_fail=$((dep_fail + 1))
      fi
    done <<< "$dep_ids"
    if (( dep_fail > 0 )); then
      status=1
    else
      echo "marked ${inactivated} deployment(s) inactive — env object remains (cosmetic residue)"
    fi
  fi
else
  if [[ "$env_probe" == *"404"* || "$env_probe" == *"Not Found"* ]]; then
    echo "github environment '${ENV_LABEL}' already absent"
  else
    echo "warn: could not read github environment '${ENV_LABEL}' state — leaving its deployments alone"
    status=1
  fi
fi

# --- GHCR pr-<n> tags --------------------------------------------------------
# GITHUB_TOKEN can never delete packages — packages:write covers publish,
# not delete — so version deletes ride the admin-token chain (release App
# or maintainer PAT), which needs a package-delete-capable scope.
pkg_token="${GH_ADMIN_TOKEN:-${GH_TOKEN:-}}"
if [[ -z "$pkg_token" ]]; then
  echo "warn: no GitHub token — skipping GHCR pr-${PR} tag cleanup"
  status=1
elif [[ "$DRY_RUN" == "1" ]]; then
  echo "dry-run: would delete pr-${PR}*/pr-${PR}-sha-* tags on oxidean-{api,web,runner,gateway}"
else
  for svc in api web runner gateway; do
    pkg="oxidean-${svc}"
    if ! ids="$(GH_TOKEN="$pkg_token" gh api "orgs/${ORG}/packages/container/${pkg}/versions?per_page=100" \
      --paginate \
      --jq ".[] | select(.metadata.container.tags[]? | test(\"^pr-${PR}(-|\$)\")) | .id" 2>/dev/null)"; then
      echo "warn: could not list versions for ${pkg} — skipping its tag cleanup"
      status=1
      continue
    fi
    while IFS= read -r id; do
      [[ "$id" =~ ^[0-9]+$ ]] || continue
      if GH_TOKEN="$pkg_token" gh api -X DELETE "orgs/${ORG}/packages/container/${pkg}/versions/${id}" >/dev/null 2>&1; then
        echo "deleted ${pkg} version ${id}"
      else
        echo "warn: delete failed for ${pkg} ${id} — token needs package delete scope (GITHUB_TOKEN cannot delete packages)"
        status=1
      fi
    done <<< "$ids"
  done
fi

if (( status == 0 )); then
  echo "teardown done: PR #${PR}"
else
  echo "teardown done with errors: PR #${PR}" >&2
fi
exit "$status"
