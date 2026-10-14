#!/usr/bin/env bash
# Verify (or wait for) the Oxidean service image manifests on GHCR.
#
# Usage:
#   scripts/ghcr-images.sh check <tag> [--services api,web,runner,gateway]
#   scripts/ghcr-images.sh wait  <tag> [--services api,web,runner,gateway] [--timeout 2700]
#
# `check` prints per-image status and exits 1 if any manifest is missing.
# `wait` polls `check` until every manifest exists (default 45m bound).
#
# Image base: ghcr.io/${GHCR_OWNER:-oxidean}/oxidean-<service>. Auth: public
# packages pull with an anonymous token; set GHCR_TOKEN (or GITHUB_TOKEN) for
# private packages.

set -euo pipefail

OWNER="${GHCR_OWNER:-oxidean}"
IMAGE_BASE="${OXIDEAN_IMAGE_BASE:-ghcr.io/${OWNER}}"
DEFAULT_SERVICES="api,web,runner,gateway"
TIMEOUT=2700
INTERVAL=20

die() {
  echo "error: $*" >&2
  exit 1
}

# Bearer token for the GHCR v2 API — anonymous for public packages, basic-auth
# exchange when GHCR_TOKEN/GITHUB_TOKEN is present (private packages).
ghcr_token() {
  local repo="$1"
  local args=()
  if [[ -n "${GHCR_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
    args+=(-u "${GHCR_USER:-${GITHUB_ACTOR:-oxidean-ci}}:${GHCR_TOKEN:-${GITHUB_TOKEN:-}}")
  fi
  curl -fsSL "${args[@]}" \
    "https://ghcr.io/token?scope=repository:${repo}:pull" 2>/dev/null \
    | jq -r '.token'
}

# 0 when <image>:<tag> has a manifest on GHCR, 1 otherwise. GHCR answers 403
# (not 404) for missing/private tags — treated as MISS either way; the caller
# prints the status line, so curl's own error text is swallowed.
ghcr_manifest_exists() {
  local repo="$1" tag="$2"
  local token
  token="$(ghcr_token "$repo")" || return 1
  curl -fsSL -o /dev/null \
    -H "Authorization: Bearer ${token}" \
    -H 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json' \
    "https://ghcr.io/v2/${repo}/manifests/${tag}" 2>/dev/null
}

check_all() {
  local tag="$1" services_csv="$2"
  local missing=0 svc repo
  IFS=',' read -r -a svcs <<< "$services_csv"
  for svc in "${svcs[@]}"; do
    repo="${OWNER}/oxidean-${svc}"
    if ghcr_manifest_exists "$repo" "$tag"; then
      echo "ok   ${IMAGE_BASE}/oxidean-${svc}:${tag}"
    else
      echo "MISS ${IMAGE_BASE}/oxidean-${svc}:${tag}" >&2
      missing=1
    fi
  done
  return "$missing"
}

usage() {
  sed -n '2,13p' "$0"
}

main() {
  command -v jq >/dev/null 2>&1 || die "jq is required"
  local cmd="${1:-}"
  shift || true
  local tag="" services="$DEFAULT_SERVICES"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --services) services="$2"; shift ;;
      --timeout) TIMEOUT="$2"; shift ;;
      -h | --help)
        usage
        exit 0
        ;;
      *)
        [[ -z "$tag" ]] || die "unexpected argument: $1"
        tag="$1"
        ;;
    esac
    shift
  done
  [[ -n "$tag" ]] || die "usage: $0 <check|wait> <tag> [--services …] [--timeout secs]"

  case "$cmd" in
    check)
      check_all "$tag" "$services"
      ;;
    wait)
      local deadline=$((SECONDS + TIMEOUT))
      until check_all "$tag" "$services"; do
        if (( SECONDS >= deadline )); then
          die "timed out (${TIMEOUT}s) waiting for GHCR manifests tagged ${tag}"
        fi
        echo "…waiting for GHCR manifests tagged ${tag} (sleep ${INTERVAL}s)"
        sleep "$INTERVAL"
      done
      ;;
    "" | -h | --help)
      usage
      ;;
    *)
      usage >&2
      die "unknown command: ${cmd}"
      ;;
  esac
}

main "$@"
