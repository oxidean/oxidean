#!/usr/bin/env bash
# Compose Git-over-SSH smoke (GIT-03 / D-SSH-02 / D-SSH-07).
# Asserts TCP SSH on OXIDEAN_SSH_PORT, then optionally git ls-remote / push
# over scp-style git@host:owner/repo.git (not ssh:// primary).
#
# Prerequisites:
#   - Docker Compose stack up (`make up`) with OXIDEAN_SSH_ENABLED on api
#   - `git` + `ssh` + `ssh-keygen` on PATH
#   - Public repo SMOKE_GIT_OWNER/SMOKE_GIT_REPO (same as HTTPS smoke)
#   - SSH public key registered for that owner:
#       * SMOKE_SESSION_COOKIE — register ephemeral key via sshKey.add, or
#       * SMOKE_SSH_IDENTITY — path to private key whose pubkey is already registered
#
# Env knobs:
#   OXIDEAN_SMOKE_URL     default http://localhost (health check via Traefik)
#   OXIDEAN_SSH_HOST      default localhost
#   OXIDEAN_SSH_PORT      default 2222
#   SMOKE_GIT_OWNER        default smokeowner
#   SMOKE_GIT_REPO          default smokerepo
#   SMOKE_SESSION_COOKIE   session cookie (e.g. oxidean_session=…) for sshKey.add
#   SMOKE_SSH_IDENTITY     private key path (skips keygen + register)
#   SMOKE_SKIP_LS_REMOTE   if 1, only assert TCP listen (no git client).
#                          Also skips the SSH denial branch in compose-smoke-protection.sh
#                          (D-PKG-03); HTTPS denial there remains mandatory.
#   SMOKE_SSH_PUSH         if 1, also push a throwaway ref (needs write + verified email)
#
# CI note (DEBT-11): ci-smoke-protocol.sh seeds the owner/repo + session and
# exports SMOKE_SESSION_COOKIE + SMOKE_SSH_PUSH=1, so this script runs live
# ls-remote/push in the smoke-protocol job (SMOKE_SEED_FIXTURES=0 → TCP only).
#
# Operator hosts without Docker: exits 0 with a skip message.
# CI=true or SMOKE_REQUIRE_STACK=1 fails closed (T-11.1-40 / D-QH-04).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
SMOKE_NAME="smoke-git-ssh"

BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"
SSH_HOST="${OXIDEAN_SSH_HOST:-localhost}"
SSH_PORT="${OXIDEAN_SSH_PORT:-2222}"
OWNER="${SMOKE_GIT_OWNER:-smokeowner}"
REPO="${SMOKE_GIT_REPO:-smokerepo}"
# scp-style (D-SSH-02); Port via GIT_SSH_COMMAND -p when ≠ 22
GIT_SSH_URL="git@${SSH_HOST}:${OWNER}/${REPO}.git"

if ! command -v git >/dev/null 2>&1; then
  echo "git not found on PATH; cannot run git SSH smoke" >&2
  exit 1
fi
if ! command -v ssh >/dev/null 2>&1; then
  echo "ssh not found on PATH; cannot run git SSH smoke" >&2
  exit 1
fi

smoke_require_docker

echo "==> wait for ${BASE_URL}/health"
ok=0
for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null "${BASE_URL}/health" 2>/dev/null; then
    ok=1
    break
  fi
  sleep 2
done
if [[ "$ok" -ne 1 ]]; then
  echo "health check failed at ${BASE_URL}/health — is the stack up? (make up)" >&2
  exit 1
fi

echo "==> probe SSH TCP ${SSH_HOST}:${SSH_PORT}"
tcp_ok=0
if timeout 3 bash -c "echo >/dev/tcp/${SSH_HOST}/${SSH_PORT}" 2>/dev/null; then
  tcp_ok=1
elif command -v nc >/dev/null 2>&1 && nc -z -w 3 "${SSH_HOST}" "${SSH_PORT}" 2>/dev/null; then
  tcp_ok=1
fi
if [[ "$tcp_ok" -ne 1 ]]; then
  echo "FAIL: nothing listening on ${SSH_HOST}:${SSH_PORT} — is OXIDEAN_SSH_ENABLED set and ports published?" >&2
  exit 1
fi
echo "==> TCP ${SSH_PORT} reachable"

