#!/usr/bin/env bash
# Resolve ox-v* release tags / CLI-feed download URLs for a commit.
#
# Sourced by railway-staging-api-redeploy.sh and railway-production-deploy.sh;
# also runnable for one-shot lookups:
#   scripts/ox-dist-tag.sh tag <sha> [--merged-fallback]
#   scripts/ox-dist-tag.sh url <sha> [--merged-fallback]
#
# Under `set -o pipefail`, a bare `git tag … | grep …` exits 1 when no tags
# match (grep's "no lines" status). Every pipeline here must tolerate empty
# input so untagged commits resolve to "" (host-only ox) instead of killing
# the caller.

# Newest stable ox-v*.*.* tag for a git selector (--points-at / --merged).
# Stable = no -alpha / -beta / -nightly prerelease suffix. Empty when none.
ox_newest_stable_tag() {
  local selector="$1" sha="$2"
  git tag "$selector" "$sha" --list 'ox-v*.*.*' \
    | { grep -v -e '-alpha' -e '-beta' -e '-nightly' || true; } \
    | sort -V | tail -n1
}

# Newest ox-v* tag (including prereleases) pointing at a commit. Empty when none.
ox_newest_tag_points_at() {
  local sha="$1"
  git tag --points-at "$sha" --list 'ox-v*' | sort -V | tail -n1
}

# Resolve the ox-v* tag used for a commit's CLI feed.
#   --merged-fallback  after points-at misses, take the newest stable ox tag
#                      merged into the commit (production promote path).
# Empty stdout = no released ox → callers pin OXIDEAN_CLI_DIST_URL empty.
ox_resolve_tag() {
  local sha="$1"
  local merged=0
  shift || true
  local arg
  for arg in "$@"; do
    case "$arg" in
      --merged-fallback) merged=1 ;;
      *)
        echo "ox-dist-tag: unknown argument: ${arg}" >&2
        return 2
        ;;
    esac
  done
  local tag
  tag="$(ox_newest_stable_tag --points-at "$sha")"
  if [[ -z "$tag" && "$merged" == "1" ]]; then
    tag="$(ox_newest_stable_tag --merged "$sha")"
  fi
  if [[ -z "$tag" ]]; then
    tag="$(ox_newest_tag_points_at "$sha")"
  fi
  printf '%s' "$tag"
}

# Download URL for the commit's ox release feed, or empty for host-only.
ox_dist_url_for_sha() {
  local sha="$1"
  shift || true
  local tag
  tag="$(ox_resolve_tag "$sha" "$@")" || return $?
  if [[ -n "$tag" ]]; then
    printf '%s' "https://github.com/${GITHUB_REPOSITORY:-oxidean/oxidean}/releases/download/${tag}"
  fi
}

# CLI entry when executed (not sourced).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  usage() {
    sed -n '2,12p' "$0"
    exit 2
  }
  [[ $# -ge 2 ]] || usage
  cmd="$1"
  sha="$2"
  shift 2
  case "$cmd" in
    tag) ox_resolve_tag "$sha" "$@" ;;
    url) ox_dist_url_for_sha "$sha" "$@" ;;
    *) usage ;;
  esac
  printf '\n'
fi
