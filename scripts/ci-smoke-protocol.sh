#!/usr/bin/env bash
# CI / local fail-closed runner for forge protocol smokes (D-QH-04).
# Brings up Compose, seeds a verified owner + public repo + PAT + session
# cookie via the RPC surface (DEBT-11), then asserts Traefik/.git/LFS/packages
# routing and runs real git ls-remote/push over HTTPS and SSH.
# DEBT-01: compose-smoke-protection runs last — it re-ups a fresh stack (wipes
# volumes), seeds its own repo, and asserts HTTPS protected-push denial (ORG-06).
#
# Env knobs:
#   SMOKE_SEED_FIXTURES   default 1 — bootstrap smokeowner/smokerepo + PAT so
#                         the client checks run live. Set 0 for routing/TCP-only
#                         (restores the pre-seed behavior; SMOKE_SKIP_LS_REMOTE
#                         defaults back to 1).
#   SMOKE_SEED_PASSWORD   default SmokeProtocol1! — fixture account password;
#                         also used for auth.login on reused (non-empty) stacks.
#   SMOKE_SEED_EMAIL      default <owner>@example.com.
#   SMOKE_GIT_OWNER / SMOKE_GIT_REPO — fixture identity (defaults match the
#                         per-smoke script defaults smokeowner/smokerepo).
#   SMOKE_SKIP_LS_REMOTE  default 0 when seeded, 1 when SMOKE_SEED_FIXTURES=0.
#   SMOKE_SSH_PUSH        default 1 when seeded — also push a throwaway ref.
#   SMOKE_SKIP_LFS_CLIENT default 1 — git-lfs binary transfer remains opt-in
#                         (needs git-lfs on PATH; out of DEBT-11 scope).
#
# Reusing a stack that already has users: auth.bootstrap_setup returns
# auth.setup_unavailable, so the seed falls back to auth.login with the same
# env credentials. Point them at an existing verified account when needed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Always fail closed in this entrypoint (even if CI env is unset locally).
export CI="${CI:-true}"
export SMOKE_REQUIRE_STACK=1
export SMOKE_SEED_FIXTURES="${SMOKE_SEED_FIXTURES:-1}"
if [[ "${SMOKE_SEED_FIXTURES}" == "1" ]]; then
  export SMOKE_SKIP_LS_REMOTE="${SMOKE_SKIP_LS_REMOTE:-0}"
  export SMOKE_SSH_PUSH="${SMOKE_SSH_PUSH:-1}"
else
  export SMOKE_SKIP_LS_REMOTE="${SMOKE_SKIP_LS_REMOTE:-1}"
fi
export SMOKE_SKIP_LFS_CLIENT="${SMOKE_SKIP_LFS_CLIENT:-1}"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
# shellcheck source=scripts/docker-wsl-creds.sh
source "${ROOT}/scripts/docker-wsl-creds.sh"
SMOKE_NAME="ci-smoke-protocol"
smoke_require_docker

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"
SEED_OWNER="${SMOKE_GIT_OWNER:-smokeowner}"
SEED_REPO="${SMOKE_GIT_REPO:-smokerepo}"
SEED_PASSWORD="${SMOKE_SEED_PASSWORD:-SmokeProtocol1!}"
SEED_EMAIL="${SMOKE_SEED_EMAIL:-${SEED_OWNER}@example.com}"

COOKIE_JAR="$(mktemp)"
RPC_OUT="$(mktemp)"

cleanup() {
  rm -f "$COOKIE_JAR" "$RPC_OUT"
  docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> docker compose config"
docker compose -f "$COMPOSE_FILE" config >/dev/null

if [[ "${OXIDEAN_COMPOSE_SKIP_BUILD:-}" == "1" ]]; then
  echo "==> docker compose up -d --wait (OXIDEAN_COMPOSE_SKIP_BUILD=1; using preloaded images)"
  docker compose -f "$COMPOSE_FILE" up -d --wait
else
  echo "==> docker compose up --build -d --wait"
  docker compose -f "$COMPOSE_FILE" up --build -d --wait
fi

echo "==> wait for ${BASE_URL}/health"
ok=0
for _ in $(seq 1 90); do
  if curl -fsS -o /dev/null "${BASE_URL}/health" 2>/dev/null; then
    ok=1
    break
  fi
  sleep 2
done
if [[ "$ok" -ne 1 ]]; then
  echo "FAIL: health check failed at ${BASE_URL}/health after compose up" >&2
  docker compose -f "$COMPOSE_FILE" ps >&2 || true
  exit 1
fi

# --- Fixture seed (DEBT-11): verified user + public repo + PAT + session. ---
# Same RPC pattern as compose-smoke-protection.sh (cookie jar + python3 JSON).
rpc() {
  local body="$1"
  local http_code
  http_code="$(
    curl -sS -o "$RPC_OUT" -w "%{http_code}" \
      -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
      -H "content-type: application/json" \
      -H "Oxidean-RPC-Version: 1" \
      -d "$body" \
      "${BASE_URL}/api/rpc" || true
  )"
  if [[ "$http_code" != "200" ]]; then
    echo "RPC HTTP $http_code for: $body" >&2
    head -c 800 "$RPC_OUT" >&2 || true
    echo >&2
    return 1
  fi
  if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get("ok") is True else 1)' "$RPC_OUT"; then
    return 0
  fi
  echo "RPC error for: $body" >&2
  head -c 800 "$RPC_OUT" >&2 || true
  echo >&2
  return 1
}

