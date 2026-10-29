# Smoke Test Results — oxpilot

## CI-side smoke (executed)

| Check | Result |
|---|---|
| `e2e-stack` job — dev-auth stack: session create, POST `/api/oxpilot/chat` stream contract (append-or-snapshot), flag-off denial | executed by CI run on head SHA (see PR checks) |
| Web integration — 53 route/component tests + 20 stream unit | 73/73 pass (local evidence in build-and-test `test-results.md`) |
| `db-dialects` — `dialect_oxpilot` probe on PG/MySQL/SQLite | CI |
| `web-browser` — Chromium DOM-race tests | CI |

## Post-deploy smoke (operator runbook)

1. `GET /health` — api + web services return 200 (existing probes).
2. Flag-off env: `POST /api/oxpilot/chat` → denial response (expected; proves the gate).
3. Flag-on env (when enabled): create session → send with stub provider → confirm SSE snapshot/append frames and transcript persistence across reconnect.
4. Computer-use envs: connector handshake → grant list → exec_shell smoke behind consent.
