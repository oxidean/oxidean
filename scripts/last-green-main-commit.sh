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
#   RUNS_PAGE          runs fetched per workflow (default 100)

set -euo pipefail

REPO="${GITHUB_REPOSITORY:-oxidean/oxidean}"
LOOKBACK="${PROMOTE_LOOKBACK:-40}"
PAGE="${RUNS_PAGE:-100}"

die() {
  echo "error: $*" >&2
  exit 1
}

for cmd in git gh jq; do
  command -v "$cmd" >/dev/null 2>&1 || die "missing required command: ${cmd}"
done

git fetch origin main:refs/remotes/origin/main

# Newest-first run lists; the first entry seen per head_sha is that commit's
# latest run — the same selection the workflow's per-SHA Require CI success
# step makes: ci.yml takes the newest run only (a failed re-run revokes the
# green), while images.yml accepts ANY completed-success push run (a
# cancelled/failed republish does not unpublish the sha-* tags). ci.yml
# counts any branch=main run (push or dispatch); images counts push events
# only — tag/PR legs never publish the sha-* set.
ci_runs="$(gh api "repos/${REPO}/actions/workflows/ci.yml/runs?branch=main&per_page=${PAGE}" \
  --jq '[.workflow_runs[] | {head_sha, status, conclusion}]')"
img_runs="$(gh api "repos/${REPO}/actions/workflows/images.yml/runs?branch=main&per_page=${PAGE}" \
  --jq '[.workflow_runs[] | select(.event == "push") | {head_sha, status, conclusion}]')"

picked="$(
  git rev-list --first-parent --max-count="${LOOKBACK}" origin/main \
    | jq -Rn -r --argjson ci "${ci_runs}" --argjson img "${img_runs}" '
        def latest($runs): reduce $runs[] as $r ({}; .[$r.head_sha] //= {status: $r.status, conclusion: $r.conclusion});
        (latest($ci)) as $ciMap
        | ([$img[] | select(.status == "completed" and .conclusion == "success") | .head_sha] | unique) as $imgOk
        | [inputs
            | . as $s
            | select(
                ($ciMap[$s] // {} | .status == "completed" and .conclusion == "success")
                and ($imgOk | index($s) != null)
              )
          ]
        | .[0] // empty
      '
)"

if [[ -z "${picked}" ]]; then
  die "no commit in the last ${LOOKBACK} first-parent main commits has a successful CI run + Images push run — pass commit_sha explicitly or wait for CI"
fi

printf '%s\n' "${picked}"