if [[ "${SMOKE_SKIP_LS_REMOTE:-0}" == "1" ]]; then
  echo "==> SMOKE_SKIP_LS_REMOTE=1 — skipping git ls-remote"
  echo "==> smoke-git-ssh OK (TCP only)"
  exit 0
fi

workdir="$(mktemp -d)"
# shellcheck disable=SC2064
trap "rm -rf '$workdir'" EXIT

identity=""
if [[ -n "${SMOKE_SSH_IDENTITY:-}" ]]; then
  identity="${SMOKE_SSH_IDENTITY}"
  if [[ ! -f "$identity" ]]; then
    echo "SMOKE_SSH_IDENTITY not found: $identity" >&2
    exit 1
  fi
else
  if ! command -v ssh-keygen >/dev/null 2>&1; then
    echo "ssh-keygen not found; set SMOKE_SSH_IDENTITY or install OpenSSH" >&2
    exit 1
  fi
  identity="${workdir}/id_ed25519"
  ssh-keygen -t ed25519 -N "" -f "$identity" -C "smoke-git-ssh@oxidean" -q
  pub="$(cat "${identity}.pub")"
  if [[ -z "${SMOKE_SESSION_COOKIE:-}" ]]; then
    echo "No SMOKE_SESSION_COOKIE — cannot register ephemeral key via sshKey.add." >&2
    echo "Set SMOKE_SESSION_COOKIE (verified session) or SMOKE_SSH_IDENTITY for a pre-registered key." >&2
    echo "Public key to register manually:" >&2
    echo "$pub" >&2
    exit 1
  fi
  echo "==> sshKey.add via session cookie"
  # Escape pubkey for JSON (spaces OK inside quotes).
  pub_json=$(printf '%s' "$pub" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
  rpc_body=$(printf '{"procedure":"sshKey.add","input":{"title":"smoke-git-ssh","public_key":%s}}' "$pub_json")
  rpc_out="$(mktemp)"
  http_code="$(
    curl -sS -o "$rpc_out" -w "%{http_code}" \
      -H "content-type: application/json" \
      -H "Oxidean-RPC-Version: 1" \
      -H "Cookie: ${SMOKE_SESSION_COOKIE}" \
      -d "$rpc_body" \
      "${BASE_URL}/api/rpc" || true
  )"
  if [[ "$http_code" != "200" ]] || grep -q '"error"' "$rpc_out" 2>/dev/null; then
    echo "sshKey.add failed (http=$http_code)" >&2
    head -c 500 "$rpc_out" >&2 || true
    echo >&2
    exit 1
  fi
  echo "==> key registered"
fi

export GIT_SSH_COMMAND="ssh -i ${identity} -p ${SSH_PORT} -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=${workdir}/known_hosts"

echo "==> git ls-remote ${GIT_SSH_URL}"
set +e
ls_out="$(GIT_TERMINAL_PROMPT=0 git ls-remote "${GIT_SSH_URL}" 2>&1)"
ls_rc=$?
set -e

if [[ "$ls_rc" -ne 0 ]]; then
  echo "git ls-remote failed (rc=$ls_rc)." >&2
  echo "$ls_out" >&2
  echo >&2
  echo "Prerequisite: public repo ${OWNER}/${REPO} and registered SSH key for that owner." >&2
  echo "TCP check already passed; re-run after the repo/key exist." >&2
  exit 1
fi
echo "$ls_out" | head -5
echo "==> ls-remote OK"

if [[ "${SMOKE_SSH_PUSH:-0}" == "1" ]]; then
  echo "==> optional push (SMOKE_SSH_PUSH=1)"
  work="${workdir}/push"
  mkdir -p "$work"
  git -C "$work" init -q
  git -C "$work" config user.email "smoke@localhost"
  git -C "$work" config user.name "smoke"
  echo "smoke $(date -u +%Y%m%dT%H%M%SZ)" >"$work/SMOKE.txt"
  git -C "$work" add SMOKE.txt
  git -C "$work" commit -q -m "smoke-git-ssh"
  GIT_TERMINAL_PROMPT=0 git -C "$work" push -q "${GIT_SSH_URL}" "+HEAD:refs/heads/smoke-ssh" \
    || {
      echo "git push over SSH failed (need owner + verified email)" >&2
      exit 1
    }
  echo "==> push OK (refs/heads/smoke-ssh)"
fi

echo "==> smoke-git-ssh OK"
