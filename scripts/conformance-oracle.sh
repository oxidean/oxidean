#!/usr/bin/env bash
# conformance-oracle.sh — run the Phase 0 conformance corpus against real
# GitHub Actions and record per-run/per-job outcomes as
# crates/oxidean-api/tests/conformance/golden.json. This is the parity oracle:
# the harness diffs against it when OXIDEAN_CONFORMANCE_GOLDEN=1. Never a CI
# gate — it needs a scratch repo, `gh auth`, network, and runner minutes.
#
# Usage:
#   scripts/conformance-oracle.sh <github-owner/repo> [--cleanup]
#
# Requirements: gh (authenticated, `repo` + `workflow` scopes), git, jq.
# The scratch repo must allow Actions and have a runner available
# (ubuntu-latest works on any github-hosted plan).
set -euo pipefail

REPO="${1:?usage: conformance-oracle.sh <owner/repo> [--cleanup]}"
CLEANUP="${2:-}"
CORPUS="$(cd "$(dirname "$0")/.." && pwd)/crates/oxidean-api/tests/conformance"
GOLDEN="$CORPUS/golden.json"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

for cmd in gh git jq; do
  command -v "$cmd" >/dev/null || { echo "missing $cmd" >&2; exit 1; }
done

# Only push run-kind + warning cases — validation_failure cases must not pass
# GitHub's own workflow validation (they would be red in the oracle too, but
# GitHub reports them as startup failures which we record separately).
KINDS=$(jq -r '.cases[] | select(.kind=="run" or .kind=="queued") | .id' "$CORPUS/cases.json")

mkdir -p "$WORK/repo/.github/workflows"
cd "$WORK/repo"
git init -q -b main .
for id in $KINDS; do
  # Workflows with ${{ }} warnings still run; secrets/env/quota notes are
  # documented per case — oracle records raw GitHub outcomes.
  cp "$CORPUS/workflows/$id.yml" ".github/workflows/$id.yml"
done
git add -A
git -c user.email=oracle@oxidean.local -c user.name=oracle commit -qm "conformance corpus"

gh repo create "$REPO" --private --source=. --push 2>/dev/null || git push "https://github.com/$REPO" main
git push -f "https://github.com/$REPO" main

echo "pushed $(echo "$KINDS" | wc -w) workflows; waiting for runs..."
sleep 20

OUT='{}'
for id in $KINDS; do
  # Latest run for this workflow file.
  run=$(gh api "repos/$REPO/actions/workflows/$id.yml/runs?per_page=1" \
    --jq '.workflow_runs[0] | {id, status, conclusion}' 2>/dev/null || echo '{}')
  rid=$(jq -r '.id // empty' <<<"$run")
  if [ -z "$rid" ]; then
    OUT=$(jq --arg id "$id" '. + {($id): {status:"no-run"}}' <<<"$OUT")
    continue
  fi
  jobs=$(gh api "repos/$REPO/actions/runs/$rid/jobs" \
    --jq '[.jobs[] | {key: .name, status, conclusion}]' 2>/dev/null || echo '[]')
  OUT=$(jq --arg id "$id" --argjson run "$run" --argjson jobs "$jobs" \
    '. + {($id): {run: $run.conclusion, status: $run.status, jobs: $jobs}}' <<<"$OUT")
done

jq --arg repo "$REPO" --arg at "$(date -u +%FT%TZ)" \
  '{repo: $repo, harvested_at: $at, cases: .}' <<<"$OUT" >"$GOLDEN"
echo "wrote $GOLDEN"

if [ "$CLEANUP" = "--cleanup" ]; then
  gh repo delete "$REPO" --yes
  echo "deleted $REPO"
fi
