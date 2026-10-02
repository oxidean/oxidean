#!/usr/bin/env bash
# Compose Smart HTTP smoke (GIT-02 / D-18 / D-22).
# Asserts Traefik routes /{owner}/{repo}.git to the API (not SPA HTML), then
# optionally runs git ls-remote / push against a public repo.
#
# Prerequisites:
#   - Docker Compose stack up (`make up`) with Traefik on :80
#   - `git` on PATH
#   - For ls-remote: a public repo at SMOKE_GIT_OWNER/SMOKE_GIT_REPO (defaults below)
#     Create one via the UI/RPC after signup + email verify, or set env to an existing
#     public owner/repo on the running instance.
#   - Optional push: SMOKE_PAT=oxidean_pat_… (or oxidean_fg_…) + write access
#
# Env knobs:
#   OXIDEAN_SMOKE_URL   default http://localhost (must match Traefik Host)
#   SMOKE_GIT_OWNER      default smokeowner
#   SMOKE_GIT_REPO        default smokerepo
#   SMOKE_PAT            if set, also git push a throwaway ref (Basic auth username=git)
#   SMOKE_SKIP_LS_REMOTE  if 1, only assert non-HTML routing (no git client)
#
# CI note (DEBT-11): ci-smoke-protocol.sh seeds the repo + mints SMOKE_PAT, so
# ls-remote + throwaway-ref push run live in the smoke-protocol job
# (SMOKE_SEED_FIXTURES=0 → routing-only).
#
# Operator hosts without Docker: exits 0 with a skip message.
# CI=true or SMOKE_REQUIRE_STACK=1 fails closed (T-11.1-40 / D-QH-04).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# shellcheck source=scripts/smoke-lib.sh
source "${ROOT}/scripts/smoke-lib.sh"
SMOKE_NAME="smoke-git-https"

BASE_URL="${OXIDEAN_SMOKE_URL:-http://localhost}"
OWNER="${SMOKE_GIT_OWNER:-smokeowner}"
REPO="${SMOKE_GIT_REPO:-smokerepo}"
GIT_URL="${BASE_URL}/${OWNER}/${REPO}.git"
INFO_REFS="${GIT_URL}/info/refs?service=git-upload-pack"

if ! command -v git >/dev/null 2>&1; then
  echo "git not found on PATH; cannot run git HTTPS smoke" >&2
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

echo "==> Traefik .git routing (must not be SPA text/html): ${INFO_REFS}"
# Follow redirects manually disabled; capture headers + body snippet.
tmp_headers="$(mktemp)"
tmp_body="$(mktemp)"
trap 'rm -f "$tmp_headers" "$tmp_body"' EXIT

http_code="$(
  curl -sS -D "$tmp_headers" -o "$tmp_body" -w "%{http_code}" \
    -H "Host: localhost" \
    "$INFO_REFS" || true
)"
ct="$(grep -i '^content-type:' "$tmp_headers" | head -1 | tr -d '\r' || true)"
body_head="$(head -c 200 "$tmp_body" | tr '\n' ' ')"

if echo "$ct" | grep -qi 'text/html'; then
  echo "FAIL: .git path returned text/html (SPA stole Smart HTTP). content-type=$ct code=$http_code" >&2
  echo "body: $body_head" >&2
  exit 1
fi
if echo "$body_head" | grep -qiE '<(!doctype|html|script)'; then
  echo "FAIL: .git response body looks like HTML (code=$http_code ct=$ct)" >&2
  echo "body: $body_head" >&2
  exit 1
fi
echo "==> routing OK (http=$http_code content-type=${ct:-none})"

if [[ "${SMOKE_SKIP_LS_REMOTE:-0}" == "1" ]]; then
  echo "==> SMOKE_SKIP_LS_REMOTE=1 — skipping git ls-remote"
  echo "==> smoke-git-https OK (routing only)"
  exit 0
fi

echo "==> git ls-remote ${GIT_URL}"
# GIT_TERMINAL_PROMPT=0: never hang on credentials for private/missing repos.
set +e
ls_out="$(GIT_TERMINAL_PROMPT=0 git ls-remote "$GIT_URL" 2>&1)"
ls_rc=$?
set -e

if echo "$ls_out" | grep -qiE '<(!doctype|html|script)|text/html'; then
  echo "FAIL: git ls-remote output looks like HTML (SPA leak)" >&2
  echo "$ls_out" >&2
  exit 1
fi

if [[ "$ls_rc" -ne 0 ]]; then
  echo "git ls-remote failed (rc=$ls_rc)." >&2
  echo "$ls_out" >&2
  echo >&2
  echo "Prerequisite: public repo ${OWNER}/${REPO} must exist on the instance." >&2
  echo "Set SMOKE_GIT_OWNER / SMOKE_GIT_REPO, or create the repo after verified signup." >&2
  echo "Routing check already passed; re-run after the repo exists." >&2
  exit 1
fi
echo "$ls_out" | head -5
echo "==> ls-remote OK"

if [[ -n "${SMOKE_PAT:-}" ]]; then
  echo "==> optional push with SMOKE_PAT (username=git)"
  work="$(mktemp -d)"
  # shellcheck disable=SC2064
  trap "rm -rf '$work'; rm -f '$tmp_headers' '$tmp_body'" EXIT
  git -C "$work" init -q
  git -C "$work" config user.email "smoke@localhost"
  git -C "$work" config user.name "smoke"
  echo "smoke $(date -u +%Y%m%dT%H%M%SZ)" >"$work/SMOKE.txt"
  git -C "$work" add SMOKE.txt
  git -C "$work" commit -q -m "smoke-git-https"
  # Embed credentials in URL; do not print the secret.
  auth_url="http://git:${SMOKE_PAT}@${BASE_URL#http://}/${OWNER}/${REPO}.git"
  GIT_TERMINAL_PROMPT=0 git -C "$work" push -q "$auth_url" "+HEAD:refs/heads/smoke-https" \
    || {
      echo "git push with SMOKE_PAT failed" >&2
      exit 1
    }
  echo "==> push OK (refs/heads/smoke-https)"
fi

echo "==> smoke-git-https OK"
