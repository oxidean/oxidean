#!/usr/bin/env bash
# Push release tags to origin in one atomic ref transaction.
#
# Usage:
#   scripts/push-tags.sh <tag> [<tag> ...]
#
# Why not `for t in $NEW; do git push origin "$t"; done`: rapid sequential
# ref-creation pushes race GitHub's ref backend and intermittently come back
# `remote: fatal error in commit_refs`, leaving a partially-cut release —
# release-beta run 39315353804 pushed 6/8 tags before the rejection on
# oxidean-vcs-v0.2.0-beta.1, and `set -e` aborted the loop so the rest never
# attempted. One `git push --atomic` is a single ref transaction — every tag
# lands or none does — so a failed attempt is safe to retry: refs that did
# land come back "up-to-date", never "rejected".

set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "push-tags: no tags given — nothing to push"
  exit 0
fi

attempts=3
for attempt in $(seq 1 "$attempts"); do
  if git push --atomic origin "$@"; then
    exit 0
  fi
  if [ "$attempt" -eq "$attempts" ]; then
    echo "::error::tag push failed after ${attempts} attempts"
    exit 1
  fi
  echo "::warning::tag push attempt ${attempt} of ${attempts} failed — retrying in 15s"
  sleep 15
done
