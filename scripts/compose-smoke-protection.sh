#!/usr/bin/env bash
# Compose ORG-06 protected-push smoke (D-PKG-01 / D-PKG-03) + GIT-21 protected tags.
# Brings up a fresh Compose stack (wipes project volumes), asserts the protection
# helper binary is executable, creates a reviews-required rule with
# enforce_admins, and expects an HTTPS push to the protected ref to fail.
# Also creates a protected tag ruleset (v*, deny all, enforce_admins) and expects
# an HTTPS tag push to fail while a non-matching tag push succeeds.
# When SSH TCP 2222 is reachable and SMOKE_SKIP_LS_REMOTE is unset, also
# expects an SSH push to the same protected ref to fail (D-PKG-03 SSH half).
#
# Env:
#   OXIDEAN_SMOKE_URL     default http://localhost
#   COMPOSE_FILE           default docker-compose.yml
#   SMOKE_REQUIRE_STACK    if 1 (or CI=true), fail closed when Docker missing
#   OXIDEAN_SSH_HOST      default localhost
#   OXIDEAN_SSH_PORT      default 2222
#   SMOKE_SKIP_LS_REMOTE   if 1, skip SSH denial branch (HTTPS remains mandatory)
#
# Operator hosts without Docker: exits 0 with a skip message (unless fail-closed).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
# shellcheck source=scripts/docker-wsl-creds.sh
source "${ROOT}/scripts/docker-wsl-creds.sh"
SMOKE_NAME="compose-smoke-protection"

BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"
COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.yml}"
OWNER="${SMOKE_PROTECT_OWNER:-protowner}"
REPO="${SMOKE_PROTECT_REPO:-protrepo}"
PASSWORD="${SMOKE_PROTECT_PASSWORD:-ProtectSmoke1!}"
SSH_HOST="${OXIDEAN_SSH_HOST:-localhost}"
SSH_PORT="${OXIDEAN_SSH_PORT:-2222}"

smoke_require_docker

if ! command -v git >/dev/null 2>&1; then
  echo "git not found on PATH; cannot run protection smoke" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 not found on PATH; cannot parse RPC JSON" >&2
  exit 1
fi

cleanup() {
  docker compose -f "$COMPOSE_FILE" down --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> docker compose down -v (clean volumes for bootstrap)"
docker compose -f "$COMPOSE_FILE" down -v --remove-orphans >/dev/null 2>&1 || true

echo "==> docker compose config"
docker compose -f "$COMPOSE_FILE" config >/dev/null

if [[ "${OXIDEAN_COMPOSE_SKIP_BUILD:-}" == "1" ]]; then
  echo "==> docker compose up -d --wait (OXIDEAN_COMPOSE_SKIP_BUILD=1)"
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
  echo "FAIL: health check failed at ${BASE_URL}/health" >&2
  docker compose -f "$COMPOSE_FILE" ps >&2 || true
  exit 1
fi

echo "==> assert protection helper executable in API image"
docker compose -f "$COMPOSE_FILE" exec -T api test -x /usr/local/bin/oxidean-protection-hook
echo "==> helper OK"

COOKIE_JAR="$(mktemp)"
RPC_OUT="$(mktemp)"
WORK="$(mktemp -d)"
# shellcheck disable=SC2064
trap "rm -f '$COOKIE_JAR' '$RPC_OUT'; rm -rf '$WORK'; cleanup" EXIT

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

rpc_json_field() {
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print(d["data"][sys.argv[2]])' "$RPC_OUT" "$1"
}

echo "==> bootstrap verified admin (${OWNER})"
rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"auth.bootstrap_setup","input":{"email":"'"$OWNER"'@example.com","username":"'"$OWNER"'","password":"'"$PASSWORD"'","allow_signup":False,"provider_mode":"local"}}))')"

echo "==> create public repo ${OWNER}/${REPO}"
rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"repo.create","input":{"name":"'"$REPO"'","visibility":"public","description":"protection smoke","stack_id":"rust","license_id":"MIT","gitignore_id":"Rust"}}))')"

echo "==> create classic PAT"
rpc '{"procedure":"pat.createClassic","input":{"name":"protect-smoke","scopes":["repo"]}}'
PAT="$(rpc_json_field token)"
if [[ -z "$PAT" || "$PAT" == "None" ]]; then
  echo "FAIL: pat.createClassic did not return token" >&2
  exit 1
fi

echo "==> create branch protection (reviews + enforce_admins) on main"
rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"repo.branchProtection.create","input":{"owner":"'"$OWNER"'","name":"'"$REPO"'","pattern":"main","require_reviews":True,"required_approving_review_count":1,"enforce_admins":True}}))')"

AUTH_HOST="${BASE_URL#http://}"
AUTH_HOST="${AUTH_HOST#https://}"
AUTH_URL="http://git:${PAT}@${AUTH_HOST}/${OWNER}/${REPO}.git"

echo "==> clone + attempt protected push to main (expect denial)"
git -C "$WORK" clone -q "$AUTH_URL" repo
git -C "$WORK/repo" config user.email "protect-smoke@localhost"
git -C "$WORK/repo" config user.name "protect-smoke"
echo "protect-smoke $(date -u +%Y%m%dT%H%M%SZ)" >>"$WORK/repo/PROTECT_SMOKE.txt"
git -C "$WORK/repo" add PROTECT_SMOKE.txt
git -C "$WORK/repo" commit -q -m "protect-smoke direct push"

