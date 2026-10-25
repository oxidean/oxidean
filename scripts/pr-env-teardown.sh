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
# Every leg is best-effort — failures warn and the run continues. Errors
# split into two classes: a scope wall (401/403/404, "Resource not
# accessible") is a missing grant — permanent until the release App or PAT
# gains it — so it degrades to a `::warning` + step-summary leak report
# instead of failing the run; anything else still exits non-zero, so a red
# job means an unexpected failure, not a known missing grant. A permanently
# red job trains everyone to ignore it — which is how the real leaks went
# unnoticed across three fix PRs (runs 40147871008/40150179728/40153046588).
# OXIDEAN_TEARDOWN_STRICT=1 restores the old policy (degraded legs fail).
#
# Credentials:
#   RAILWAY_API_TOKEN    account/workspace token (Railway leg skipped when
#                        unset; env-scoped project tokens cannot enumerate)
#   RAILWAY_PROJECT_ID   project the PR env lives in (required for that leg)
#   GH_ADMIN_TOKEN       token able to delete environments (release GitHub
#                        App Administration grant / admin PAT). No GH_TOKEN
#                        fallback — the job token can never manage
#                        environments, so it is not a useful last resort.
#   GH_PKG_TOKEN         extra token able to delete GHCR package versions
#                        (release App org Packages grant / PAT with
#                        delete:packages). Falls back to GH_ADMIN_TOKEN;
#                        only tried after GH_TOKEN is denied.
#   GH_TOKEN             repo token for gh api — deletes versions of the
#                        repo-published oxidean-* packages (granular
#                        permissions grant the publishing repo admin on the
#                        package; job has packages:write), lists versions,
#                        and carries deployments:write for inactivation —
#                        but is never used for environment deletes
#   GH_REPO              owner/repo (default: $GITHUB_REPOSITORY)
#
# Usage:
#   scripts/pr-env-teardown.sh <pr-number> [--dry-run]

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/railway-gql.sh
source scripts/railway-gql.sh
# shellcheck source=scripts/gh-api-helpers.sh
source scripts/gh-api-helpers.sh

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

status=0 degraded=0
declare -a degraded_notes=()

# A denied leg is a missing grant, not a runtime error — count it apart so
# the run can warn + summarize without going red.
degrade() {
  degraded=$((degraded + 1))
  degraded_notes+=("- $1")
  echo "warn: $1"
}

# Version ids on a package carrying pr-<PR> tags; caller captures combined
# output so a failure's error text can be classified. any() keeps a version
# tagged both pr-<N> and pr-<N>-sha-* from being emitted (and deleted) twice.
pr_version_ids() { # $1=token $2=package
  GH_TOKEN="$1" gh api "orgs/${ORG}/packages/container/$2/versions?per_page=100" \
    --paginate \
    --jq ".[] | select(any(.metadata.container.tags[]?; test(\"^pr-${PR}(-|\$)\"))) | .id" 2>&1
}

# Retire a PR environment's deployments when the env object itself can't be
# deleted — the only move left that stops them pointing at a dead preview
# URL. Returns non-zero when the list or any status POST failed.
inactivate_env_deployments() {
  local dep_ids dep dep_fail=0 inactivated=0
  # -X GET is load-bearing: gh api defaults to POST once -f params exist,
  # which lands on create-deployment ("ref wasn't supplied"), not a list.
  # Listing and statuses ride the job's GITHUB_TOKEN — the workflow grants
  # it exactly `deployments: write`, while the admin-token chain may
  # resolve to a PAT without that scope (runs 40147871008/40150179728).
  if ! dep_ids="$(GH_TOKEN="${GH_TOKEN:-$admin_token}" gh api -X GET "repos/${REPO}/deployments" \
    -f environment="$ENV_LABEL" -f per_page=100 --paginate --jq '.[].id' 2>/dev/null)"; then
    echo "warn: could not list deployments for '${ENV_LABEL}' — cannot retire them"
    return 1
  fi
  while IFS= read -r dep; do
    [[ "$dep" =~ ^[0-9]+$ ]] || continue
    if GH_TOKEN="${GH_TOKEN:-$admin_token}" gh api -X POST "repos/${REPO}/deployments/${dep}/statuses" \
      -f state=inactive -f description="PR #${PR} closed — environment retired" >/dev/null 2>&1; then
      inactivated=$((inactivated + 1))
    else
      echo "warn: could not mark deployment ${dep} inactive"
      dep_fail=$((dep_fail + 1))
    fi
  done <<< "$dep_ids"
  (( dep_fail == 0 )) || return 1
  echo "marked ${inactivated} deployment(s) inactive for '${ENV_LABEL}'"
}

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
# The job token is deliberately not a fallback here: it can never delete an
# environment, so it would only reproduce the 403 this leg already knows
# how to degrade.
admin_token="${GH_ADMIN_TOKEN:-}"
if [[ -z "$admin_token" ]]; then
  # No env-capable credential — the env object is out of reach; the job
  # token can still retire its deployments.
  degrade "no env-delete token (release App Administration mint and RELEASE_TOKEN both unavailable) — env state unknown; attempting deployment inactivation anyway"
  if ! inactivate_env_deployments; then
    status=1
  fi
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
  degrade "could not delete github environment '${ENV_LABEL}' (token lacks environment admin) — env object remains"
  if ! inactivate_env_deployments; then
    status=1
  fi
