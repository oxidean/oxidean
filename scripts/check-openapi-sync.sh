#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
make openapi-gen
git diff --exit-code -- docs/openapi.yaml
echo "openapi-sync-check: ok"