set +e
push_out="$(GIT_TERMINAL_PROMPT=0 git -C "$WORK/repo" push origin HEAD:refs/heads/main 2>&1)"
push_rc=$?
set -e

if [[ "$push_rc" -eq 0 ]]; then
  echo "FAIL: HTTPS push to protected main succeeded (expected denial)" >&2
  echo "$push_out" >&2
  exit 1
fi
echo "==> HTTPS push denied as expected (rc=$push_rc)"
echo "$push_out" | head -20 || true

echo "==> GIT-21: create protected tag ruleset v* (deny all, enforce_admins)"
rpc "$(python3 -c 'import json; print(json.dumps({"procedure":"repo.tagProtection.create","input":{"owner":"'"$OWNER"'","name":"'"$REPO"'","pattern":"v*","enforce_admins":True}}))')"

echo "==> GIT-21: non-matching tag push allowed (control)"
git -C "$WORK/repo" tag smoke-unprotected
git -C "$WORK/repo" push -q origin refs/tags/smoke-unprotected
echo "==> unprotected tag pushed OK"

echo "==> GIT-21: push protected tag refs/tags/vsmoke-protected (expect denial)"
git -C "$WORK/repo" tag vsmoke-protected
set +e
tag_push_out="$(GIT_TERMINAL_PROMPT=0 git -C "$WORK/repo" push origin refs/tags/vsmoke-protected 2>&1)"
tag_push_rc=$?
set -e
if [[ "$tag_push_rc" -eq 0 ]]; then
  echo "FAIL: HTTPS push of protected tag succeeded (expected denial)" >&2
  echo "$tag_push_out" >&2
  exit 1
fi
echo "==> HTTPS protected tag push denied as expected (rc=$tag_push_rc)"
echo "$tag_push_out" | head -20 || true

# --- D-PKG-03 SSH half: optional when TCP published and not skip-flagged ---
ssh_skip_reason=""
if [[ "${SMOKE_SKIP_LS_REMOTE:-0}" == "1" ]]; then
  ssh_skip_reason="SMOKE_SKIP_LS_REMOTE=1"
elif ! command -v ssh >/dev/null 2>&1; then
  ssh_skip_reason="ssh client not on PATH"
elif ! command -v ssh-keygen >/dev/null 2>&1; then
  ssh_skip_reason="ssh-keygen not on PATH"
else
  echo "==> probe SSH TCP ${SSH_HOST}:${SSH_PORT}"
  tcp_ok=0
  if timeout 3 bash -c "echo >/dev/tcp/${SSH_HOST}/${SSH_PORT}" 2>/dev/null; then
    tcp_ok=1
  elif command -v nc >/dev/null 2>&1 && nc -z -w 3 "${SSH_HOST}" "${SSH_PORT}" 2>/dev/null; then
    tcp_ok=1
  fi
  if [[ "$tcp_ok" -ne 1 ]]; then
    ssh_skip_reason="SSH TCP ${SSH_HOST}:${SSH_PORT} unreachable"
  fi
fi

if [[ -n "$ssh_skip_reason" ]]; then
  echo "==> skip SSH protected-push denial ($ssh_skip_reason); HTTPS denial remains mandatory"
else
  echo "==> SSH protected-push denial (D-PKG-03)"
  identity="${WORK}/id_ed25519"
  ssh-keygen -t ed25519 -N "" -f "$identity" -C "protect-smoke-ssh@oxidean" -q
  pub="$(cat "${identity}.pub")"
  pub_json=$(printf '%s' "$pub" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
  rpc "$(printf '{"procedure":"sshKey.add","input":{"title":"protect-smoke-ssh","public_key":%s}}' "$pub_json")"
  echo "==> SSH key registered"

  GIT_SSH_URL="git@${SSH_HOST}:${OWNER}/${REPO}.git"
  export GIT_SSH_COMMAND="ssh -i ${identity} -p ${SSH_PORT} -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=${WORK}/known_hosts"

  set +e
  ssh_push_out="$(GIT_TERMINAL_PROMPT=0 git -C "$WORK/repo" push "${GIT_SSH_URL}" HEAD:refs/heads/main 2>&1)"
  ssh_push_rc=$?
  set -e

  if [[ "$ssh_push_rc" -eq 0 ]]; then
    echo "FAIL: SSH push to protected main succeeded (expected denial)" >&2
    echo "$ssh_push_out" >&2
    exit 1
  fi
  echo "==> SSH push denied as expected (rc=$ssh_push_rc)"
  echo "$ssh_push_out" | head -20 || true

  set +e
  ssh_tag_out="$(GIT_TERMINAL_PROMPT=0 git -C "$WORK/repo" push "${GIT_SSH_URL}" refs/tags/vsmoke-protected 2>&1)"
  ssh_tag_rc=$?
  set -e
  if [[ "$ssh_tag_rc" -eq 0 ]]; then
    echo "FAIL: SSH push of protected tag succeeded (expected denial)" >&2
    echo "$ssh_tag_out" >&2
    exit 1
  fi
  echo "==> SSH protected tag push denied as expected (rc=$ssh_tag_rc)"
  echo "$ssh_tag_out" | head -20 || true
fi

if [[ -n "$ssh_skip_reason" ]]; then
  echo "==> compose-smoke-protection OK (helper present + HTTPS protected branch/tag push denied; SSH skipped: ${ssh_skip_reason})"
else
  echo "==> compose-smoke-protection OK (helper present + HTTPS and SSH protected branch/tag push denied)"
fi