elif [[ "$env_probe" == *"404"* || "$env_probe" == *"Not Found"* ]]; then
  echo "github environment '${ENV_LABEL}' already absent"
elif is_scope_denial "$env_probe"; then
  # The token can't even read the env — deployments may still exist, and
  # the job token's deployments:write retires them regardless.
  degrade "could not read github environment '${ENV_LABEL}' state (scope wall) — attempting deployment inactivation anyway"
  if ! inactivate_env_deployments; then
    status=1
  fi
else
  echo "warn: could not read github environment '${ENV_LABEL}' state — ${env_probe}"
  status=1
fi

# --- GHCR pr-<n> tags --------------------------------------------------------
# Listing and deleting need different tokens. These packages are connected
# to this repo (images.yml pushes them with GITHUB_TOKEN), so the job token
# can READ their version lists even without package scopes.
# Deletes try each credential in turn, job token first: granular package
# permissions grant the repo admin on packages its own workflow published,
# and the job already has packages:write (run 40204467596 proved the
# org-scoped App mint 404s — Packages needs an org-wide installation). If
# the job token is ever denied — e.g. packages disconnected from the repo —
# the App/PAT chain gets its turn before the leg degrades. A denied leg is
# a missing grant — it warns + lands on the step summary instead of
# failing the run (run 40153046588: RELEASE_TOKEN couldn't even list).
pkg_token="${GH_PKG_TOKEN:-${GH_ADMIN_TOKEN:-}}"
list_token="${GH_TOKEN:-$pkg_token}"
declare -a del_tokens=()
for t in "${GH_TOKEN:-}" "${GH_PKG_TOKEN:-}" "${GH_ADMIN_TOKEN:-}"; do
  [[ -n "$t" ]] && del_tokens+=("$t")
done
if [[ -z "$list_token" ]] && (( ${#del_tokens[@]} == 0 )); then
  echo "warn: no GitHub token — skipping GHCR pr-${PR} tag cleanup"
  status=1
elif [[ "$DRY_RUN" == "1" ]]; then
  echo "dry-run: would delete pr-${PR}*/pr-${PR}-sha-* tags on oxidean-{api,web,runner,gateway}"
else
  for svc in api web runner gateway; do
    pkg="oxidean-${svc}"
    if ids="$(pr_version_ids "$list_token" "$pkg")"; then
      :
    elif [[ "$list_token" != "$pkg_token" && -n "$pkg_token" ]] \
      && ids="$(pr_version_ids "$pkg_token" "$pkg")"; then
      : # package chain listed where the job token couldn't
    elif is_scope_denial "$ids"; then
      degrade "cannot list GHCR versions for ${pkg} — pr-${PR} tag state unknown (needs App Packages grant or PAT read:packages): ${ids%%$'\n'*}"
      continue
    else
      echo "warn: could not list versions for ${pkg} — ${ids}"
      status=1
      continue
    fi
    if (( ${#del_tokens[@]} == 0 )); then
      # No delete-capable credential at all — only worth degrading when
      # there are actually tags to remove.
      if [[ -n "${ids//[[:space:]]/}" ]]; then
        degrade "no package-delete token for ${pkg} — pr-${PR} tags retained (job token needs repo-published packages or a PAT delete:packages)"
      fi
      continue
    fi
    while IFS= read -r id; do
      [[ "$id" =~ ^[0-9]+$ ]] || continue
      deleted=0 del_err=""
      for tok in "${del_tokens[@]}"; do
        if del_err="$(GH_TOKEN="$tok" gh api -X DELETE "orgs/${ORG}/packages/container/${pkg}/versions/${id}" 2>&1)"; then
          deleted=1
          break
        fi
        # Scope wall → next credential gets its turn; a real error fails fast.
        is_scope_denial "$del_err" || break
      done
      if (( deleted )); then
        echo "deleted ${pkg} version ${id}"
      elif is_scope_denial "$del_err"; then
        degrade "delete denied for ${pkg} version ${id} — pr-${PR} tag retained (needs package admin: repo-published packages grant the job token admin, else PAT delete:packages): ${del_err%%$'\n'*}"
      else
        echo "warn: delete failed for ${pkg} ${id} — ${del_err}"
        status=1
      fi
    done <<< "$ids"
  done
fi

# OXIDEAN_TEARDOWN_STRICT=1 restores the old policy: missing grants fail.
if [[ "${OXIDEAN_TEARDOWN_STRICT:-0}" == "1" && $degraded -gt 0 ]]; then
  status=1
fi
if (( degraded > 0 )); then
  echo "::warning::teardown for PR #${PR} degraded — ${degraded} surface(s) retained behind missing grants (see the step summary)"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      echo "### Teardown degraded for PR #${PR}"
      echo
      echo "${degraded} leg(s) left surfaces in place behind missing token grants:"
      echo
      printf '%s\n' "${degraded_notes[@]}"
      echo
      echo "To clear them: grant the release App **Administration: write** (env deletes) and **Packages: write** (GHCR deletes), or point \`RELEASE_TOKEN\` at a PAT with admin + read:packages + delete:packages."
    } >> "$GITHUB_STEP_SUMMARY"
  fi
fi
if (( status == 0 )); then
  echo "teardown done: PR #${PR}"
else
  echo "teardown done with errors: PR #${PR}" >&2
fi
exit "$status"