rpc_error_code() {
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print((d.get("error") or {}).get("code") or "")' "$RPC_OUT" 2>/dev/null || true
}

rpc_json_field() {
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["data"][sys.argv[2]])' "$RPC_OUT" "$1"
}

seed_protocol_fixtures() {
  if ! command -v python3 >/dev/null 2>&1; then
    echo "FAIL: python3 not on PATH; cannot parse RPC JSON for fixture seed" >&2
    exit 1
  fi

  echo "==> seed fixtures: ${SEED_OWNER}/${SEED_REPO} (bootstrap or login)"
  if rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"auth.bootstrap_setup","input":{"email":"'"$SEED_EMAIL"'","username":"'"$SEED_OWNER"'","password":"'"$SEED_PASSWORD"'","allow_signup":False,"provider_mode":"local"}}))')"; then
    echo "==> bootstrap_setup created ${SEED_OWNER} (fresh stack)"
  elif [[ "$(rpc_error_code)" == "auth.setup_unavailable" ]]; then
    echo "==> instance already set up — auth.login as ${SEED_EMAIL}"
    if ! rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"auth.login","input":{"identifier":"'"$SEED_EMAIL"'","password":"'"$SEED_PASSWORD"'","remember_me":False}}))')"; then
      echo "FAIL: bootstrap unavailable and login rejected — point SMOKE_SEED_EMAIL /" >&2
      echo "SMOKE_SEED_PASSWORD at a verified account, or SMOKE_SEED_FIXTURES=0 for" >&2
      echo "routing/TCP-only depth." >&2
      exit 1
    fi
  else
    echo "FAIL: auth.bootstrap_setup failed unexpectedly" >&2
    exit 1
  fi

  echo "==> create public repo ${SEED_OWNER}/${SEED_REPO}"
  if rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"repo.create","input":{"name":"'"$SEED_REPO"'","visibility":"public","description":"protocol smoke","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}))')"; then
    echo "==> repo created"
  elif [[ "$(rpc_error_code)" == "repo.name_taken" ]]; then
    echo "==> repo already exists (reused stack) — continuing"
  else
    echo "FAIL: repo.create failed" >&2
    exit 1
  fi

  echo "==> create classic PAT (scopes: repo)"
  rpc '{"procedure":"pat.createClassic","input":{"name":"smoke-protocol","scopes":["repo"]}}'
  local pat
  pat="$(rpc_json_field token)"
  if [[ -z "$pat" || "$pat" == "None" ]]; then
    echo "FAIL: pat.createClassic did not return token" >&2
    exit 1
  fi
  export SMOKE_PAT="$pat"

  # Session cookie for sshKey.add / repo.lfs.setEnabled (value never printed).
  local session_cookie
  session_cookie="$(awk -F'\t' '$6 == "oxidean_session" {print $6"="$7}' "$COOKIE_JAR" | tail -1)"
  if [[ -z "$session_cookie" ]]; then
    echo "FAIL: no oxidean_session cookie after bootstrap/login" >&2
    exit 1
  fi
  export SMOKE_SESSION_COOKIE="$session_cookie"
  export SMOKE_SESSION="$session_cookie"
  export SMOKE_GIT_OWNER="$SEED_OWNER"
  export SMOKE_GIT_REPO="$SEED_REPO"
  echo "==> fixtures ready (session + PAT redacted)"
}

if [[ "${SMOKE_SEED_FIXTURES}" == "1" ]]; then
  seed_protocol_fixtures
else
  echo "==> SMOKE_SEED_FIXTURES=0 — routing/TCP-only depth (no seeded repo)"
fi

echo "==> make smoke-git-https (routing + ls-remote/push when seeded; SMOKE_SKIP_LS_REMOTE=${SMOKE_SKIP_LS_REMOTE})"
./scripts/smoke-git-https.sh

echo "==> make smoke-git-ssh (TCP + ls-remote/push when seeded; SMOKE_SKIP_LS_REMOTE=${SMOKE_SKIP_LS_REMOTE} SMOKE_SSH_PUSH=${SMOKE_SSH_PUSH:-0})"
./scripts/smoke-git-ssh.sh

echo "==> make smoke-git-lfs (routing; SMOKE_SKIP_LFS_CLIENT=${SMOKE_SKIP_LFS_CLIENT})"
./scripts/smoke-git-lfs.sh

echo "==> make smoke-packages (registry PathPrefix routing)"
./scripts/smoke-packages.sh

# DEBT-01: ORG-06 protected-push denial. Self-contained — re-ups a fresh stack
# (wipes volumes) to seed a repo; honors OXIDEAN_COMPOSE_SKIP_BUILD and maps
# SMOKE_SKIP_LS_REMOTE=1 to skipping its SSH half (HTTPS denial stays mandatory).
echo "==> make smoke-protection (ORG-06/D-PKG-03 protected-push denial; fresh stack)"
./scripts/compose-smoke-protection.sh

echo "==> ci-smoke-protocol OK"
