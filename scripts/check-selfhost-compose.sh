#!/usr/bin/env bash
# Drift gate for the self-host Compose example (deploy/selfhost/).
#
# Keeps the example honest as the product moves:
#   1. `docker compose config` parses for every profile combination.
#   2. Image refs stay within the published set, and the referenced
#      OXIDEAN_VERSION tag's manifests exist on GHCR (ghcr-images.sh).
#   3. Every variable the example references is real — consumed by code
#      (crates/**), documented (docs/CONFIGURATION.md), or an explicit
#      compose-level allowlist entry — so renamed/dropped knobs fail here.
#   4. The production-required set is actually present in the example.
#   5. .env.example has no orphans: every key is wired into a compose file,
#      and every OXIDEAN_* it documents appears in CONFIGURATION.md.
#
# Usage: scripts/check-selfhost-compose.sh   (also: make selfhost-check)

set -euo pipefail

cd "$(dirname "$0")/.."
DIR=deploy/selfhost
COMPOSE="$DIR/docker-compose.yml"
OVERRIDE="$DIR/docker-compose.docker-sock.yml"
ENVX="$DIR/.env.example"
CONFIGMD=docs/CONFIGURATION.md
COMPOSE_FILES=("$COMPOSE" "$OVERRIDE")

fail() { echo "selfhost-check: FAIL: $*" >&2; exit 1; }
info() { echo "selfhost-check: $*"; }

for cmd in docker curl; do
  command -v "$cmd" >/dev/null 2>&1 || fail "requires $cmd on PATH"
done

