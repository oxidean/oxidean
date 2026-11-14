#!/usr/bin/env bash
# Print the newest commit on origin/main with proven-green CI — the default
# promote target when production-deploy.yml's commit_sha input is empty.
#
# A commit qualifies when its newest ci.yml run on branch=main completed
# successfully AND a completed-success Images push run exists for it (that
# run publishes the sha-* image set the deploy pins). Walking first-parent
# history newest-first means a merge whose CI is still in-flight — or that
# just went red — is skipped in favor of the last proven commit, so landing
# on main never blocks a promote already being prepared. Only pushed tips
# carry a branch=main run, so intermediate commits of a multi-commit push
# are correctly skipped: their exact tree was never CI'd on main.
#
# An explicit commit_sha bypasses this resolver; the workflow's Require CI
# success step still validates whichever SHA was chosen.
#
# Usage:
#   GH_TOKEN=… scripts/last-green-main-commit.sh
#
# Env:
#   GITHUB_REPOSITORY  repo to inspect (default oxidean/oxidean)
#   GH_TOKEN           gh api credential (github.token in Actions)
#   PROMOTE_LOOKBACK   first-parent commits to scan (default 40)

set -euo pipefail

REPO="${GITHUB_REPOSITORY:-oxidean/oxidean}"
LOOKBACK="${PROMOTE_LOOKBACK:-40}"

die() {
  echo "error: $*" >&2
  exit 1
}

for cmd in git gh jq; do
  command -v "$cmd" >/dev/null 2>&1 || die "missing required command: ${cmd}"
done
if [[ ! "$LOOKBACK" =~ ^[0-9]+$ ]] || ((LOOKBACK == 0)); then
  die "PROMOTE_LOOKBACK must be a positive integer"
fi

git fetch origin main:refs/remotes/origin/main

# Per-commit probes — the same head_sha-keyed queries the workflow's Require
# CI success step runs, so resolver and gate can never diverge: ci.yml takes
# the newest run on branch=main only (.[0] — a failed re-run revokes the
# green), while images.yml accepts ANY completed-success push run (a
# cancelled republish does not unpublish the sha-* tags). Probing per SHA
# also keeps each lookup inside a single small page — no bulk-fetch
# truncation window.
while IFS= read -r sha; do
  ci_state="$(gh api "repos/${REPO}/actions/workflows/ci.yml/runs?head_sha=${sha}&branch=main&per_page=10" \
    --jq '.workflow_runs[0] // {} | "\(.status // "none") \(.conclusion // "none")"' 2>/dev/null || echo "api-error api-error")"
  if [[ "$ci_state" != "completed success" ]]; then
    echo "skip ${sha:0:12}: ci newest run = ${ci_state}" >&2
    continue
  fi
  img_ok="$(gh api "repos/${REPO}/actions/workflows/images.yml/runs?head_sha=${sha}&branch=main&per_page=30" \
    --jq '[.workflow_runs[] | select(.event == "push" and .status == "completed" and .conclusion == "success")] | length' 2>/dev/null || echo 0)"
  if [[ "$img_ok" == "0" ]]; then
    echo "skip ${sha:0:12}: no successful Images push run" >&2
    continue
  fi
  printf '%s\n' "$sha"
  exit 0
done < <(git rev-list --first-parent --max-count="${LOOKBACK}" origin/main)

die "no commit in the last ${LOOKBACK} first-parent main commits has a successful CI run + Images push run — pass commit_sha explicitly or wait for CI"
