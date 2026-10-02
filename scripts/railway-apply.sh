#!/usr/bin/env bash
# Preview or apply the Oxidean Cloud Railway IaC (.railway/railway.ts) from an
# operator machine. Preview (`railway config plan`) is the default; mutation
# requires --apply, which keeps the CLI's own interactive confirmation as the
# human-verify gate and refuses to run under CI — IaC-apply credentials live
# on operator machines only, never in CI or the repo (D-CLOUD-07, DEBT-07).
#
# Usage:
#   scripts/railway-apply.sh                            # verify context + plan
#   scripts/railway-apply.sh --environment staging      # plan, asserting link
#   scripts/railway-apply.sh --apply                    # railway config apply
#   scripts/railway-apply.sh --environment production --apply
#
# Prerequisites:
#   - Railway CLI >= 5.42.1 and jq on PATH
#   - `cd .railway && npm ci` once per checkout (TypeScript IaC SDK)
#   - Auth on this machine only: `railway login`, or export RAILWAY_TOKEN
#     (project token scoped to one environment — preferred) /
#     RAILWAY_API_TOKEN (account/workspace token)
#   - `railway link --project <id> --environment <env>` — config plan/apply
#     take no --project/--environment flags; the link selects the target
#
# Docs: .railway/README.md (operator runbook), docs/DEPLOYMENT.md

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IAC_DIR="${REPO_ROOT}/.railway"
APPLY=0
EXPECT_ENV=""

die() {
  echo "error: $*" >&2
  exit 1
}

usage() {
  sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply) APPLY=1 ;;
    --environment | -e)
      shift
      [[ $# -gt 0 && "${1:0:2}" != "--" ]] || die "--environment requires a value"
      EXPECT_ENV="$1"
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *) die "unknown flag: $1 (see --help)" ;;
  esac
  shift
done

command -v railway >/dev/null 2>&1 || die "railway CLI not found — install CLI >= 5.42.1 (see .railway/README.md)"
command -v jq >/dev/null 2>&1 || die "jq is required"
[[ -f "${IAC_DIR}/railway.ts" ]] || die "no .railway/railway.ts at ${REPO_ROOT} — run from an Oxidean checkout"
[[ -d "${IAC_DIR}/node_modules/railway" ]] || die "IaC SDK not installed — run: cd .railway && npm ci"

cli_version="$(railway --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)"
if [[ -n "$cli_version" ]] && [[ "$(printf '%s\n5.42.1\n' "$cli_version" | sort -V | head -1)" != "5.42.1" ]]; then
  echo "warning: railway CLI ${cli_version} < 5.42.1 — TypeScript IaC requires >= 5.42.1" >&2
fi

# Auth: token env vars win; otherwise require a working CLI login. Tokens stay
# on this machine — never write them to files, CI secrets, or IaC source.
if [[ -n "${RAILWAY_TOKEN:-}" ]]; then
  echo "auth: RAILWAY_TOKEN (project token)"
elif [[ -n "${RAILWAY_API_TOKEN:-}" ]]; then
  echo "auth: RAILWAY_API_TOKEN (account/workspace token)"
elif railway whoami --json >/dev/null 2>&1; then
  echo "auth: railway login ($(railway whoami --json | jq -r '.name // .email // "ok"' 2>/dev/null))"
else
  die "not authenticated — run railway login, or export RAILWAY_TOKEN / RAILWAY_API_TOKEN on this machine"
fi

# Resolve the linked context before any plan/apply so the operator (and
# --environment) can catch a wrong target. --json fails instead of prompting
# when nothing is linked.
status_json="$(railway status --json 2>/dev/null || true)"
[[ -n "$status_json" ]] || die "no linked project/environment — run: railway link --project <id> --environment <env>"
proj="$(echo "$status_json" | jq -r '(.project | if type=="object" then .name else . end) // (.linkedProject.project | if type=="object" then .name else . end) // empty')"
env_name="$(echo "$status_json" | jq -r '(.environment | if type=="object" then .name else . end) // (.linkedProject.environment | if type=="object" then .name else . end) // empty')"
[[ -n "$proj$env_name" ]] || die "could not parse railway status — run railway status and confirm the link manually"
echo "linked: project=${proj:-?} environment=${env_name:-?}"
if [[ -n "$EXPECT_ENV" ]]; then
  [[ "$env_name" == "$EXPECT_ENV" ]] || die "linked environment '${env_name}' != --environment '${EXPECT_ENV}' — relink: railway link --environment ${EXPECT_ENV}"
fi

cd "$REPO_ROOT"

if [[ "$APPLY" != "1" ]]; then
  echo "==> railway config plan (preview only — re-run with --apply to mutate)"
  exec railway config plan
fi

[[ -z "${CI:-}" ]] || die "refusing to apply under CI — railway config apply is an operator-machine action only"

echo "==> railway config apply — project=${proj:-?} environment=${env_name:-?}"
echo "    The CLI prints the plan and asks for confirmation; answer 'no' if anything looks off."
railway config apply

cat <<'EOF'

Applied. IaC cannot set the following — finish them in the dashboard:
  - Deploy triggers: staging = autodeploy on main + Wait for CI;
    preview/production = GitHub connected, autodeploy OFF.
    Production verify: make cloud-production-autodeploy-check
  - preserve() variables per environment: OXIDEAN_ENV, OXIDEAN_ACTIONS_SECRETS_KEY,
    OXIDEAN_VITE_ALLOWED_HOSTS (web), OXIDEAN_RUNNER_REGISTRATION_TOKEN (api+runner),
    email/SSO keys — see .railway/README.md
  - Public domain on the gateway service (advertise vars reference its domain)
EOF
