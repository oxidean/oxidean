#!/usr/bin/env bash
# shellcheck disable=SC2016 # GraphQL literals stay single-quoted; $vars bind via jq --arg/--argjson
# Shared Railway GraphQL + auth helpers. Sourced by the railway-* scripts —
# do not execute directly.
#
#   source "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/railway-gql.sh"
#
# Auth (checked in order by railway_auth_header):
#   RAILWAY_TOKEN       Railway *project* token → `Project-Access-Token` header
#   RAILWAY_API_TOKEN   account/workspace token → `Authorization: Bearer`
#   ~/.railway/config.json  CLI login access token → `Authorization: Bearer`

die() {
  echo "error: $*" >&2
  exit 1
}

need_jq() {
  command -v jq >/dev/null 2>&1 || die "jq is required"
}

railway_api_url() {
  printf '%s' "${RAILWAY_API_URL:-https://backboard.railway.com/graphql/v2}"
}

# Prints one curl config-file line selecting the right auth header for the
# available credential. Railway project tokens are NOT bearer tokens — they
# only authenticate via the `Project-Access-Token` header.
railway_auth_header() {
  if [[ -n "${RAILWAY_TOKEN:-}" ]]; then
    printf 'header = "Project-Access-Token: %s"\n' "$RAILWAY_TOKEN"
    return
  fi
  if [[ -n "${RAILWAY_API_TOKEN:-}" ]]; then
    printf 'header = "Authorization: Bearer %s"\n' "$RAILWAY_API_TOKEN"
    return
  fi
  local cfg="${HOME}/.railway/config.json"
  if [[ -f "$cfg" ]]; then
    local t
    # CLI login stores the bearer in user.accessToken (user.token may be empty).
    t="$(jq -r '.user.accessToken // .user.token // empty' "$cfg" 2>/dev/null || true)"
    if [[ -n "$t" && "$t" != "null" ]]; then
      printf 'header = "Authorization: Bearer %s"\n' "$t"
      return
    fi
  fi
  die "no Railway credential: set RAILWAY_TOKEN (project token) or RAILWAY_API_TOKEN (account/workspace token), or run railway login"
}

# GraphQL: prints response JSON to stdout. Fails on HTTP/transport errors or
# GraphQL errors.
railway_gql() {
  local query="$1"
  local variables_json
  if [[ $# -ge 2 && -n "${2}" ]]; then
    variables_json="$2"
  else
    variables_json='{}'
  fi
  local auth_line payload
  auth_line="$(railway_auth_header)"
  payload="$(jq -n --arg q "$query" --argjson v "$variables_json" '{query: $q, variables: $v}')"
  local raw
  raw="$(
    printf '%s' "$payload" | curl -sS "$(railway_api_url)" \
      -H "Content-Type: application/json" \
      --config <(printf '%s' "$auth_line") \
      -d @-
  )" || die "Railway GraphQL request failed"
  if echo "$raw" | jq -e '.errors? | select(length > 0)' >/dev/null 2>&1; then
    echo "$raw" | jq '.' >&2
    die "Railway GraphQL returned errors"
  fi
  printf '%s' "$raw"
}

# Resolve an environment id by name within a project. Prints the id; dies when
# no environment matches.
railway_resolve_environment_id() {
  local project_id="$1" name="$2"
  local raw id
  raw="$(
    railway_gql '
      query($id: String!) {
        project(id: $id) {
          environments { edges { node { id name } } }
        }
      }
    ' "$(jq -n --arg id "$project_id" '{id:$id}')"
  )"
  id="$(echo "$raw" | jq -r --arg name "$name" '
    .data.project.environments.edges[]?
    | .node
    | select(.name == $name)
    | .id
  ' | head -n1)"
  [[ -n "$id" && "$id" != "null" ]] || die "environment named '${name}' not found in project ${project_id}"
  printf '%s' "$id"
}

# Find the ephemeral environment id Railway created for a pull request
# (Environment.meta.prNumber). Prints the id, or nothing when no such
# environment exists yet.
railway_find_pr_environment_id() {
  local project_id="$1" pr_number="$2"
  local raw
  raw="$(
    railway_gql '
      query($id: String!) {
        project(id: $id) {
          environments {
            edges { node { id name isEphemeral meta { prNumber } } }
          }
        }
      }
    ' "$(jq -n --arg id "$project_id" '{id:$id}')"
  )"
  echo "$raw" | jq -r --argjson pr "$pr_number" '
    .data.project.environments.edges[]?
    | .node
    | select(.isEphemeral == true and .meta.prNumber == $pr)
    | .id
  ' | head -n1
}
