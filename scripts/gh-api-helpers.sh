#!/usr/bin/env bash
# Shared gh-api error classification for the PR-surface cleanup scripts
# (pr-env-teardown.sh, pr-env-sweep.sh). A denied call is a missing grant —
# permanent until a token in the chain gains it — so callers degrade it to
# a warning instead of a hard failure; anything else is a real error (5xx,
# rate limit, network, unexpected state).

is_scope_denial() { # $1 = captured gh api stderr/output
  case "$1" in
    *"HTTP 401"*|*"HTTP 403"*|*"HTTP 404"*|*"Resource not accessible"*|*"read:packages"*|*"delete:packages"*) return 0 ;;
    *) return 1 ;;
  esac
}
