#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL/gh-api literals stay single-quoted; $vars bind via jq --arg
# Sweep stale per-PR surfaces: oxidean-pr-<n> Railway environments, the
# "Oxidean / oxidean-pr-<n>" GitHub Environments, and pr-<n> GHCR tags.
#
# Why this exists: the pull_request:closed cleanup run is routinely
# cancelled before any job starts — on merge GitHub tears down
# refs/pull/<n>/merge and the queued run goes with it — so images.yml's
# cleanup-pr is a fast path, not a guarantee. This sweep is the
# authoritative teardown: it detects leaked PR lanes on every surface
# (envs/tags leak even when the Railway env was never bound) and runs
# scripts/pr-env-teardown.sh for each PR that is no longer open. Each
# leaked env keeps its volumes; past ~6 leaks the project hits the
# 20-volume cap and every subsequent PR env bind dies in volumeCreate.
#
# Never deletes on ambiguity: a PR whose state can't be resolved via
# `gh pr view` is kept. Per-surface listing failures warn and mark the run
# failed without aborting — a broken Railway token must not shield
# GitHub-side leaks (and a red run is the only alert this job has).
#
# Credentials:
#   RAILWAY_API_TOKEN    account/workspace token (enumeration needs one;
#                        env-scoped project tokens cannot list environments)
#                        or a `railway login` config
#   RAILWAY_PROJECT_ID   project holding the PR envs (Railway surface only)
#   GH_ADMIN_TOKEN       token able to list/delete GitHub environments
#                        (release GitHub App / admin PAT; falls back to
#                        GH_TOKEN which cannot delete environments at all)
#   GH_TOKEN             gh api token — packages:write on the job to read
#                        and delete GHCR package versions
#   GH_REPO              owner/repo (default: $GITHUB_REPOSITORY)
#
# Usage:
#   scripts/pr-env-sweep.sh [--dry-run]

set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/railway-gql.sh
source scripts/railway-gql.sh

DRY_RUN="${OXIDEAN_TEARDOWN_DRY_RUN:-0}"
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    *) die "unexpected argument: ${arg}" ;;
  esac
done

REPO="${GH_REPO:-${GITHUB_REPOSITORY:-}}"
[[ -n "$REPO" ]] || die "set GH_REPO or GITHUB_REPOSITORY"
ORG="${REPO%%/*}"

need_jq
command -v gh >/dev/null 2>&1 || die "gh is required"

declare -A prs=()
surface_err=0

add_prs() { # stdin: one PR number per line
  local n
  while IFS= read -r n; do
    [[ -n "$n" ]] && prs["$n"]=1
  done
}

# --- surface 1: ephemeral Railway envs named oxidean-pr-<n> -----------------
echo "== scanning Railway environments =="
if [[ -z "${RAILWAY_PROJECT_ID:-}" ]]; then
  echo "warn: RAILWAY_PROJECT_ID unset — skipping Railway surface"
  surface_err=1
elif [[ -z "${RAILWAY_API_TOKEN:-}" && ! -f "${HOME}/.railway/config.json" ]]; then
  echo "no account Railway credential — skipping Railway surface"
else
  # Env enumeration requires the account/workspace token.
  unset RAILWAY_TOKEN
  if raw="$(railway_gql '
      query($id: String!) {
        project(id: $id) {
          environments { edges { node { name isEphemeral } } }
        }
      }
    ' "$(jq -n --arg id "$RAILWAY_PROJECT_ID" '{id:$id}')")"; then
    add_prs < <(echo "$raw" | jq -r '
      .data.project.environments.edges[]?.node
      | select(.isEphemeral == true) | .name' \
      | sed -n 's/^oxidean-pr-\([0-9][0-9]*\)$/\1/p')
  else
    echo "::error::Railway environment listing failed — skipping Railway surface" >&2
    surface_err=1
  fi
fi

# --- surface 2: GitHub environments named *oxidean-pr-<n> -------------------
echo "== scanning GitHub environments =="
admin_token="${GH_ADMIN_TOKEN:-${GH_TOKEN:-}}"
if [[ -z "$admin_token" ]]; then
  echo "warn: no GitHub token — skipping GitHub environment surface"
  surface_err=1
elif gh_envs="$(GH_TOKEN="$admin_token" gh api "repos/${REPO}/environments?per_page=100" \
    --jq '.environments[]?.name' 2>/dev/null)"; then
  add_prs < <(echo "$gh_envs" | sed -n 's/^.*oxidean-pr-\([0-9][0-9]*\)$/\1/p')
else
  echo "warn: could not list GitHub environments — skipping that surface" >&2
  surface_err=1
fi

# --- surface 3: GHCR pr-<n> / pr-<n>-sha-* tags ------------------------------
echo "== scanning GHCR pr-* tags =="
if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "warn: GH_TOKEN unset — skipping GHCR surface"
  surface_err=1
else
  for svc in api web runner gateway; do
    if tags="$(gh api "orgs/${ORG}/packages/container/oxidean-${svc}/versions?per_page=100" \
        --paginate --jq '.[].metadata.container.tags[]?' 2>/dev/null)"; then
      add_prs < <(echo "$tags" | sed -n 's/^pr-\([0-9][0-9]*\)\(-.*\)\?$/\1/p')
    else
      echo "warn: could not list versions for oxidean-${svc} — skipping" >&2
      surface_err=1
    fi
  done
fi

if (( ${#prs[@]} == 0 )); then
  echo "no oxidean-pr-* surfaces found — nothing to sweep."
  exit "$surface_err"
fi

echo "candidate PR lanes: $(printf '%s\n' "${!prs[@]}" | sort -n | tr '\n' ' ')"

swept=0 kept=0 failed=0
while read -r pr; do
  state="$(gh pr view "$pr" --repo "$REPO" --json state --jq '.state' 2>/dev/null || true)"
  case "$state" in
    OPEN)
      echo "PR #${pr} is OPEN — keeping"
      kept=$((kept + 1)) ;;
    MERGED | CLOSED)
      echo "PR #${pr} is ${state} — tearing down"
      if [[ "$DRY_RUN" == "1" ]]; then
        scripts/pr-env-teardown.sh "$pr" --dry-run || true
        swept=$((swept + 1))
      elif scripts/pr-env-teardown.sh "$pr"; then
        swept=$((swept + 1))
      else
        failed=$((failed + 1))
      fi ;;
    *)
      echo "warn: could not resolve PR #${pr} state (got '${state:-empty}') — leaving its surfaces in place"
      kept=$((kept + 1)) ;;
  esac
done < <(printf '%s\n' "${!prs[@]}" | sort -n)

echo "sweep done: ${swept} torn down, ${kept} kept (open or unresolved), ${failed} teardown(s) failed"
(( failed == 0 && surface_err == 0 ))