# Third-party tag existence. docker.io refs go through the Hub tags API:
# `docker manifest inspect` spends the anonymous pull-rate budget, which
# flakes on shared CI egress IPs. Other registries use manifest inspect.
third_party_image_exists() {
  local img="$1" ref tag repo first
  tag="${img##*:}"
  ref="${img%:*}"
  ref="${ref#docker.io/}"
  ref="${ref#index.docker.io/}"
  first="${ref%%/*}"
  if [[ "$first" == *.* || "$first" == *:* || "$first" == "localhost" ]]; then
    docker manifest inspect "$img" >/dev/null 2>&1
    return
  fi
  repo="$ref"
  [[ "$repo" == */* ]] || repo="library/$repo"
  [ "$(curl -sS -o /dev/null -w '%{http_code}' \
    "https://hub.docker.com/v2/repositories/${repo}/tags/${tag}")" = "200" ]
}

for f in "$COMPOSE" "$OVERRIDE" "$ENVX" "$CONFIGMD" scripts/ghcr-images.sh; do
  [ -f "$f" ] || fail "missing $f"
done

# --- 1. compose config parses under every profile combo ----------------------
# `:?`-guarded secrets get dummy values — config validation only.
export POSTGRES_PASSWORD=check OXIDEAN_ACTIONS_SECRETS_KEY=check \
  DOMAIN=example.invalid OXIDEAN_RUNNER_REGISTRATION_TOKEN=check
for combo in "" "--profile tls" "--profile actions" "--profile tls --profile actions"; do
  # shellcheck disable=SC2086
  docker compose -f "$COMPOSE" $combo config >/dev/null \
    || fail "docker compose config failed for: ${combo:-base}"
done
docker compose -f "$COMPOSE" -f "$OVERRIDE" --profile actions config >/dev/null \
  || fail "docker compose config failed for docker-sock override"
info "compose config parses (base, tls, actions, tls+actions, docker-sock override)"

# --- 2. image refs: allowlist + manifest existence ---------------------------
mapfile -t imgs < <(grep -hoE 'image:[[:space:]]*[^[:space:]]+' "${COMPOSE_FILES[@]}" | awk '{print $2}' | sort -u)
[ "${#imgs[@]}" -gt 0 ] || fail "no image: refs found in $COMPOSE"

allowed_oxidean="api web runner"
oxidean_svcs=""
for img in "${imgs[@]}"; do
  case "$img" in
    ghcr.io/oxidean/oxidean-*:*)
      svc="${img#ghcr.io/oxidean/oxidean-}"
      svc="${svc%%:*}"
      case " $allowed_oxidean " in *" $svc "*) ;; *) fail "unexpected oxidean image 'oxidean-$svc' — published set is: $allowed_oxidean";; esac
      oxidean_svcs="${oxidean_svcs:+$oxidean_svcs }$svc"
      ;;
    ghcr.io/oxidean/*)
      fail "nonstandard oxidean image ref: $img"
      ;;
    *:*)
      third_party_image_exists "$img" \
        || fail "third-party image '$img' has no manifest — typo'd tag?"
      info "third-party image ok: $img"
      ;;
    *) fail "unpinned image ref: $img" ;;
  esac
done

# Resolve the tag the way Compose resolves it for a copied .env: process env
# wins, then the template's uncommented value, then the compose :- fallback —
# so a stale OXIDEAN_VERSION in .env.example fails the manifest check too.
tag="${OXIDEAN_VERSION:-}"
[ -n "$tag" ] || tag="$(grep -oE '^OXIDEAN_VERSION=[^[:space:]#]+' "$ENVX" | head -1 | cut -d= -f2-)"
[ -n "$tag" ] || tag="$(grep -oE 'OXIDEAN_VERSION:-[a-zA-Z0-9._-]+' "$COMPOSE" | head -1 | sed 's/.*:-//')"
[ -n "$tag" ] || fail "could not resolve OXIDEAN_VERSION (env / .env.example / compose default)"
svcs_csv="$(echo "$oxidean_svcs" | tr ' ' '\n' | sort -u | tr '\n' ',' | sed 's/,$//')"
scripts/ghcr-images.sh check "$tag" --services "$svcs_csv" >/dev/null \
  || fail "ghcr.io/oxidean/oxidean-{$svcs_csv}:$tag — manifest check failed (published?)"
info "oxidean images resolve on GHCR at tag '$tag' ($svcs_csv)"

# --- 3. every referenced variable is real ------------------------------------
# Compose-level vars that are not consumed by application code: docker-image
# env, Caddyfile env, or interpolation-only knobs.
allowlist="POSTGRES_USER POSTGRES_PASSWORD POSTGRES_DB MYSQL_USER MYSQL_PASSWORD \
MYSQL_DATABASE MYSQL_ROOT_PASSWORD MYSQL_DATABASE_URL DOMAIN OXIDEAN_VERSION \
OXIDEAN_HTTP_BIND OXIDEAN_RUNNER_ORIGIN OXIDEAN_RUNNER_NAME OXIDEAN_RUNNER_LABELS \
OXIDEAN_RUNNER_GIT_TOKEN RUST_LOG PORT API_BIND"

# ${VAR} interpolation refs in the compose files
mapfile -t vars_interp < <(grep -hoE '\$\{[A-Z][A-Z0-9_]*' "${COMPOSE_FILES[@]}" | sed 's/^\${//' | sort -u)
# env-block keys ("      KEY:" under environment:)
mapfile -t vars_env < <(grep -hoE '^[[:space:]]+[A-Z][A-Z0-9_]*:' "${COMPOSE_FILES[@]}" | tr -d ' :' | sort -u)
# uncommented keys in .env.example
mapfile -t vars_envx < <(grep -oE '^[A-Z][A-Z0-9_]*=' "$ENVX" | tr -d '=' | sort -u)

candidates="$(printf '%s\n' "${vars_interp[@]}" "${vars_env[@]}" "${vars_envx[@]}" | sort -u)"
stale=""
while IFS= read -r v; do
  [ -z "$v" ] && continue
  case " $allowlist " in *" $v "*) continue;; esac
  case "$v" in
    OXIDEAN_*|DATABASE_URL|WORKOS_*|API_BIND|RUST_LOG|PORT)
      if ! grep -rqwF "$v" crates/ && ! grep -qwF "$v" "$CONFIGMD"; then
        stale="$stale $v"
      fi
      ;;
    *) : ;; # non-product keys (POSTGRES_*, etc.) — allowlist covers intent
  esac
done <<< "$candidates"
[ -z "$stale" ] || fail "referenced vars not found in crates/** or docs/CONFIGURATION.md:${stale}"
info "all referenced variables resolve in code or docs"

# --- 4. production-required set is present ------------------------------------
# User-facing knobs must appear in BOTH files; container-internal constants
# only need to be set in the compose env blocks.
required_both="OXIDEAN_ENV OXIDEAN_CORS_ORIGINS OXIDEAN_PUBLIC_ORIGIN \
OXIDEAN_ACTIONS_SECRETS_KEY OXIDEAN_SSH_ENABLED OXIDEAN_SSH_HOST OXIDEAN_SSH_PORT \
OXIDEAN_WEB_BEHIND_PROXY DATABASE_URL POSTGRES_PASSWORD \
OXIDEAN_WEB_FLOW_PRIVATE_KEY OXIDEAN_AUTO_MIGRATE"
required_compose_only="OXIDEAN_PROTECTION_HELPER OXIDEAN_SSH_HOST_KEY_DIR \
OXIDEAN_API_ORIGIN"
missing_req=""
for v in $required_both; do
  grep -qwF "$v" "$COMPOSE" || missing_req="$missing_req $v(compose)"
  grep -qwF "$v" "$ENVX" || missing_req="$missing_req $v(.env.example)"
done
for v in $required_compose_only; do
  grep -qwF "$v" "$COMPOSE" || missing_req="$missing_req $v(compose)"
done
[ -z "$missing_req" ] || fail "required knobs missing:${missing_req}"
info "required production knobs present in compose + .env.example"

# --- 5. .env.example has no orphans -------------------------------------------
orphans=""
for v in "${vars_envx[@]}"; do
  grep -qE "\\\$\{${v}[}:?]|^[[:space:]]+${v}:" "${COMPOSE_FILES[@]}" || orphans="$orphans $v"
done
[ -z "$orphans" ] || fail ".env.example keys never wired into compose:${orphans}"

undoc=""
for v in "${vars_envx[@]}"; do
  case "$v" in OXIDEAN_*|DATABASE_URL|WORKOS_*)
    grep -qwF "$v" "$CONFIGMD" || undoc="$undoc $v";;
  esac
done
[ -z "$undoc" ] || fail ".env.example vars missing from docs/CONFIGURATION.md:${undoc}"
info ".env.example is fully wired and documented"

info "OK — self-host example is structurally valid, current, and documented"
