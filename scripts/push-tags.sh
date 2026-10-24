#!/usr/bin/env bash
# Push release tags to origin in atomic batches of at most three tags.
#
# Usage:
#   scripts/push-tags.sh <tag> [<tag> ...]
#
# Why batches: GitHub creates no push events for tags when more than three
# tags are pushed in a single push — a full component cut (~38 tags) in one
# transaction silently skips every tag-triggered lane. That is what broke the
# 2026-10-24 stable cut: ox-cli.yml never ran for ox-v0.3.0, so the api
# container's boot-time OXIDEAN_CLI_DIST_URL fetch 404'd and the production
# promote failed (release.yml and images.yml tag lanes were suppressed too).
#
# Why still --atomic per batch (not `for t in $NEW; do git push`): rapid
# sequential ref-creation pushes race GitHub's ref backend and intermittently
# come back `remote: fatal error in commit_refs`, leaving a partially-cut
# release — release-beta run 39315353804 pushed 6/8 tags before the rejection
# on oxidean-vcs-v0.2.0-beta.1, and `set -e` aborted the loop so the rest
# never attempted. --atomic makes each batch a single ref transaction —
# every tag in the batch lands or none does — and a failed batch is safe to
# retry: refs that did land come back "up-to-date", never "rejected".

set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "push-tags: no tags given — nothing to push"
  exit 0
fi

attempts=3
batch=()

push_batch() {
  ((${#batch[@]})) || return 0
  local attempt
  for attempt in $(seq 1 "$attempts"); do
    if git push --atomic origin "${batch[@]}"; then
      batch=()
      return 0
    fi
    if [[ "$attempt" -eq "$attempts" ]]; then
      echo "::error::tag push failed after ${attempts} attempts: ${batch[*]}"
      exit 1
    fi
    echo "::warning::tag push attempt ${attempt} of ${attempts} failed for: ${batch[*]} — retrying in 15s"
    sleep 15
  done
}

for tag in "$@"; do
  batch+=("$tag")
  ((${#batch[@]} < 3)) || push_batch
done
push_batch
